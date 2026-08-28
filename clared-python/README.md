# Clared Python SDK

Python client and LangGraph / CrewAI middleware for Clared execution firewalls.

## Installation

```bash
pip install -e .
```

## Usage

```python
from clared import protect_agent

# Wrap an existing LangGraph or Python workflow
safe_agent = protect_agent(
    my_agent_graph,
    sidecar_url="http://localhost:4000",
    budget={"money.minor.USD.capture": 50000},
    allowed_tools=["stripe.refund", "postgres.orders.update"]
)
```
