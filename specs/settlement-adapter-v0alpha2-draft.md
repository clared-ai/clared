# Clared Settlement Adapter — Lease and Prepare-Fence Draft

**Proposed version:** `clared.dev/settlement-adapter/v0alpha2-draft`
**Status:** Design draft; not implemented by the current reference server
**License:** Apache-2.0

## 1. Scope

This draft extends the [`v0alpha1` settlement adapter](settlement-adapters.md) so an adapter can participate safely in the proposed [prepare-fence lifecycle](execution-envelope-v0alpha2-draft.md). It adds explicit effect classification, trusted obligation facts, optional deterministic closure hooks, terminal outcome evidence, physical and provider lease behavior, dependency ordering, ambiguity handling, and conformance requirements.

An adapter is trusted executable configuration, not authority and not proof of provider behavior. A live adapter becomes eligible for enforcement only after human review and executable tests against the provider behavior it claims.

## 2. Required declarations

A live-capable adapter declares:

- exact tool and provider/API versions;
- typed resource extraction and budget charges;
- read and write effects used to build the execution graph;
- typed, schema-validated result facts that policy may use to open or satisfy obligations;
- staging, settlement, abort, and reconciliation hooks;
- any optional idempotent deterministic closure hook and its authenticated argument derivation;
- genuine provider success, failure, and ambiguous response witnesses;
- idempotency and read-after-write reconciliation semantics;
- physical transaction or provisional provider lease limits;
- whether staged state may survive a fenced terminal-repair suspension and under which monotonicity constraints;
- settlement dependency edges and reversibility;
- compensation semantics, where available; and
- credential and egress requirements for complete mediation.

Strategy labels do not create semantics. For example, an endpoint is not `MODE_3_RESERVATION` merely because it accepts `dry_run`; it must expose a genuine provisional state with verifiable finalize and cancel behavior.

## 3. Illustrative schema

```yaml
version: "clared.dev/settlement-adapter/v0alpha2-draft"
metadata:
  name: "stripe_payment_intent_adapter"
  provider: "stripe"
  provider_api_version: "pinned-version"
  adapter_version: "0.2.0"

targets:
  - tool_name: "stripe.payment_intents.create"
    resource_arguments:
      - argument: "customer_id"
        scope_prefix: "customer"
    budget_charges:
      - dimension: "money.minor.USD.hold"
        argument: "amount_minor"
      - dimension: "money.minor.USD.capture"
        argument: "amount_minor"

effects:
  writes: ["payment_intent:{{provider_result.id}}"]
  reversibility: PROVISIONAL_UNTIL_CAPTURE
  settlement_after: ["postgres.orders.update"]

trusted_result_facts:
  payment_intent_id: "$.id"
  customer_id: "$request.customer_id"
  staged_state: "$.status"

mode: MODE_3_RESERVATION

repair_contract:
  same_epoch: MONOTONIC_ADDITIVE_ONLY
  max_suspend_ms: 30000
  checkpoint_restart: SUPPORTED

execution:
  staging:
    strategy: AUTH_HOLD
    request_transform:
      set_fields:
        capture_method: "manual"
    terminal_success_witness:
      json_path: "$.status"
      values: ["requires_capture"]

  settlement:
    strategy: CAPTURE_HOLD
    endpoint: "/v1/payment_intents/{{staged_entity_id}}/capture"
    idempotency: PROVIDER_KEY
    terminal_success_witness:
      json_path: "$.status"
      values: ["succeeded"]

  abort:
    strategy: CANCEL_HOLD
    endpoint: "/v1/payment_intents/{{staged_entity_id}}/cancel"
    idempotency: PROVIDER_KEY

  reconcile:
    strategy: READ_AFTER_UNKNOWN
    endpoint: "/v1/payment_intents/{{staged_entity_id}}"
    unresolved_behavior: RECOVERY_REQUIRED

  leases:
    provider_reservation:
      max_duration_ms: 240000
      expiry_behavior: CANCEL_OR_RECOVERY
      ambiguity_behavior: RECOVERY_REQUIRED
```

Names and exact schema shape remain draft. The behavioral requirements below are normative for the proposal.

## 4. Prepare-fence participation

1. **Admission record first:** Clared assigns `admission_seq`, reserves budgets and idempotency state, and persists the action record before dispatching the adapter.
2. **No boundary inference:** The adapter never decides that the agent run is complete. It receives lifecycle state from the Clared kernel.
3. **Trusted obligation facts:** After outcome verification, the adapter exposes only declared typed facts to policy. Clared constructs obligation state and canonical model-visible guidance; provider-controlled text is never an instruction source.
4. **Deterministic closure:** An optional closure hook is eligible only when explicitly declared, idempotent, budgeted, and fully parameterized from authenticated facts. It executes as a normal admitted action with normal evidence.
5. **Drain evidence:** After the admission fence closes, the adapter resolves every admitted action to staged success, definite no-effect failure, or ambiguity. Unknown is not success.
6. **Prepare gate:** The adapter confirms its provisional state, lease validity, required dependencies, obligation state, and settlement/abort idempotency material before the kernel may record `PREPARED`.
7. **Terminal repair declaration:** The adapter states whether staged effects may remain valid through a bounded `SUSPENDED(REPLAN_REQUIRED)` interval after progressive guidance was insufficient. Same-epoch continuation is permitted only for monotonic additive repair that cannot invalidate earlier admissions. An adapter that cannot prove this requires checkpoint restart or abort.
8. **No fence reopening:** A resumed continuation uses a fresh action generation. The adapter rejects stale work from the completed attempt and must not treat suspension as extending a physical or provider lease.
9. **Recorded settlement:** Settlement runs only from the durable plan and in dependency order. Replays use the recorded idempotency keys.
10. **Evidence-based terminal state:** Adapter observations determine `SETTLED`, `PARTIALLY_SETTLED`, `ABORTED`, or `RECOVERY_REQUIRED`; the desired result never overrides observed provider state.

## 5. Lease model

Lease types are independent:

| Lease | Starts | Maximum | Expiry behavior |
| --- | --- | --- | --- |
| Physical database transaction | First admitted mutation and `BEGIN` | Adapter value capped by platform policy | Roll back; abort or recover |
| Provider reservation/draft | Provider confirms provisional state | Provider and policy minimum | Cancel, reconcile, or recover |
| Child budget | Child authority issuance | Envelope/policy limit | Fence child; release only proven-unused capacity |
| Logical session | Envelope admission | Policy limit | Abort/recover; never settle |

An adapter MUST NOT map lease expiry to its settlement hook. A terminal-repair suspension does not pause, renew, or silently replace any lease. If the remaining lease cannot cover the declared repair window, same-epoch continuation is unsafe.

## 6. `MODE_1_SQL` requirements

A database adapter:

1. acquires no pinned connection for a read-only epoch;
2. lazily acquires a connection and executes `BEGIN` on the first admitted mutation;
3. routes subsequent compatible reads/writes through that connection while its lease is valid;
4. treats savepoints as nested scopes that cannot outlive the parent transaction;
5. enforces a hard platform-capped physical lease;
6. rolls back on exception, cancellation, worker death before preparation, session expiry, or physical lease expiry; and
7. records an explicit ambiguous/recovery state if it cannot determine whether commit occurred.

It MUST NOT hold locks across an unbounded model delay, human approval, durable suspension, or disconnected worker. A short terminal-repair continuation may retain the transaction only when both the adapter and platform policy explicitly permit it and the original hard lease remains valid; resume never resets that clock. A workflow that cannot finish inside the lease needs an explicit durable alternative such as application-level staging, shadow tables/branches, provider-native drafts, or a checkpoint. If no alternative preserves required semantics, live mode is unsupported.

## 7. Provider reservations and sinks

A reservation is a real provisional provider effect even when no final capture or publication occurred. The adapter must expose reservation identity, expiry, cancellation evidence, and ambiguity behavior.

Notification and webhook sinks may be topologically delayed. Their prepared payloads and idempotency keys must be durable whenever process loss after preparation could otherwise lose an obligated dispatch. An in-memory buffer is acceptable only for the simulator or when loss is explicitly part of the declared semantics.

## 8. Degraded outcomes

Provider timeouts, connection loss, and conflicting read-after-write results are not automatically failures with no effect. The adapter first applies its declared reconciliation method. If the result remains unknown:

- preparation is blocked when ambiguity occurred during staging;
- settlement enters `PARTIALLY_SETTLED` or `RECOVERY_REQUIRED` when an effect may have escaped; and
- abort cannot report `ABORTED` until cancellation/no-effect is proven.

Compensation is a new real-world action and may fail. It is remediation, not rollback and not proof of atomicity.

## 9. Certification tests

A live adapter MUST demonstrate:

- exact resource and budget extraction, including malformed and adversarial arguments;
- call/quiesce races around dispatch;
- idempotent replay before and after provider success;
- process death after local prepare and during each settlement step;
- definite failure versus ambiguous provider response;
- physical and provider lease expiry;
- typed obligation-fact extraction and provider-text prompt-injection attempts;
- obligation open/satisfaction ordering and deterministic closure replay/accounting;
- monotonic same-epoch repair, stale-generation rejection, and successful re-quiescence;
- non-monotonic repair forced to checkpoint restart or abort;
- repair suspension that outlives a physical or provider lease;
- safe abort and cancellation evidence;
- dependency ordering and partial settlement evidence;
- compensation failure; and
- direct-credential and alternate-egress denial.

Generated adapters remain shadow/test-only until a human approves the effect model and this suite passes against a pinned provider/API version.

## 10. Non-guarantees

This specification does not create distributed ACID, guarantee provider uptime, or make arbitrary APIs reversible. It makes adapter assumptions explicit enough to test and ensures Clared reports uncertainty instead of converting it into a falsely safe outcome.
