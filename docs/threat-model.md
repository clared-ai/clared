# Threat model

**Profile:** Clared `v0alpha1` reference implementation  
**Backend:** In-memory simulator  
**Last reviewed:** 2026-08-28

This document separates the envelope mechanics implemented today from the trust boundaries a live deployment would still need. Finding a case where the implementation reports a safer outcome than it actually achieved is a security defect.

## Security objective

Clared addresses operations in which an action-taking agent may choose a valid but unpredictable sequence of mutating tools. The objective is to prevent authority granted for one bounded task from becoming broader authority across tools, resources, aggregate spend, time, or capability generations, and to make the terminal outcome explicit.

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
| Compromised proxy or signing key | Outside the current implementation boundary. | Requires key persistence, rotation, isolation, and operational controls. |
| Memory exhaustion from unbounded sessions or replay records | No quotas or garbage collection in the reference server. | Known denial-of-service risk; do not expose the simulator as a public service. |

## What the current evidence proves

A valid receipt proves that one running reference process produced the stated simulated outcome under its in-memory state and signing key. It does not prove that PostgreSQL, Stripe, Twilio, or any other provider was contacted. It does not prove a universal rollback guarantee.

## Security review priorities

The most useful adversarial reports currently are:

1. A path that executes outside the tool, resource, budget, generation, or lifecycle envelope.
2. A replay that causes a second staged or terminal action.
3. An argument shape that bypasses typed accounting or policy context separation.
4. A receipt that verifies after its evidence is changed.
5. An adapter declaration whose ambiguity could produce a falsely safe outcome.

Report boundary bypasses privately under [SECURITY.md](../SECURITY.md). Use GitHub Discussions for model limitations and protocol design questions.
