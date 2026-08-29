# Threat model

**Profile:** Clared `v0alpha1` reference implementation  
**Backend:** In-memory simulator  
**Last reviewed:** 2026-08-29

This document separates the envelope mechanics implemented today from the trust boundaries a live deployment would still need. Finding a case where the implementation reports a safer outcome than it actually achieved is a security defect.

## Security objective

Clared addresses operations in which an action-taking agent may choose a valid but unpredictable sequence of mutating tools. The objective is to let an organization grant useful autonomy while preventing authority for one bounded task from becoming broader authority across tools, resources, aggregate spend, time, capability generations, or the cumulative trajectory, and to make the terminal outcome explicit.

Clared does not attempt to prove that the agent reasoned correctly. It constrains what the resulting trajectory may do.

## Trust boundaries

| Component | Current trust assumption |
| --- | --- |
| Agent and tool arguments | Untrusted. Reserved policy context is overwritten by the proxy. |
| Trusted harness | Authenticates the initiating principal and holds the delegation secret. The convenience Python wrapper is not process isolation. |
| Clared proxy | Trusted to enforce lifecycle, policy, budget, scope, and adapter rules and to protect its signing key. |
| Adapter registry | Trusted configuration. A dishonest adapter can misdescribe resource extraction or settlement behavior. |
| Execution backend | Simulated today. A live backend must withhold provider credentials from the agent and persist ambiguous outcomes. |
| Receipt verifier | Must obtain the expected signer public key through a trusted channel; a key embedded only in an untrusted receipt is not sufficient trust. |

The proposed `v0alpha2` design adds a trusted Semantic Run Supervisor and distinct lifecycle credential. Neither is part of the current implementation. See the [prepare-fence draft](../specs/execution-envelope-v0alpha2-draft.md).

## Attack analysis

| Attack or failure | Current mechanism | Current status / residual risk |
| --- | --- | --- |
| Principal self-assertion or confused deputy | Single-use HMAC delegation binds tenant, principal, role, task intent, expiry, and nonce. | Enforced in the reference protocol. Production still needs an identity provider and separation between agent and delegator credentials. |
| Tool outside delegated task | Exact session allowlist and mandatory registered adapter. | Enforced, fail-closed. |
| Resource substitution | Adapter types each resource argument; every qualified resource must appear in the envelope and is evaluated by policy. | Enforced for declared resource arguments. Adapter omissions remain trusted-configuration risk. |
| Aggregate spend through many individually valid calls | Integer typed budgets are checked and deducted under the session write lock. | Enforced in one process. Durable, multi-replica accounting is not implemented. |
| Monetary bypass using floats or negative values | Budget-bearing arguments must decode as non-negative integers in minor units. | Enforced. |
| Agent forges privileged policy context | Proxy overwrites reserved admission and approval fields before Cedar evaluation. | Enforced for current reserved fields. Future trusted claims need a separate authenticated channel. |
| Stale capability after amendment | Ed25519 capability binds the session and monotonic generation; only the active token is accepted. | Enforced. |
| Tool or terminal replay | Scoped idempotency keys return cached identical results and reject conflicting reuse. | Enforced in memory; state is lost on restart. |
| Policy changes between action and settlement | Every staged action and every declared resource is evaluated again at seal time. | Enforced against the in-process policy set. Policy version pinning is not yet included in receipts. |
| Failure after an early action | Adapter ordering and rollback strategy determine simulated abort behavior. | Demonstrated only in memory. No claim is made about real provider rollback. |
| Irreversible or ambiguous provider outcome | Protocol includes degraded lifecycle states and forbids reporting `SETTLED` unless adapter settlement succeeds. | Live detection, recovery, and reconciliation are not implemented. |
| Agent bypasses Clared | Production design requires provider credentials and mutating network paths to terminate at the enforcing proxy. | Not prevented by the Python helper alone. |
| Policy is incomplete or depends on unobserved state | Conservative envelope bounds, explicit adapters, and commit-time revalidation limit modeled effects. | Clared cannot prevent an outcome that is neither formalized nor observable. Production policy facts must come from authenticated session history, approvals, and fresh provider state. |
| Agent needs authority outside the active envelope | Generation-fenced amendment is the intended escalation path. | The current profile widens budgets with a fresh delegation proof; a real human approval channel, broader scope amendment, suspension, and revocation remain future work. |
| Compromised proxy or signing key | Outside the current implementation boundary. | Requires key persistence, rotation, isolation, and operational controls. |
| Memory exhaustion from unbounded sessions or replay records | No quotas or garbage collection in the reference server. | Known denial-of-service risk; do not expose the simulator as a public service. |

## Target lifecycle threats (`v0alpha2` draft, not implemented)

These are requirements for the proposed design, not current defenses:

| Attack or failure | Required target behavior |
| --- | --- |
| Timer, MCP silence, model `finish_reason`, process exit, or socket trace is mistaken for successful task completion | None is a success signal. Live settlement requires a certified semantic root boundary, explicit harness, or one-task managed worker. |
| Agent steals or forges lifecycle authority | Action and supervisor credentials use distinct types and audiences; lifecycle material stays outside model context; `intent/quiesce` rejects action capabilities. |
| Tool call races quiescence | One linearizable operation lock orders admission against the fence. The call is assigned a Clared-owned sequence at or below the high-water mark or is rejected after the fence. |
| Caller omits an action or child from the quiescence request | Caller supplies neither authoritative sequence nor child manifest. Clared snapshots its own admission log and registered child set. |
| Detached child acts after its parent closes | Parent fencing invalidates child authority; a parent cannot prepare until registered children are prepared or safely terminal. |
| Root exception, cancellation, or abandoned stream is treated as success | Certified adapters route those events to abort/recovery. Only a certified framework root-success event may request quiescence, and its output remains provisional until settlement. |
| Process dies before or after preparation | Before preparation, rollback/recovery applies. After durable preparation, recovery resumes the recorded settlement plan idempotently. |
| Database locks survive model thinking or human approval | Physical transactions open lazily, have hard maximum leases, and roll back or checkpoint before unbounded pauses. Lease expiry never commits. |
| Provider result is unknown while the epoch drains | Ambiguity blocks preparation or enters `RECOVERY_REQUIRED`; the system never guesses no-effect or success. |
| Business invariants exist only as prompt instructions | Policy and graph state remain authoritative outside the prompt. Certified profiles surface relevant obligation deltas in ordinary tool results, but enforcement never depends on model compliance. |
| Agent or provider claims an obligation is satisfied or waived | Only authenticated graph transitions from admitted actions or a policy-authorized principal can resolve it; revision replay is rejected. |
| Provider-controlled text becomes a high-priority instruction | Clared renders guidance only from approved templates and authenticated facts. Provider/tool free text remains untrusted data. |
| Required-action guidance silently expands agent authority | Guidance is non-authorizing. The attempted action must still pass the existing capability, resource, budget, policy, and adapter checks. |
| Client strips obligation metadata or model-visible guidance | The integration cannot be certified for progressive self-correction. Enforcement remains fail-closed and terminal behavior uses its declared resume or block-only capability. |
| Obligation flooding bloats context or causes a blocker to be truncated | Profiles bound and relevance-filter model-visible snapshots while retaining complete supervisor state. A blocking obligation is never silently omitted; limit exhaustion suspends or fails closed. |
| Deterministic closure executes an attacker-chosen repair | Closure is allowed only from an approved idempotent recipe whose arguments derive from authenticated state and whose action passes ordinary admission and accounting. |
| Final response reaches the user before settlement | Integration buffers the final response or marks it provisional. Authoritative success follows a signed terminal receipt. |
| Terminal validation rejects a repairable candidate after the framework root returned | Clared withholds the candidate, keeps the completed generation fenced, and emits a signed `REPLAN_REQUIRED` suspension with authenticated outstanding obligations. |
| Delayed work from the completed attempt enters after repair begins | `intent/resume` creates a fresh greater generation and continuation invocation; stale, detached, and replayed work is rejected. |
| A repair mutates assumptions behind already-staged actions | Same-epoch repair is restricted to monotonic additive work. Non-monotonic repair must roll back or abort the staged branch and restart from a durable checkpoint. |
| Model steering is attempted for an ambiguous provider result or hard violation | Ambiguity and non-repairable violations follow abort/recovery. They never enter the model repair loop. |
| Repair loops consume unbounded model, tool, time, or provider capacity | Profiles cap attempts, cumulative wall time, model/tool budgets, and allowed repair classes. Exhaustion aborts or enters recovery. |
| Framework adapter claims it can steer after root completion but cannot preserve state | Certification distinguishes `RESUME_CAPABLE` from `BLOCK_ONLY`; unsupported or broken resume paths fail closed and return structured application failure. |
| Block-only integration returns failure but leaves staged effects or a transaction stranded | It aborts/reconciles before returning unless a configured durable orchestrator explicitly accepts suspension ownership; no physical lease may be orphaned. |
| Suspension silently extends a database or provider lease | Suspension does not pause or renew physical leases. Expiry forces checkpoint restart, abort, or recovery and never permits settlement. |
| eBPF or socket telemetry is promoted to a completion oracle | OS telemetry is limited to bypass, process-death, and egress evidence and cannot initiate settlement. |
| Unsupported framework version runs live | `clared doctor` detects missing/incompatible hooks and live mode fails closed unless an operator explicitly selected shadow/per-call behavior. |

The required adversarial suite is enumerated in the draft specification. Promotion to an implemented profile requires executable evidence for every listed race and failure mode.

## What the current evidence proves

A valid receipt proves that one running reference process produced the stated simulated outcome under its in-memory state and signing key. It does not prove that PostgreSQL, Stripe, Twilio, or any other provider was contacted. It does not prove a universal rollback guarantee.

## Security review priorities

The most useful adversarial reports currently are:

1. A path that executes outside the tool, resource, budget, generation, or lifecycle envelope.
2. A replay that causes a second staged or terminal action.
3. An argument shape that bypasses typed accounting or policy context separation.
4. A receipt that verifies after its evidence is changed.
5. An adapter declaration whose ambiguity could produce a falsely safe outcome.

For the draft lifecycle, especially useful design reviews include a counterexample to obligation revision ordering, forged resolution, provider-text instruction injection, stripped-delivery certification, deterministic closure derivation, fence linearization, a framework callback that escapes async context or child registration, a streaming path with ambiguous completion, a replay or stale-generation path through terminal repair, a non-monotonic repair incorrectly retained in the same epoch, or an adapter lease path that can settle after expiry.

Report boundary bypasses privately under [SECURITY.md](../SECURITY.md). Use GitHub Discussions for model limitations and protocol design questions.
