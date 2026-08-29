# Roadmap

Clared is validating a security boundary before expanding the integration surface. Milestones are ordered by evidence, not by a promise of dates.

## Current reference profile

- Versioned Execution Envelope and Settlement Adapter specifications.
- Signed, generation-fenced session capabilities and single-use delegation proofs.
- Tool allowlists, typed resources, aggregate budgets, idempotency, and commit-time policy revalidation.
- Adapter-driven in-memory stage, seal, rollback, ordering, and signed evidence.
- Python client, fault-injection demo, adversarial tests, and CI.

## Next: prove the semantic lifecycle boundary

1. Document one consequential workflow that a design partner is unwilling to authorize today, the controls blocking it, and the evidence required to enable broader autonomy.
2. Review and refine the [`v0alpha2` prepare-fence draft](specs/execution-envelope-v0alpha2-draft.md) and [adapter lease draft](specs/settlement-adapter-v0alpha2-draft.md) before treating either as a compatibility promise.
3. Implement separate action and lifecycle credentials, a linearizable `intent/quiesce` admission fence, Clared-owned `admission_seq`, registered child epochs, incremental outstanding obligations, and durable `PREPARED` state.
4. Implement signed obligation revisions and deltas in ordinary tool results, exact-action versus outcome-only resolution, forged-satisfaction rejection, compact open-obligation snapshots, and optional declared deterministic closure.
5. Treat normal root return as provisional. Make the explicit Python harness the first block-only implementation of the new lifecycle: it withholds output until settlement and aborts/reconciles staging before returning structured finalization failure; exception, cancellation, and abandoned streaming consumption also abort or recover.
6. Implement signed `REPLAN_REQUIRED` suspension, supervisor-only `intent/resume`, fresh and optionally narrowed continuation authority for monotonic additive repair, bounded repair budgets, and checkpoint restart for non-monotonic repair.
7. Publish the lifecycle race suite: obligation open/satisfy ordering, stripped guidance, provider-text injection, deterministic closure replay, tool-versus-quiesce, replayed quiescence/resume, forged lifecycle calls, stale work after resume, child ordering, detached children, repair exhaustion, process death, ambiguous drains, lease expiry during suspension, and final-output delivery failure.
8. Build one certified framework adapter selected from actual design-partner usage. Add `clared run` and `clared doctor` only for pinned versions whose root hooks, tool hooks, obligation delivery, async propagation, child registration, output ordering, and declared `RESUME_CAPABLE` or `BLOCK_ONLY` behavior pass conformance.
9. Refuse live multi-step mode for unsupported runtimes. Shadow or per-call downgrade must be an explicit operator choice.

## Next: make the boundary touch real agent traffic

1. Build an MCP-compatible enforcement shim that routes mutating tool calls through an envelope. Without the semantic supervisor, explicit harness, or managed-worker boundary, run it only in shadow, deny-only, or independently safe per-call mode.
2. Ensure the enforcing shim, not the agent, owns downstream credentials and mutating egress paths.
3. Use eBPF or OS/network telemetry only for bypass and process-death evidence, never to infer successful task completion.
4. Publish conformance fixtures for capability, scope, budget, lifecycle, idempotency, and complete-mediation behavior.
5. Package supported Python and Rust artifacts only when their names, compatibility policy, and upgrade path are stable enough to maintain.

## Next: prove stateful policy, steering, and speculative change-sets

1. Load versioned customer Cedar bundles and evaluate them during envelope admission, tool calls, and settlement preparation.
2. Derive authenticated policy facts from session history, approvals, adapters, and fresh provider state rather than agent-supplied context.
3. Build an execution graph incrementally from staged actions, resource effects, dependencies, reversibility, and co-requisite obligations. The agent remains free to choose the path without submitting a rigid pre-scripted plan.
4. Add explicit decision outcomes for allow-and-stage, deny-and-replan, require-approval, require-action, checkpoint, suspend, revoke, and abort, with authenticated obligation deltas surfaced progressively in tool results and revalidated at termination.
5. Add deterministic closure recipes only for idempotent actions fully derivable from authenticated facts; keep judgment-dependent outcomes with the agent or an authorized principal.
6. Implement durable speculative change-set overlays or database branching for workflows that cannot safely fit inside a short physical transaction lease.
7. Bind the admitted envelope hash, policy version, adapter versions, and trusted state witnesses into signed evidence.

## Next: prove real settlement modes and compliance attestation

1. Implement a PostgreSQL executor that acquires a pinned transaction lazily on first mutation, enforces a hard physical lease, and never keeps locks open across unbounded model or human pauses.
2. Support connection-scoped savepoints within the parent lease and an explicit durable staging/checkpoint alternative for longer workflows.
3. Demonstrate injected failure, cancellation, physical lease expiry, process death before prepare, and ambiguous commit recovery with provider-observed evidence.
4. Persist sessions, admission records, idempotency keys, child state, signer identity, prepared settlement plans, and ambiguous terminal outcomes.
5. Format signed evidence receipts for audit use without claiming that a receipt alone establishes regulatory compliance.

## After those proofs

- Stripe test-mode reservation and capture semantics.
- Buffered notification egress.
- Recovery and reconciliation for partial or ambiguous settlement.
- Enterprise identity-provider delegation.
- Additional certified framework integrations justified by design-partner usage.

## Explicitly not claimed

The roadmap does not assume universal completion inference, that every framework can resume, that every provider can support rollback, or that Clared can make arbitrary APIs transactional. Timers and lease expiry never authorize settlement or extend a repair window. Each live runtime adapter must prove its semantic boundary and repair capability, and each live settlement adapter must expose the provider's real reservation, lease, compensation, and ambiguity semantics. Features move into the current profile only with executable conformance tests and accurate failure evidence.
