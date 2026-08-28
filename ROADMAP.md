# Roadmap

Clared is validating a security boundary before expanding the integration surface. Milestones are ordered by evidence, not by a promise of dates.

## Current reference profile

- Versioned Execution Envelope and Settlement Adapter specifications.
- Signed, generation-fenced session capabilities and single-use delegation proofs.
- Tool allowlists, typed resources, aggregate budgets, idempotency, and commit-time policy revalidation.
- Adapter-driven in-memory stage, seal, rollback, ordering, and signed evidence.
- Python client, fault-injection demo, adversarial tests, and CI.

## Next: make the boundary touch real agent traffic

1. Document one consequential workflow that a design partner is unwilling to authorize today, the controls blocking it, and the evidence required to enable broader autonomy.
2. Build an MCP-compatible enforcement shim that routes mutating tool calls through an envelope without requiring an agent rewrite.
3. Ensure the enforcing shim, not the agent, owns downstream credentials and mutating egress paths.
4. Publish conformance fixtures for capability, scope, budget, lifecycle, and idempotency behavior.
5. Package supported Python and Rust artifacts only when their names, compatibility policy, and upgrade path are stable enough to maintain.

## Next: prove stateful policy and steering

1. Load versioned customer Cedar bundles and evaluate them during envelope admission, tool calls, and settlement preparation.
2. Derive authenticated policy facts from session history, approvals, adapters, and fresh provider state rather than agent-supplied context.
3. Build an execution graph incrementally from staged actions, resource effects, dependencies, reversibility, and co-requisite obligations. The agent remains free to choose the path; it does not submit a fixed plan up front.
4. Add explicit decision outcomes for allow-and-stage, deny-and-replan, require-approval, require-action, checkpoint, suspend, revoke, and abort.
5. Bind the admitted envelope hash, policy version, adapter versions, and trusted state witnesses into signed evidence.

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
