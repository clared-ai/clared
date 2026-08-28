# Clared

[![CI](https://github.com/clared-ai/clared/actions/workflows/ci.yml/badge.svg)](https://github.com/clared-ai/clared/actions/workflows/ci.yml)

> **Status:** Experimental security reference implementation. The current backend is an in-memory simulator: it enforces the envelope protocol but does not contact databases, payment providers, or notification services.

Clared explores a specific failure mode in action-taking agents: every tool call can be individually authorized while the multi-step operation still produces an unsafe aggregate outcome.

An order agent might update a database, authorize a payment, and notify a customer. If a late step fails, a per-call gateway cannot by itself reconcile the state already created by earlier calls. Clared places the whole operation inside a bounded execution session, meters aggregate budgets, stages actions through declared adapters, revalidates policy at seal time, and reports the final outcome explicitly.

We are looking for teams whose agents mutate databases, payment systems, infrastructure, or external APIs across a single task. [Challenge the protocol in Discussions](https://github.com/clared-ai/clared/discussions), or email [liran@clared.ai](mailto:liran@clared.ai) to evaluate a concrete workflow privately.

## Run the fault-injection demo

The demo compares an unsafe workflow with the Clared reference simulator. No external accounts or API keys are required. It requires Rust and Python 3.10+; `uv` is used when available.

```bash
git clone https://github.com/clared-ai/clared.git
cd clared
make demo
```

Expected comparison:

```text
Unsafe baseline: inconsistent state
Clared failure path: ABORTED, 2 simulated actions reverted, 0 escaped
Clared success path: SETTLED with SHA-256 evidence and Ed25519 signature
```

## What is enforced now

| Boundary | Reference implementation |
| --- | --- |
| Delegation | Single-use HMAC-SHA256 proof binds tenant, principal, role, intent, expiry, and nonce |
| Capability | Short-lived Ed25519-signed token bound to one session and generation |
| Tool access | Fail-closed allowlist; every allowed tool must have a registered adapter |
| Resource scope | Adapter-declared argument types must match a qualified envelope target |
| Policy | Default-deny Cedar admission; reserved approval context cannot come from tool arguments |
| Aggregate budgets | Integer-only typed dimensions, including money, mutations, and notifications |
| Lifecycle | Expiry and terminal states are enforced; settled or aborted sessions cannot execute |
| Replay control | Tool, seal, and abort requests use scoped idempotency keys |
| Commit evidence | Deterministically serialized outcome evidence is SHA-256 hashed and Ed25519 signed |

Provider execution is deliberately simulated. Responses are labeled `in_memory_simulator` and use `SIMULATED_*` statuses. Real PostgreSQL, Stripe, and Twilio executors are future integration work.

## How the boundary fits together

```text
Trusted harness
  │  signed delegation proof
  ▼
intent/propose ──► Clared policy + envelope admission
  │                         │
  │                         └── Ed25519 session capability
  ▼
tools/call ─────► allowlist + scope + budget + Cedar + idempotency
  │
  ├── intent/abort ──► discard all simulated staged actions
  │
  └── intent/seal ───► revalidate policy, settle in declared order,
                       sign the outcome evidence
```

The agent should not possess downstream credentials. A production boundary requires every mutating path to terminate at the enforcing proxy; the Python helper alone is not a sandbox.

See the [threat model](docs/threat-model.md) for the trusted computing base, concrete attacks, implemented defenses, and residual risks.

## Where Clared fits

Clared is not a replacement for the systems below. It adds a stateful authority and settlement boundary around an agent-selected tool trajectory.

| Category | Primary job | What remains outside it that Clared targets |
| --- | --- | --- |
| Durable workflow engines (for example Temporal or Restate) | Reliably execute an application-defined workflow | Bound authority for a non-deterministic sequence selected by an agent |
| Policy engines (Cedar or OPA) | Decide whether one request is allowed from current facts | Track aggregate budgets and staged effects across calls, then revalidate at seal time |
| LLM gateways | Govern model traffic, cost, routing, and observability | Govern downstream mutations after the model chooses an action |
| Sandboxed runtimes | Isolate code, processes, files, or network access | Express which business effects may accumulate and how they settle |
| Tool gateways and MCP permission layers | Expose tools and approve or deny individual calls | Bind the whole multi-tool operation to one capability, resource set, budget, lifecycle, and receipt |

The proposed unit is the combination: session-scoped authority, aggregate typed budgets, adapter-declared staged effects, commit-time policy revalidation, and signed terminal evidence.

## Open specifications

The contracts are Apache-2.0 licensed and independently implementable.

| Specification | Governs | Status |
| --- | --- | --- |
| [Clared Execution Envelope](specs/execution-envelope.md) | Delegation, capabilities, budgets, resource scope, lifecycle, idempotency, and receipts | `v0alpha1` |
| [Clared Settlement Adapter](specs/settlement-adapters.md) | How a tool declares staging, settlement, rollback, resource extraction, and budget accounting | `v0alpha1` |

See [specs/README.md](specs/README.md) for versioning and contribution guidance.

## Python integration

The package is not published to PyPI yet. Install it from this repository while the `v0alpha1` API is still changing.

Use `ClaredSession.call_tool` for every mutating action:

```python
from clared import ClaredHarness

harness = ClaredHarness()

async with harness.session(
    tenant_id="acme",
    principal="alice",
    agent_role="checkout_agent",
    task_intent="authorize_order_1042",
    target_resources=["order:ord_1042", "customer:cus_9918"],
    allowed_tools=["postgres.orders.update", "stripe.payment_intents.create"],
    budgets={
        "database.mutations.count": 1,
        "money.minor.USD.hold": 50000,
        "money.minor.USD.capture": 50000,
    },
) as session:
    await session.call_tool(
        "postgres.orders.update",
        {"order_id": "ord_1042", "status": "payment_authorized"},
        idempotency_key="order-1042-update-v1",
    )
```

`with_clared_session` is a convenience that injects this client into a workflow. It cannot prevent bypass unless direct credentials and alternate egress paths are removed.

## Non-guarantees

Clared does not claim universal distributed ACID across arbitrary APIs. A future live executor will coordinate adapter-defined reservations, transactions, and compensators, but partial provider failures must still be represented as explicit degraded states. The current release proves the envelope and lifecycle mechanics against an in-memory simulator only.

See [ROADMAP.md](ROADMAP.md) for the evidence-gated path to an MCP-compatible shim, a real PostgreSQL transaction executor, conformance fixtures, and installable releases.

## Repository layout

```text
clared-core/       Rust JSON-RPC service, policy engine, capabilities, sessions
clared-python/     Explicit Python harness and tool client
adapters/          Versioned settlement adapter declarations
specs/             Open execution-envelope and adapter specifications
docs/              Threat model and security analysis
examples/          Runnable fault-injection comparison
```

## Development

```bash
make check
```

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md). Licensed under [Apache-2.0](LICENSE).
