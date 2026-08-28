import base64
import hashlib
import hmac
import inspect
import os
import secrets
import time
import uuid
from contextlib import asynccontextmanager
from typing import Any, Dict, List, Optional, Union

import httpx


def create_delegation_token(
    secret: Union[str, bytes],
    *,
    tenant_id: str,
    principal: str,
    agent_role: str,
    task_intent: str,
    expires_at_ms: int,
) -> str:
    """Create the reference HMAC delegator proof expected by clared-core."""
    secret_bytes = secret.encode() if isinstance(secret, str) else secret
    if len(secret_bytes) < 32:
        raise ValueError("Delegation secret must contain at least 32 bytes")
    nonce = secrets.token_hex(12)
    fields = [tenant_id, principal, agent_role, task_intent, str(expires_at_ms), nonce]
    if any("\x1f" in field for field in fields):
        raise ValueError("Delegation fields may not contain the unit separator")
    message = "\x1f".join(fields).encode()
    digest = hmac.new(secret_bytes, message, hashlib.sha256).digest()
    encoded = base64.urlsafe_b64encode(digest).decode().rstrip("=")
    return f"clared-delegation-v1.{expires_at_ms}.{nonce}.{encoded}"


class ClaredSession:
    def __init__(
        self,
        sidecar_url: str,
        session_id: str,
        capability_token: str,
        generation: int,
        expires_at_ms: int,
        delegation_secret: Union[str, bytes],
        tenant_id: str,
        principal: str,
        agent_role: str,
        task_intent: str,
    ):
        self.sidecar_url = sidecar_url
        self.session_id = session_id
        self.capability_token = capability_token
        self.generation = generation
        self.expires_at_ms = expires_at_ms
        self.delegation_secret = delegation_secret
        self.tenant_id = tenant_id
        self.principal = principal
        self.agent_role = agent_role
        self.task_intent = task_intent
        self.client = httpx.AsyncClient(base_url=sidecar_url, timeout=10.0)
        self.settlement_receipt: Optional[Dict[str, Any]] = None
        self.abort_receipt: Optional[Dict[str, Any]] = None
        self._seal_idempotency_key = f"seal_{uuid.uuid4().hex}"
        self._abort_idempotency_key = f"abort_{uuid.uuid4().hex}"

    async def call_tool(
        self,
        name: str,
        arguments: Dict[str, Any],
        *,
        idempotency_key: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Route a tool request through the Clared execution envelope."""
        response = await self.client.post(
            "/",
            json={
                "jsonrpc": "2.0",
                "id": f"call_{uuid.uuid4().hex}",
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": arguments,
                    "_clared_meta": {
                        "session_id": self.session_id,
                        "capability_token": self.capability_token,
                        "generation": self.generation,
                        "idempotency_key": idempotency_key or f"tool_{uuid.uuid4().hex}",
                    },
                },
            },
        )
        response.raise_for_status()
        payload = response.json()
        if "error" in payload:
            raise RuntimeError(f"Clared tool call rejected: {payload['error']}")
        return payload["result"]

    async def amend(self, budget_additions: Dict[str, int]) -> Dict[str, Any]:
        """Amend budgets and fence the previous capability generation."""
        delegation_token = create_delegation_token(
            self.delegation_secret,
            tenant_id=self.tenant_id,
            principal=self.principal,
            agent_role=self.agent_role,
            task_intent=self.task_intent,
            expires_at_ms=self.expires_at_ms,
        )
        payload = await self._rpc(
            "intent/amend",
            {
                "session_id": self.session_id,
                "capability_token": self.capability_token,
                "delegation_token": delegation_token,
                "budget_additions": budget_additions,
            },
        )
        self.capability_token = payload["capability_token"]
        self.generation = payload["generation"]
        return payload

    async def seal(self, *, idempotency_key: Optional[str] = None) -> Dict[str, Any]:
        receipt = await self._rpc(
            "intent/seal",
            {
                "session_id": self.session_id,
                "capability_token": self.capability_token,
                "idempotency_key": idempotency_key or self._seal_idempotency_key,
            },
        )
        self.settlement_receipt = receipt
        return receipt

    async def abort(
        self,
        reason: str = "Client workflow error",
        *,
        idempotency_key: Optional[str] = None,
    ) -> Dict[str, Any]:
        receipt = await self._rpc(
            "intent/abort",
            {
                "session_id": self.session_id,
                "capability_token": self.capability_token,
                "idempotency_key": idempotency_key or self._abort_idempotency_key,
                "reason": reason,
            },
        )
        self.abort_receipt = receipt
        return receipt

    async def _rpc(self, method: str, params: Dict[str, Any]) -> Dict[str, Any]:
        response = await self.client.post(
            "/",
            json={
                "jsonrpc": "2.0",
                "id": f"req_{uuid.uuid4().hex}",
                "method": method,
                "params": params,
            },
        )
        response.raise_for_status()
        payload = response.json()
        if "error" in payload:
            raise RuntimeError(f"Clared {method} rejected: {payload['error']}")
        return payload["result"]

    async def close(self) -> None:
        await self.client.aclose()


class ClaredHarness:
    def __init__(
        self,
        sidecar_url: str = "http://127.0.0.1:4000",
        delegation_secret: Optional[Union[str, bytes]] = None,
    ):
        self.sidecar_url = sidecar_url
        self.delegation_secret = delegation_secret or os.environ.get(
            "CLARED_DELEGATION_SECRET"
        )
        if self.delegation_secret is None:
            raise ValueError("CLARED_DELEGATION_SECRET is required")
        secret_bytes = (
            self.delegation_secret.encode()
            if isinstance(self.delegation_secret, str)
            else self.delegation_secret
        )
        if len(secret_bytes) < 32:
            raise ValueError("CLARED_DELEGATION_SECRET must contain at least 32 bytes")

    @asynccontextmanager
    async def session(
        self,
        *,
        tenant_id: str,
        principal: str,
        agent_role: str,
        task_intent: str,
        target_resources: List[str],
        allowed_tools: List[str],
        budgets: Dict[str, int],
        ttl_ms: int = 30_000,
    ):
        delegation_token = create_delegation_token(
            self.delegation_secret,
            tenant_id=tenant_id,
            principal=principal,
            agent_role=agent_role,
            task_intent=task_intent,
            expires_at_ms=int(time.time() * 1000) + max(ttl_ms, 60_000),
        )
        async with httpx.AsyncClient(base_url=self.sidecar_url, timeout=5.0) as client:
            response = await client.post(
                "/",
                json={
                    "jsonrpc": "2.0",
                    "id": f"propose_{uuid.uuid4().hex}",
                    "method": "intent/propose",
                    "params": {
                        "delegation_token": delegation_token,
                        "tenant_id": tenant_id,
                        "principal": principal,
                        "agent_role": agent_role,
                        "task_intent": task_intent,
                        "target_resources": target_resources,
                        "allowed_tools": allowed_tools,
                        "budgets": budgets,
                        "ttl_ms": ttl_ms,
                    },
                },
            )
            response.raise_for_status()
            payload = response.json()
            if "error" in payload:
                raise RuntimeError(f"Clared proposal rejected: {payload['error']}")
            result = payload["result"]

        session = ClaredSession(
            sidecar_url=self.sidecar_url,
            session_id=result["session_id"],
            capability_token=result["capability_token"],
            generation=result["generation"],
            expires_at_ms=result["expires_at_ms"],
            delegation_secret=self.delegation_secret,
            tenant_id=tenant_id,
            principal=principal,
            agent_role=agent_role,
            task_intent=task_intent,
        )
        try:
            yield session
            await session.seal()
        except Exception as error:
            try:
                await session.abort(reason=str(error))
            except Exception:
                # Preserve the workflow exception if cleanup also fails.
                pass
            raise
        finally:
            await session.close()


def with_clared_session(
    agent_workflow: Any,
    *,
    sidecar_url: str = "http://127.0.0.1:4000",
    delegation_secret: Optional[Union[str, bytes]] = None,
    budgets: Dict[str, int],
    allowed_tools: List[str],
    target_resources: List[str],
):
    """Inject a ClaredSession into a workflow; it is not a sandbox by itself.

    A hard boundary requires withholding downstream credentials and routing every
    mutating tool through ``ClaredSession.call_tool``.
    """
    harness = ClaredHarness(sidecar_url, delegation_secret)

    class SessionBoundAgent:
        def __init__(self, inner: Any):
            self.inner = inner

        async def invoke(self, inputs: Dict[str, Any], **kwargs: Any) -> Any:
            async with harness.session(
                tenant_id=inputs.get("tenant_id", "default_org"),
                principal=inputs.get("user_id", "system"),
                agent_role="autonomous_agent",
                task_intent=inputs.get("task_intent", "automated_task"),
                target_resources=target_resources,
                allowed_tools=allowed_tools,
                budgets=budgets,
            ) as session:
                enriched_inputs = dict(inputs)
                enriched_inputs["clared_session"] = session
                result = self.inner.invoke(enriched_inputs, **kwargs)
                if inspect.isawaitable(result):
                    return await result
                return result

    return SessionBoundAgent(agent_workflow)
