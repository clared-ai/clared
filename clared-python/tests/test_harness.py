from contextlib import asynccontextmanager

import pytest

from clared.harness import (
    ClaredHarness,
    ClaredSession,
    create_delegation_token,
    with_clared_session,
)


SECRET = "0123456789abcdef0123456789abcdef"


def test_delegation_token_is_structured_and_secret_is_required():
    token = create_delegation_token(
        SECRET,
        tenant_id="acme",
        principal="alice",
        agent_role="checkout",
        task_intent="authorize_order",
        expires_at_ms=2_000_000_000_000,
    )
    assert token.startswith("clared-delegation-v1.")
    assert len(token.split(".")) == 4
    with pytest.raises(ValueError):
        ClaredHarness(delegation_secret="short")


@pytest.mark.asyncio
async def test_session_wrapper_injects_explicit_tool_client(monkeypatch):
    sentinel_session = object()

    @asynccontextmanager
    async def fake_session(self, **kwargs):
        yield sentinel_session

    monkeypatch.setattr(ClaredHarness, "session", fake_session)

    class MockGraph:
        def invoke(self, inputs):
            return inputs["clared_session"]

    wrapped = with_clared_session(
        MockGraph(),
        delegation_secret=SECRET,
        budgets={"database.mutations.count": 1},
        allowed_tools=["postgres.orders.update"],
        target_resources=["order:ord_1"],
    )
    assert await wrapped.invoke({"user_id": "alice"}) is sentinel_session


@pytest.mark.asyncio
async def test_terminal_calls_reuse_stable_idempotency_keys(monkeypatch):
    calls = []

    async def fake_rpc(method, params):
        calls.append((method, params))
        return {"status": "SETTLED" if method == "intent/seal" else "ABORTED"}

    session = ClaredSession(
        sidecar_url="http://127.0.0.1:4000",
        session_id="ses_test",
        capability_token="cap_test",
        generation=1,
        expires_at_ms=2_000_000_000_000,
        delegation_secret=SECRET,
        tenant_id="acme",
        principal="alice",
        agent_role="checkout",
        task_intent="authorize_order",
    )
    monkeypatch.setattr(session, "_rpc", fake_rpc)
    try:
        await session.seal()
        await session.seal()
        await session.abort()
        await session.abort()
    finally:
        await session.close()

    assert calls[0][1]["idempotency_key"] == calls[1][1]["idempotency_key"]
    assert calls[2][1]["idempotency_key"] == calls[3][1]["idempotency_key"]
