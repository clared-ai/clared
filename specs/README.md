# Clared open specifications

The specifications in this directory define contracts, not a claim of industry-standard status.

| Specification | Current version |
| --- | --- |
| [Execution Envelope](execution-envelope.md) | `clared.dev/execution-envelope/v0alpha1` |
| [Settlement Adapter](settlement-adapters.md) | `clared.dev/settlement-adapter/v0alpha1` |

The following documents are non-implemented design drafts for the next lifecycle revision:

| Draft | Proposed version | Primary change |
| --- | --- | --- |
| [Execution Envelope — Prepare-Fence Draft](execution-envelope-v0alpha2-draft.md) | `clared.dev/execution-envelope/v0alpha2-draft` | Progressive obligation feedback, semantic supervisor, lifecycle authority, quiescence fence, provisional output and terminal repair |
| [Settlement Adapter — Lease and Prepare-Fence Draft](settlement-adapter-v0alpha2-draft.md) | `clared.dev/settlement-adapter/v0alpha2-draft` | Trusted obligation facts, deterministic closure, physical/provider leases, repair compatibility and ambiguity handling |

All specifications are licensed under Apache-2.0. Independent implementations and adversarial reviews are welcome. Draft documents are proposals, not claims about the current server.

Until `v1`, compatibility may change between releases. Proposed changes should include:

1. The concrete failure mode being addressed.
2. Wire or schema changes.
3. Backward-compatibility impact.
4. New conformance and adversarial tests.

Promotion of the `v0alpha2` drafts additionally requires implemented progressive-obligation delivery plus the supervisor/gateway handshake, published obligation/lifecycle/terminal-repair race tests, and at least one live adapter with provider-backed lease and recovery evidence.

The specifications should move to independent repositories only after external implementations require separate governance or release cycles.
