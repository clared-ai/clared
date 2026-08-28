import asyncio
from contextlib import asynccontextmanager
from typing import Any, Dict, List, Optional
import httpx


class ClaredSession:
    def __init__(
        self,
        sidecar_url: str,
        session_id: str,
        capability_token: str,
        generation: int = 1,
    ):
        self.sidecar_url = sidecar_url
        self.session_id = session_id
        self.capability_token = capability_token
        self.generation = generation
        self.client = httpx.AsyncClient(base_url=sidecar_url, timeout=10.0)

    async def seal(self) -> Dict[str, Any]:
        """Seals the active execution session and commits staged writes."""
        resp = await self.client.post(
            "/",
            json={
                "jsonrpc": "2.0",
                "id": "seal_req",
                "method": "intent/seal",
                "params": {
                    "session_id": self.session_id,
                    "capability_token": self.capability_token,
                },
            },
        )
        return resp.json()

    async def abort(self, reason: str = "Client workflow error") -> Dict[str, Any]:
        """Aborts the session and rolls back uncommitted holds."""
        resp = await self.client.post(
            "/",
            json={
                "jsonrpc": "2.0",
                "id": "abort_req",
                "method": "intent/abort",
                "params": {
                    "session_id": self.session_id,
                    "capability_token": self.capability_token,
                    "reason": reason,
                },
            },
        )
        return resp.json()

    async def close(self):
        await self.client.aclose()


class ClaredHarness:
    def __init__(self, sidecar_url: str = "http://localhost:4000"):
        self.sidecar_url = sidecar_url

    @asynccontextmanager
    async def session(
        self,
        tenant_id: str,
        principal: str,
        agent_role: str,
        task_intent: str,
        target_resources: List[str],
        allowed_tools: List[str],
        budget: Optional[Dict[str, int]] = None,
        ttl_ms: int = 30000,
    ):
        """Opens a bounded capability session with the Clared sidecar."""
        budgets = budget or {
            "money.minor.USD.hold": 50000,
            "money.minor.USD.capture": 50000,
            "database.mutations.count": 10,
        }

        async with httpx.AsyncClient(base_url=self.sidecar_url, timeout=5.0) as client:
            resp = await client.post(
                "/",
                json={
                    "jsonrpc": "2.0",
                    "id": "propose_req",
                    "method": "intent/propose",
                    "params": {
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
            data = resp.json()
            if "error" in data:
                raise RuntimeError(f"Clared capability proposal rejected: {data['error']}")

            res = data.get("result", {})
            session_id = res.get("session_id", "ses_local")
            token = res.get("capability_token", "tok_local")
            gen = res.get("generation", 1)

        sess = ClaredSession(
            sidecar_url=self.sidecar_url,
            session_id=session_id,
            capability_token=token,
            generation=gen,
        )
        try:
            yield sess
            await sess.seal()
        except Exception as e:
            await sess.abort(reason=str(e))
            raise
        finally:
            await sess.close()


def protect_agent(
    agent_workflow: Any,
    sidecar_url: str = "http://localhost:4000",
    budget: Optional[Dict[str, int]] = None,
    allowed_tools: Optional[List[str]] = None,
    target_resources: Optional[List[str]] = None,
):
    """Wraps an existing LangGraph or Python callable in a Clared capability boundary."""
    harness = ClaredHarness(sidecar_url=sidecar_url)

    class ProtectedAgent:
        def __init__(self, inner):
            self.inner = inner

        async def invoke(self, inputs: Dict[str, Any], **kwargs) -> Any:
            async with harness.session(
                tenant_id=inputs.get("tenant_id", "default_org"),
                principal=inputs.get("user_id", "system"),
                agent_role="autonomous_agent",
                task_intent="automated_task",
                target_resources=target_resources or [],
                allowed_tools=allowed_tools or [],
                budget=budget,
            ) as session:
                # Attach capability context to inputs if supported
                if isinstance(inputs, dict):
                    inputs["_dtbe_meta"] = {
                        "session_id": session.session_id,
                        "capability_token": session.capability_token,
                        "generation": session.generation,
                    }
                if asyncio.iscoroutinefunction(self.inner.invoke):
                    return await self.inner.invoke(inputs, **kwargs)
                else:
                    return self.inner.invoke(inputs, **kwargs)

    return ProtectedAgent(agent_workflow)
