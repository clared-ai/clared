# Clared

[![CI](https://github.com/clared-ai/clared/actions/workflows/ci.yml/badge.svg)](https://github.com/clared-ai/clared/actions/workflows/ci.yml)

> **Status:** Experimental open-source reference implementation on an in-memory simulator. It enforces the execution envelope end-to-end but does not contact databases, payment providers, or notification services. See [Status: shipped vs target](#status-shipped-vs-target) for the exact split.

Clared's thesis: teams should be able to authorize agent workflows that are too risky to run today — the agent gets a broad tool surface inside a stateful, revocable execution session, without holding unrestricted downstream credentials. This repository proves the boundary mechanics on an in-memory simulator; wiring real providers is future integration work.

## The problem

Agents can usually do the work. Teams still cannot safely let them.

- **Useful access becomes broad access.** A flexible agent needs many tools and credentials. Once those credentials reach the agent runtime, one wrong branch can affect any reachable customer, account, or production resource.
- **Safe calls can form an unsafe run.** Every tool call can be individually authorized while the multi-step operation still breaches a cumulative budget, skips a required state transition, or proceeds after the underlying business state has changed. Per-call approval does not see the aggregate.
- **Real systems do not roll back together.** A database write, a payment authorization, and a notification settle differently. A late failure can leave earlier effects committed, forcing manual repair — which is exactly why risk reviews keep these agents read-only, narrowly scoped, or approved one click at a time.

The wider the action space, the less a risk review will approve. The blocker is not agent capability; it is blast radius.

## The approach

Clared decouples action space from blast radius. The agent stays free to choose its tool trajectory, but every mutating effect crosses a stateful execution boundary that:

- binds the whole run — not each call — to one signed delegation and session capability,
- meters cumulative typed budgets (money, mutations, notifications) across the run,
- stages provider actions through declared settlement adapters instead of executing them inline,
- revalidates default-deny policy at seal time, when the full run is visible,
- aborts and reverts staged actions on failure, and emits SHA-256-hashed, Ed25519-signed outcome evidence.

The agent never holds downstream credentials. Unexpected authority requires a newly delegated capability.

## Quickstart

The fault-injection demo runs the same three-step workflow — database update, payment authorization, notification — twice with the same injected failure before the final step: once with no boundary, once behind Clared. No external accounts or API keys are required. This is an in-memory simulator: it does not contact Postgres, Stripe, or any other live system.

**Prerequisites:** [Rust via rustup](https://rustup.rs) (`rust-toolchain.toml` pins the toolchain), Python 3.10+, and a free TCP port 4000. `uv` is used when installed; otherwise `python3 -m venv`. First `make demo` compiles the Rust service (a few minutes); a warm binary takes seconds.

```bash
git clone https://github.com/clared-ai/clared.git
cd clared
make demo
```

Expected output (evidence digest and signature differ on every run):

```text
1. Unsafe baseline
   database update: committed immediately
   payment: authorized (hold left open)
   injected failure: injected network failure before notification
   order status: payment_authorized
   notification sent: False
   rollback: none
   result: inconsistent state

2. Clared path with the same injected failure
   database: SIMULATED_STAGED_TX
   payment: SIMULATED_AUTH_HOLD
   injected failure: injected network failure before notification
   final state: ABORTED
   reverted simulated actions: 2
   escaped side effects: 0

3. Successful simulated settlement
   final state: SETTLED
   backend: in_memory_simulator
   evidence: sha256:<64-hex digest, unique per run>
   signature: ed25519:<base64 signature, unique per run>
```

A recorded run is available at [docs/demo/demo.cast](docs/demo/demo.cast):

```bash
brew install asciinema   # one-time
asciinema play docs/demo/demo.cast
```

## Status: shipped vs target

This repository proves the `v0alpha1` execution-envelope profile against an in-memory simulator. Everything else in the docs is target architecture until implemented and verified.

**Implemented and enforced today (`v0alpha1`, in-memory simulator):**

| Boundary | Reference implementation |
| --- | --- |
| Delegation | Single-use HMAC-SHA256 proof binds tenant, principal, role, intent, expiry, and nonce |
| Capability | Short-lived Ed25519-signed token bound to one session and generation |
| Tool access | Fail-closed allowlist; every allowed tool must have a registered adapter |
| Resource scope | Adapter-declared argument types must match a qualified envelope target |
| Policy | Default-deny Cedar evaluation at tool-call and seal time; reserved approval context cannot come from tool arguments |
| Aggregate budgets | Integer-only typed dimensions, including money, mutations, and notifications |
| Lifecycle | Expiry and terminal states are enforced; settled or aborted sessions cannot execute |
| Replay control | Tool, seal, and abort requests use scoped idempotency keys |
| Commit evidence | Deterministically serialized outcome evidence is SHA-256 hashed and Ed25519 signed |

Provider execution is deliberately simulated. Responses are labeled `in_memory_simulator` and use `SIMULATED_*` statuses.

**Target architecture (not implemented — proposals and drafts only):**

| Target | Where it is specified |
| --- | --- |
| Certified Semantic Run Supervisor with separate lifecycle authority; supervisor-authenticated `intent/quiesce`, atomic admission fence, durable `PREPARED` state | [Execution Envelope prepare-fence draft](specs/execution-envelope-v0alpha2-draft.md) |
| Progressive obligation feedback: staged actions carry signed obligation deltas and Clared-rendered guidance in ordinary tool results | [Execution Envelope prepare-fence draft](specs/execution-envelope-v0alpha2-draft.md) |
| Provisional root answers with signed `REPLAN_REQUIRED` suspension and bounded terminal repair | [Execution Envelope prepare-fence draft](specs/execution-envelope-v0alpha2-draft.md) |
| Trusted obligation facts, deterministic closure, physical/provider leases, drain and ambiguity rules | [Settlement Adapter lease draft](specs/settlement-adapter-v0alpha2-draft.md) |
| Real executors: PostgreSQL transactions with hard leases, payment holds, notification egress | [ROADMAP.md](ROADMAP.md) |
| MCP-compatible enforcement shim (shadow, deny-only, or per-call until a trustworthy root boundary exists) | [ROADMAP.md](ROADMAP.md) |
| Conformance fixtures, certified framework adapters, `clared run` / `clared doctor` | [ROADMAP.md](ROADMAP.md) |

In particular: `intent/seal` by an explicit harness is the shipped completion path today. The quiesce/prepare-fence lifecycle, obligation steering, and "atomic settlement" language describe the proposed target, not current behavior.

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

Clared is not a replacement for the systems below. It adds a stateful authority and settlement boundary around an agent-selected tool trajectory. The intended developer experience is a broad tool surface over least-privilege, session-scoped authority: the agent never receives downstream credentials, and unexpected authority requires a newly delegated capability.

| Category | Primary job | What remains outside it that Clared targets |
| --- | --- | --- |
| Durable workflow engines (for example Temporal or Restate) | Reliably execute an application-defined workflow | Bound authority for a non-deterministic sequence selected by an agent |
| Policy engines (Cedar or OPA) | Decide whether one request is allowed from current facts | Track aggregate budgets, staged effects, and co-requisite obligations across calls; guide the active agent and revalidate at settlement |
| LLM gateways | Govern model traffic, cost, routing, and observability | Govern downstream mutations after the model chooses an action |
| Sandboxed runtimes | Isolate code, processes, files, or network access | Express which business effects may accumulate and how they settle |
| Tool gateways and MCP permission layers | Expose tools and approve or deny individual calls | Bind the whole multi-tool operation to one capability, resource set, budget, lifecycle, and receipt |

Cedar is the deterministic authorization evaluator, not the trajectory engine by itself. A production implementation must derive trusted facts from session history and real systems, construct the execution dependency graph incrementally as actions are staged, and evaluate run-level invariants before settlement.

## Open specifications

The contracts are Apache-2.0 licensed and independently implementable.

| Specification | Governs | Status |
| --- | --- | --- |
| [Clared Execution Envelope](specs/execution-envelope.md) | Delegation, capabilities, budgets, resource scope, lifecycle, idempotency, and receipts | `v0alpha1` |
| [Clared Settlement Adapter](specs/settlement-adapters.md) | How a tool declares staging, settlement, rollback, resource extraction, and budget accounting | `v0alpha1` |
| [Execution Envelope — Prepare-Fence Draft](specs/execution-envelope-v0alpha2-draft.md) | Proposed progressive obligation feedback, semantic supervisor, lifecycle authority, quiescence fence, terminal repair, child and streaming semantics | `v0alpha2-draft` — not implemented |
| [Settlement Adapter — Lease Draft](specs/settlement-adapter-v0alpha2-draft.md) | Proposed trusted obligation facts, deterministic closure, physical/provider leases, repair compatibility, drain evidence and ambiguity rules | `v0alpha2-draft` — not implemented |

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

Clared also cannot infer every harmful business outcome. It can enforce only authority, policies, invariants, state, and provider effects that are formalized and observable at the boundary. Unknown or unmodeled risk still requires conservative defaults, scoped rollout, monitoring, and human escalation.

See [ROADMAP.md](ROADMAP.md) for the evidence-gated path to a certified lifecycle boundary, MCP-compatible enforcement, a real PostgreSQL transaction executor, conformance fixtures, and installable releases.

## Challenge the boundary

We are looking for teams with a consequential agent workflow that is still read-only, manually approved, narrowly scoped, or otherwise blocked from broader production autonomy. [Challenge the protocol in Discussions](https://github.com/clared-ai/clared/discussions), or email [liran@clared.ai](mailto:liran@clared.ai) to evaluate what control and evidence would be required to turn that workflow on safely.

## Repository layout

```text
clared-core/       Rust JSON-RPC service, policy engine, capabilities, sessions
clared-python/     Explicit Python harness and tool client
adapters/          Versioned settlement adapter declarations
specs/             Open execution-envelope and adapter specifications
docs/              Threat model, security analysis, and demo recording
examples/          Runnable fault-injection comparison
```

## Development

```bash
make check
```

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md). Licensed under [Apache-2.0](LICENSE).
