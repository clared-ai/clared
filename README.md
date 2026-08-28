# Clared

> **Status:** Experimental reference implementation for safe multi-step agent execution. Seeking reviews, fault-injection attacks, and feedback.

Clared is an execution proxy and middleware for autonomous AI agents. It intercepts tool calls, evaluates policy invariants in memory, manages database transaction savepoints, reserves API holds, and settles multi-step operations safely.

## The core problem: Multi-step execution safety

Most agent authorization tools check permissions per call. But in multi-step workflows, **individually authorized actions can still produce an unsafe aggregate outcome.**

Consider an agent resolving a customer dispute:
1. **Step 1**: It queries an order database (allowed).
2. **Step 2**: It updates the database record to `status = 'refunded'` (allowed).
3. **Step 3**: It calls Stripe to issue a $500 refund (allowed).
4. **Step 4**: It calls Twilio to send a confirmation SMS (allowed).

If the model crashes, gets injected, or hits a network partition on Step 3, the database is updated, the refund is unissued, and the user receives no notification.

Traditional gateways cannot fix this because they do not coordinate state across steps.

## How Clared works

Clared manages the entire agent trajectory as a bounded execution session:

```
Agent Runtime (LangGraph / Python)
          │
          │ 1. `intent/propose` (Declares aggregate budgets & resource targets)
          ▼
┌────────────────────────────────────────────────────────┐
│                      Clared Proxy                      │
│                                                        │
│  - In-Memory Policy Evaluation (<0.2ms)                │
│  - Minor-Unit Integer Budgets (money.minor.USD: 50000) │
│  - Generation Fencing (`gen: 1 -> gen: 2`)             │
└──────────────┬──────────────────────────┬──────────────┘
               │                          │
   2. Tools Call (Staging)    3. `intent/seal` (Settlement)
               ▼                          ▼
┌──────────────────────────────┐   ┌────────────────────────────────┐
│       STAGING PRIMITIVES     │   │     COORDINATED SETTLEMENT     │
│ • Database: Pinned Tx BEGIN  │──►│ 1. Capture Stripe Hold         │
│ • Stripe: Auth Hold (Manual) │   │ 2. Commit Database Tx          │
│ • Twilio: RAM Buffer         │   │ 3. Flush Buffered Notifications│
└──────────────────────────────┘   └────────────────────────────────┘
```

1. **Aggregate multi-dimensional budgets**: Tracks integer minor-unit limits (such as `50000` for $500.00) in memory. Floating-point values are prohibited.
2. **Connection-pinned database transactions (Mode 1)**: Pins a database connection and executes `BEGIN ... SAVEPOINT`. The agent sees its own writes, while the production database remains protected until seal.
3. **Two-phase reservations (Mode 3)**: Uses API hold features (such as `capture_method=manual` in Stripe) to get genuine provider IDs without settling funds.
4. **Topological sink buffering**: Holds user-facing notifications (SMS, email, Slack) in RAM until all database writes and payment captures succeed.
5. **Explicit partial outcome reporting**: If an unrecoverable failure occurs during settlement, Clared marks the session as `PARTIALLY_SETTLED`, executes declared compensators, and emits a signed incident receipt.

---

## Repository layout

```
.
├── clared-core/           # Rust execution proxy & policy engine
├── clared-python/         # Python / LangGraph harness middleware
├── adapters/              # Declarative YAML adapters (Stripe, Postgres, Twilio)
├── docs/                  # Reference specifications
│   ├── execution-envelope-spec.md
│   └── settlement-adapters-spec.md
└── examples/              # End-to-end runnable integration examples
```

---

## Local development

### Prerequisites
- Rust 1.75+ (`cargo`)
- Python 3.10+

### Build the Rust proxy
```bash
cd clared-core
cargo build --release
cargo test
```

### Install the Python SDK locally
```bash
cd clared-python
pip install -e .
```

---

## Python / LangGraph middleware example

```python
from clared import protect_agent
from my_agent import billing_graph

# Wrap an existing LangGraph or Python callable
safe_agent = protect_agent(
    billing_graph,
    sidecar_url="http://localhost:4000",
    budget={
        "money.minor.USD.capture": 50000,   # $500.00 max
        "database.mutations.count": 5
    },
    allowed_tools=[
        "stripe.payment_intents.refund",
        "postgres.orders.update"
    ],
    target_resources=["customer:cus_9918"]
)

# Run the agent normally
result = await safe_agent.invoke({"dispute_id": "1042"})
```

---

## Specifications

- [Execution Envelope Specification](docs/execution-envelope-spec.md)
- [Settlement Adapters Specification](docs/settlement-adapters-spec.md)

---

## License

Apache-2.0. See [LICENSE](LICENSE) for details.
