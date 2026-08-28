# Clared open specifications

The specifications in this directory define contracts, not a claim of industry-standard status.

| Specification | Current version |
| --- | --- |
| [Execution Envelope](execution-envelope.md) | `clared.dev/execution-envelope/v0alpha1` |
| [Settlement Adapter](settlement-adapters.md) | `clared.dev/settlement-adapter/v0alpha1` |

Both are licensed under Apache-2.0. Independent implementations and adversarial reviews are welcome.

Until `v1`, compatibility may change between releases. Proposed changes should include:

1. The concrete failure mode being addressed.
2. Wire or schema changes.
3. Backward-compatibility impact.
4. New conformance and adversarial tests.

The specifications should move to independent repositories only after external implementations require separate governance or release cycles.
