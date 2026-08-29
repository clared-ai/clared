# Clared Settlement Adapter

**Version:** `clared.dev/settlement-adapter/v0alpha1`  
**Status:** Experimental reference specification  
**License:** Apache-2.0

> This is the implemented `v0alpha1` contract. The non-implemented [lease and prepare-fence `v0alpha2` draft](settlement-adapter-v0alpha2-draft.md) proposes trusted obligation facts, deterministic closure, physical lease, drain-evidence, and live conformance rules; it does not change this profile.

## 1. Scope

A settlement adapter declares how one or more tool names participate in an execution envelope. It defines:

- Exact tool-name matching.
- Resource-bearing arguments.
- Typed budget charges.
- Deterministic settlement order.
- Execution mode.
- Staging, settlement, rollback, and optional compensation hooks.

The adapter is data, not authority. The proxy must validate the adapter version and reject duplicate or unknown targets.

## 2. Example

```yaml
version: "clared.dev/settlement-adapter/v0alpha1"
metadata:
  name: "stripe_payment_intent_adapter"
  provider: "stripe"
  version: "0.1.0"

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
    settlement_order: 20

mode: MODE_3_RESERVATION

execution:
  staging:
    strategy: AUTH_HOLD
    request_transform:
      set_fields:
        capture_method: "manual"
  settlement:
    strategy: CAPTURE_AUTH
    endpoint: "/v1/payment_intents/{{staged_entity_id}}/capture"
  rollback:
    strategy: CANCEL_AUTH
    endpoint: "/v1/payment_intents/{{staged_entity_id}}/cancel"
```

The proxy qualifies each resource value as `<scope_prefix>:<argument_value>` and requires that exact value in the envelope targets. This prevents a customer identifier from matching an order scope merely because both terminal identifiers are equal. Every declared resource is independently evaluated by policy at call time and seal time. Budget arguments must be non-negative integers. Constant charges are used for count dimensions.

Settlement proceeds by ascending `settlement_order`; rollback proceeds in reverse. Equal values retain call order. The reference declarations place transactional database work before payment capture and buffered notifications last.

## 3. Execution modes

| Mode | Staging model | Seal model | Abort model |
| --- | --- | --- | --- |
| `MODE_1_SQL` | Connection-scoped transaction or savepoint | Commit | Roll back |
| `MODE_2_MOCK` | Virtual overlay object | Materialize | Discard |
| `MODE_3_RESERVATION` | Provider-supported hold or draft | Capture or activate | Cancel or delete draft |
| `MODE_4_CHECKPOINT` | Final preflight before irreversible action | Execute and record | Compensate when possible |
| `EGRESS_SINK` | Buffer outbound notification | Dispatch | Discard |

Execution modes describe required semantics, not guarantees that every API can provide them. An adapter author must not describe a live mutation as a reservation when the provider lacks a genuine hold or draft primitive.

## 4. Current reference profile

The repository includes declarations for:

- `postgres.orders.update`
- `stripe.payment_intents.create`
- `twilio.messages.create`

The `v0alpha1` Rust backend parses these declarations and uses them for exact tool matching, typed resource extraction, budget charges, execution strategies, and ordering. It simulates stage, seal, and abort outcomes in memory. It does not execute the HTTP or SQL hooks yet.

## 5. Settlement and degraded outcomes

A live backend must:

1. Persist enough idempotency state to retry safely.
2. Settle actions in an explicit dependency order.
3. Stop on an ambiguous provider outcome.
4. Report `PARTIALLY_SETTLED` when an effect may have escaped.
5. Run only declared, authenticated compensators.
6. Produce signed evidence for both successful and degraded terminal states.

Compensation is remediation, not rollback. Implementations must not claim zero side effects after an irreversible action was attempted.

## 6. Versioning

Consumers must reject unknown major or stability versions. `v0alpha1` may change incompatibly. A future `v1` requires a machine-readable schema, conformance fixtures, at least one live backend, and independent implementation feedback.
