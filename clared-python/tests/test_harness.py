import pytest
from clared.harness import ClaredHarness, protect_agent


def test_sdk_imports():
    harness = ClaredHarness()
    assert harness.sidecar_url == "http://localhost:4000"


@pytest.mark.asyncio
async def test_protect_agent_wrapper():
    class MockGraph:
        def invoke(self, inputs):
            return {"result": "processed", "data": inputs}

    graph = MockGraph()
    protected = protect_agent(
        graph,
        budget={"money.minor.USD.capture": 50000},
        allowed_tools=["stripe.refund"]
    )
    assert protected is not None
