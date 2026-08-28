# Roadmap

Clared is validating a security boundary before expanding the integration surface. Milestones are ordered by evidence, not by a promise of dates.

## Current reference profile

- Versioned Execution Envelope and Settlement Adapter specifications.
- Signed, generation-fenced session capabilities and single-use delegation proofs.
- Tool allowlists, typed resources, aggregate budgets, idempotency, and commit-time policy revalidation.
- Adapter-driven in-memory stage, seal, rollback, ordering, and signed evidence.
- Python client, fault-injection demo, adversarial tests, and CI.

## Next: make the boundary touch real agent traffic

1. Document one design partner's real multi-system workflow and failure trace.
2. Build an MCP-compatible enforcement shim that routes mutating tool calls through an envelope without requiring an agent rewrite.
3. Publish conformance fixtures for capability, scope, budget, lifecycle, and idempotency behavior.
4. Package supported Python and Rust artifacts only when their names, compatibility policy, and upgrade path are stable enough to maintain.

## Next: prove one real settlement mode

1. Implement a PostgreSQL executor using a genuine connection-scoped transaction.
2. Demonstrate injected failure followed by observable database rollback.
3. Persist sessions, idempotency records, signer identity, and ambiguous terminal outcomes.
4. Add policy-version and adapter-version bindings to signed evidence.

## After those proofs

- Stripe test-mode reservation and capture semantics.
- Buffered notification egress.
- Recovery and reconciliation for partial or ambiguous settlement.
- Enterprise identity-provider delegation.
- Thin integrations for frameworks justified by design-partner usage.

## Explicitly not claimed

The roadmap does not assume that every provider can support rollback or that Clared can make arbitrary APIs transactional. Each live adapter must expose the provider's real reservation, compensation, and ambiguity semantics. Features move into the current profile only with executable conformance tests and accurate failure evidence.
