# Clared

Commit-time authorization and safe-execution proxy for AI agents.

Clared sits between agent frameworks (such as LangGraph or CrewAI) and target tools. It intercepts tool calls, checks Cedar policies in memory, manages database transaction savepoints, reserves API holds, and settles multi-step operations safely.

## Why this exists

Prompt guardrails check what an agent says. Clared checks what an agent does at the moment of execution.

If an agent attempts three operations (such as a database edit, a Stripe refund, and a customer email) and crashes on step two, standard setups leave the database updated and the payment in limbo. Clared holds changes in staging, commits the database only when the workflow succeeds, and cancels uncommitted holds if the run aborts.

## Key mechanics

- **In-memory policy evaluation**: Evaluates AWS Cedar policies in native Rust memory before requests touch the network.
- **Connection-pinned database transactions**: Runs database writes inside a pinned `BEGIN ... SAVEPOINT` transaction, giving the agent read-your-own-writes visibility while protecting the main database from unsealed changes.
- **Two-phase reservations**: Uses API hold features (such as `capture_method=manual` in Stripe) to verify parameters and obtain IDs without moving money until seal time.
- **Write-ahead log for budgets**: Tracks multi-dimensional budgets in minor units (`money.minor.USD: 50000`) on a local write-ahead log to survive container crashes and prevent over-spending.
- **Topological sink buffering**: Delays notification tools (SMS, email, webhooks) in memory until all database writes and payment captures succeed.

## Quick start

### 1. Python / LangGraph wrapper

Install the SDK:

```bash
pip install clared
```

Wrap an existing agent workflow:

```python
from clared import protect_agent
from my_agent import billing_graph

# Protect an existing LangGraph instance
safe_agent = protect_agent(
    billing_graph,
    sidecar_url="http://localhost:4000",
    budget={
        "money.minor.USD.capture": 50000,
        "database.mutations.count": 5
    },
    allowed_tools=[
        "stripe.payment_intents.refund",
        "postgres.orders.update"
    ]
)

# Run the agent normally
result = await safe_agent.invoke({"dispute_id": "1042"})
```

### 2. Standalone MCP proxy (CLI)

Run Clared in shadow mode to log policy evaluations without blocking actions:

```bash
clared-guard --mode shadow --policy-file ./policies.cedar -- npx @modelcontextprotocol/server-postgres
```

Run in enforcement mode:

```bash
clared-guard --mode enforce --policy-file ./policies.cedar -- npx @modelcontextprotocol/server-postgres
```

## Example policy (`policies.cedar`)

```cedar
// Forbid refunds over $500 without manager flag
forbid (
    principal in Role::"AutonomousAgent",
    action == Action::"stripe.payment_intents.refund",
    resource
)
when {
    context.amount_minor > 50000 && !context.has_manager_approval
};

// Ensure tenant isolation
permit (
    principal,
    action,
    resource
)
when {
    principal.tenant_id == resource.tenant_id
};
```

## Architecture

```
Agent Runtime (LangGraph / Python)
          │
          │ (AIP Protocol / Stdio / HTTP)
          ▼
┌────────────────────────────────────────┐
│             Clared Guard               │
│                                        │
│  - Cedar Policy DAG (<0.2ms check)     │
│  - NVMe Write-Ahead Log (WAL)          │
│  - OpenAdapter Tool Staging            │
└──────────────────┬─────────────────────┘
                   │
         ┌─────────┴─────────┐
         ▼                   ▼
   PostgreSQL DB       Stripe / REST APIs
 (Pinned Connection)   (Auth-and-Hold / Sinks)
```

## Development

Build the Rust gateway from source:

```bash
git clone https://github.com/clared-ai/clared.git
cd clared/clared-core
cargo build --release
cargo test
```

## License

Apache-2.0
