# Clared Execution Envelope — Prepare-Fence Draft

**Proposed version:** `clared.dev/execution-envelope/v0alpha2-draft`
**Status:** Design draft; not implemented by the current reference server
**License:** Apache-2.0

## 1. Scope

This draft defines a deterministic lifecycle boundary for a multi-step agent run when a trusted integration can observe the runtime's semantic root invocation. It extends `v0alpha1` with:

- separate action and lifecycle authority;
- supervisor-authenticated `intent/quiesce`;
- a linearizable admission fence and Clared-owned action sequence;
- registered parent/child epoch draining;
- provisional root results, progressive obligation feedback, optional deterministic closure, and fenced terminal repair;
- internal `PREPARED` and durable settlement states;
- explicit streaming, cancellation, expiry, and output-ordering rules; and
- fail-closed certification for zero-application-code runtime adapters.

This is a target contract, not a statement that the repository currently executes live database or provider effects. The implemented profile remains [`v0alpha1`](execution-envelope.md).

## 2. Boundary premise

There is no universal, deterministic way to infer completion of an agent task from tool-call silence, a debounce timer, process activity, socket traces, or one LLM response's finish metadata. Those signals do not prove that a framework has no retry, child, callback, local-model step, or delayed tool call remaining.

Live multi-step settlement therefore requires one of:

1. a certified semantic runtime adapter;
2. an explicit trusted harness boundary; or
3. a managed worker whose single invocation is exactly one task.

A pure MCP proxy without one of those boundaries is limited to shadow, deny-only, or independently safe per-call enforcement. Timeout and inactivity paths never commit.

## 3. Authority separation

The agent is an untrusted executor. Two credential classes are distinct:

### 3.1 Action capability

An action capability authorizes bounded tool admissions. It includes the operation, tenant, principal, agent actor, allowed audience, envelope hash, policy version, generation, expiry, and replay identifier. It cannot invoke lifecycle methods.

### 3.2 Supervisor credential

A supervisor credential is held by the certified runtime adapter, explicit harness, or managed worker—not placed in model context. It binds:

- `operation_id`;
- `root_invocation_id`;
- tenant and trusted lifecycle principal;
- runtime adapter identity and version;
- allowed lifecycle methods;
- current generation; and
- issue, expiry, and replay claims.

`intent/quiesce` MUST reject an action capability even when its principal and operation match. Credential provisioning and workload identity are deployment-profile concerns, but the resulting lifecycle assertion must be cryptographically authenticated.

A repair suspension rotates lifecycle authority. A credential accepted by `intent/resume` MUST be method-scoped and bound to the `suspension_id`, steering receipt, and fence generation. The credential used to request quiescence cannot be replayed to resume the epoch.

### 3.3 Root start and asynchronous correlation

On the certified framework's root-entry hook, the supervisor creates a unique `root_invocation_id`, opens the execution envelope, and binds the returned operation context before any modeled tool can run. The propagated action context contains the operation, generation, action-capability reference, root invocation, and optional parent/child identifiers. The supervisor credential remains in private adapter state.

The binding follows the runtime's structured concurrency semantics so concurrent roots cannot exchange capabilities. Python `ContextVar`, Node.js `AsyncLocalStorage`, Java scoped context, and .NET `AsyncLocal` are implementation examples, not protocol requirements. A framework-created task or thread must inherit the correct action context, register as a child epoch, or fail closed. Missing context is not repaired by correlating timestamps or nearby socket traffic.

## 4. Lifecycle

```text
PROPOSED -> ADMITTED -> ACTIVE -> QUIESCING -> PREPARED -> SETTLING
                           |           |                         |-- SETTLED
                           |           |                         |-- PARTIALLY_SETTLED
                           |           |                         `-- RECOVERY_REQUIRED
                           |           |-- repairable invariant --> SUSPENDED
                           |           |   (REPLAN_REQUIRED)             |
                           |           |                                `-- intent/resume,
                           |           |                                    fresh generation -> ACTIVE
                           |           `-- ambiguity / hard failure --> ABORTED | RECOVERY_REQUIRED
                           |-- intent/amend -> SUSPENDED -> ACTIVE | ABORTED
                           `-- abort / revoke / expiry / failure --> ABORTED | RECOVERY_REQUIRED
```

`PREPARED` is an internal durable state. There is no model-facing `intent/prepare` method.

Tool admission closes on quiescence, abort, expiry, or revocation. `EXPIRED` and `REVOKED` are recorded as termination reasons, not proof of effect cleanup. Terminal effect outcomes are `SETTLED`, `ABORTED`, `PARTIALLY_SETTLED`, `RECOVERY_REQUIRED`, and `RECONCILED`. `ABORTED` is safe to report only when every adapter proves that no final settlement effect escaped and every provisional effect was canceled or rolled back. Otherwise the outcome is degraded or requires recovery.

A framework root return is a **candidate completion**, not authoritative success. The supervisor withholds the candidate result, requests quiescence, and releases it only after a signed terminal settlement receipt. Clared tracks outstanding co-requisites, approvals, checkpoints, and required actions while the epoch is active. Certified progressive-guidance profiles surface relevant changes through the ordinary tool-result path so the agent can correct course before completion; any still open at quiescence become finalization blockers.

## 5. Tool admission and ordering

Tool calls carry a namespaced metadata object:

```json
{
  "jsonrpc": "2.0",
  "id": "call-102",
  "method": "tools/call",
  "params": {
    "name": "billing.refunds.stage",
    "arguments": {
      "invoice_id": "inv_1042",
      "amount_minor": 45000
    },
    "_meta": {
      "ai.clared/execution": {
        "operation_id": "op_01J8F9A2B3C4D5E6",
        "capability_token": "clared-action-v2...",
        "action_id": "stage-refund-1042",
        "idempotency_key": "stage-refund-1042-attempt-1"
      }
    }
  }
}
```

`action_id` and `idempotency_key` are untrusted correlation values. The caller MUST NOT supply the ordering authority.

Successful admission is one linearizable operation that:

1. validates the active action capability and generation;
2. verifies lifecycle, tool, resource, policy, invariant, and budget constraints;
3. reserves typed budget and idempotency state;
4. allocates the next monotonic `admission_seq`; and
5. persists the admission record before adapter dispatch.

The returned tool result includes the assigned sequence. When the action opens or satisfies a co-requisite, the same result carries a signed obligation delta and canonical model-visible rendering:

```json
{
  "content": [
    {
      "type": "text",
      "text": "Refund rf_1042 was staged; no funds have settled."
    },
    {
      "type": "text",
      "text": "CLARED OBLIGATION: Create exactly one matching ledger entry for invoice inv_1042 before finalization. Do not repeat the refund."
    }
  ],
  "_meta": {
    "ai.clared/execution": {
      "operation_id": "op_01J8F9A2B3C4D5E6",
      "admission_seq": 7,
      "stage_status": "SUCCEEDED_STAGED",
      "obligation_revision": 8,
      "obligation_delta": {
        "opened": [
          {
            "obligation_id": "obl_ledger_entry_1042",
            "resolution_kind": "EXACT_ACTION",
            "required_action": "acme.ledger.entries.create@v1",
            "resource": "urn:acme:invoice:inv_1042",
            "argument_constraints": {
              "invoice_id": "inv_1042",
              "refund_id": "rf_1042"
            },
            "blocking_at": "QUIESCE"
          }
        ],
        "satisfied": []
      },
      "open_obligations": [
        {
          "obligation_id": "obl_ledger_entry_1042",
          "resolution_kind": "EXACT_ACTION",
          "required_action": "acme.ledger.entries.create@v1",
          "resource": "urn:acme:invoice:inv_1042",
          "argument_constraints": {
            "invoice_id": "inv_1042",
            "refund_id": "rf_1042"
          }
        }
      ],
      "obligation_receipt_hash": "sha256_9200f2...",
      "obligation_receipt_signature": "ed25519_4a2c91..."
    }
  }
}
```

Recovery must distinguish an admission persisted before dispatch from a dispatched action with an unknown response. It must never infer success merely because an admission record exists.

### 5.1 Progressive obligation semantics

Obligations are authenticated execution-graph state, not prompt text and not authority. A later admitted action may produce a `SATISFIED` delta. Waiver requires a policy-authorized principal; cancellation is valid only when the source staged effect is proven canceled or rolled back. Tool arguments, provider text, or model claims cannot resolve the graph by themselves.

Each obligation declares one resolution kind:

1. `EXACT_ACTION`: one admitted tool/resource shape and argument constraints are known;
2. `OUTCOME_CONSTRAINT`: the required state is known, but the agent may choose among allowed trajectories;
3. `APPROVAL_OR_CHECKPOINT`: trusted external authority or durable state is required; or
4. `NOT_MODEL_REPAIRABLE`: ambiguity or a hard safety failure follows abort/recovery.

The model-visible block MUST be rendered by Clared from approved templates and authenticated facts. Untrusted provider output cannot be promoted into it. Guidance never grants the named action; normal capability, policy, resource, budget, and adapter admission still apply.

A profile declares `obligation_delivery` as `TOOL_RESULT`, `TOOL_RESULT_AND_RUNTIME_STATE`, or `UNSUPPORTED`. A supported certified adapter proves that canonical guidance reaches the model and complete structured state reaches the supervisor. While obligations remain open, every subsequent model-visible tool result carries the current snapshot. Snapshots are bounded and relevance-filtered: they omit resolved history, policy source, and unnecessary sensitive facts. A blocking obligation is never silently truncated; disclosure/token-limit exhaustion suspends or fails closed. If the client strips either view, enforcement remains active but the profile cannot claim progressive self-correction.

An optional deterministic closure recipe may stage an `EXACT_ACTION` without another model call only when the recipe is explicitly declared, idempotent, fully parameterized from authenticated state, and accepted by the ordinary action pipeline. It produces the same budget, idempotency, evidence, and settlement records as an agent-selected action.

## 6. `intent/quiesce`

The trusted supervisor calls `intent/quiesce` only after the bound root invocation returns successfully according to the certified framework contract. That event creates a candidate result; it does not establish settlement success:

```json
{
  "jsonrpc": "2.0",
  "id": "quiesce-1",
  "method": "intent/quiesce",
  "params": {
    "operation_id": "op_01J8F9A2B3C4D5E6",
    "root_invocation_id": "run_langgraph_01J8FB7M8K",
    "supervisor_credential": "clared-supervisor-v2...",
    "completion_kind": "ROOT_SUCCEEDED",
    "idempotency_key": "quiesce-op-01J8F9A2B3C4D5E6-1"
  }
}
```

The request omits a last action sequence and child manifest because Clared owns those facts.

### 6.1 Linearization rules

Under the operation's linearizable state lock, Clared MUST:

1. validate lifecycle authority, root binding, generation, completion kind, and idempotency;
2. transition `ACTIVE -> QUIESCING`;
3. advance the fence generation and close admission for the previous generation;
4. snapshot the Clared-owned `admission_high_watermark` and registered child set; and
5. durably persist the fence before acknowledging it.

A racing tool call is either admitted with `admission_seq <= admission_high_watermark` or rejected with `ADMISSION_FENCED`. It cannot be missing from both outcomes.

An action admitted before the fence drains under its recorded admission decision. Generation invalidation prevents new admission; it does not retroactively erase already-admitted work or require an in-flight provider call to reauthenticate when its response returns.

Duplicate quiescence with the same idempotency key returns the same fence and settlement operation. Conflicting replay is rejected.

### 6.2 Fence acknowledgment

```json
{
  "jsonrpc": "2.0",
  "id": "quiesce-1",
  "result": {
    "status": "QUIESCING",
    "operation_id": "op_01J8F9A2B3C4D5E6",
    "fence_generation": 3,
    "admission_high_watermark": 12,
    "registered_children": 2,
    "settlement_operation_id": "settle_01J8FC0D3Y"
  }
}
```

The supervisor may await or poll the same operation until either a signed terminal receipt or a signed repair suspension is available.

## 7. Drain and internal preparation

Clared may enter `PREPARED` only when:

- every action through the admission high-water mark has a terminal, unambiguous stage outcome;
- every child in the snapshotted registered set is `PREPARED` or safely terminal;
- no detached child can use an unfenced capability;
- budgets, authenticated approvals, provider state, cross-action invariants, and co-requisites revalidate;
- every required physical adapter lease remains valid; and
- the complete settlement intent and idempotency material are durable in the WAL.

An ambiguous provider response blocks preparation or enters `RECOVERY_REQUIRED`. A timer may bound how long Clared waits, but expiration takes an abort/recovery path, never a success path.

If revalidation finds an obligation that progressive guidance did not resolve or could not expose, Clared remains pre-prepare and returns a signed suspension result rather than preparing or releasing the candidate output:

```json
{
  "jsonrpc": "2.0",
  "id": "quiesce-1",
  "result": {
    "status": "SUSPENDED",
    "suspension_reason": "REPLAN_REQUIRED",
    "suspension_id": "susp_01J8FC2E8R",
    "operation_id": "op_01J8F9A2B3C4D5E6",
    "fence_generation": 3,
    "admission_high_watermark": 12,
    "repair_mode": "CONTINUE_EPOCH",
    "candidate_output_disposition": "DISCARD",
    "resume_authorization_id": "resume_auth_01J8FC3A9W",
    "obligation_revision": 8,
    "outstanding_obligations": [
      {
        "obligation_id": "obl_ledger_entry_1042",
        "outcome": "REQUIRE_ACTION",
        "resolution_kind": "EXACT_ACTION",
        "required_action": "acme.ledger.entries.create@v1",
        "resource": "urn:acme:invoice:inv_1042",
        "argument_constraints": {
          "invoice_id": "inv_1042",
          "refund_id": "rf_1042"
        }
      }
    ],
    "steering_receipt_hash": "sha256_622f80...",
    "steering_receipt_signature": "ed25519_8c110a...",
    "repair_attempt": 1,
    "max_repair_attempts": 3
  }
}
```

Provider ambiguity, an expired physical lease, and a hard policy violation are not agent-steering events. They follow abort or recovery rules.

### 7.1 Fenced terminal repair and `intent/resume`

Terminal validation classifies a blocker as one of:

1. `CONTINUE_EPOCH`: monotonic additive work that does not invalidate an earlier admission and can complete within every remaining adapter lease;
2. approval that does not expand authority, which may resume only after authenticated approval;
3. `RESTART_FROM_CHECKPOINT`: non-monotonic repair that first safely rolls back or aborts the staged branch and creates a linked successor operation from durable state; or
4. `ABORT_TERMINAL`: repair is unsafe, unavailable, ambiguous, or exhausted.

`intent/resume` is supervisor-only, idempotent, and bound to the suspension and steering receipt:

```json
{
  "jsonrpc": "2.0",
  "id": "resume-1",
  "method": "intent/resume",
  "params": {
    "operation_id": "op_01J8F9A2B3C4D5E6",
    "suspension_id": "susp_01J8FC2E8R",
    "steering_receipt_hash": "sha256_622f80...",
    "previous_root_invocation_id": "run_langgraph_01J8FB7M8K",
    "continuation_invocation_id": "run_langgraph_01J8FD1A6Q",
    "supervisor_credential": "clared-supervisor-resume-v2...",
    "idempotency_key": "resume-susp-01J8FC2E8R-1"
  }
}
```

For same-epoch continuation, Clared may transition `SUSPENDED -> ACTIVE` only after prior admissions have drained, registered children are safe, adapter state remains reusable, all required leases remain valid, and the profile's repair limits allow another attempt. It issues a fresh, greater action generation; the fenced generation is never reopened. The new capability MAY be narrowed to the unresolved obligation's safe tool/resource subset. Resume cannot widen tools, resources, budgets, or principal authority. Any widening uses authenticated approval plus `intent/amend`.

The request's `supervisor_credential` is the rotated credential associated with `resume_authorization_id`, not the credential that requested quiescence. On success, Clared returns an idempotent result containing the fresh `action_generation`, continuation binding, and action capability. The supervisor installs that capability into private continuation context; it never places it in the model-visible steering event.

Profiles MUST bound repair attempts, cumulative wall time, and model/tool budgets. A suspension never extends a database transaction or provider reservation lease. If the repair cannot safely finish within the remaining lease, the operation restarts from a durable checkpoint or aborts.

## 8. Parent and child epochs

Clared registers the parent-child relationship before issuing the child action capability and budget lease. The parent cannot authoritatively enumerate children at quiescence time.

A child quiesces and prepares independently. Parent preparation waits for all registered children represented by the fence snapshot. If a child crashes or its budget lease expires, unused budget is released only after Clared proves no admitted child use remains ambiguous. Any child or background task attempting action after the parent fence with a stale generation is denied.

## 9. Failure, cancellation, and streaming

- Root exception or cancellation fences new admission and initiates abort/recovery.
- Normal exhaustion of a root result stream may produce candidate completion only when the certified framework adapter defines exhaustion as the root's authoritative terminal event.
- Consumer cancellation, generator exception, or abandoned iteration is not success.
- Process death in `ACTIVE` initiates rollback/recovery. Process death after durable preparation resumes the recorded settlement plan idempotently.
- eBPF and socket telemetry may detect death or bypass paths, but never acts as a completion oracle.

Authoritative application success MUST follow the terminal settlement receipt. The integration either buffers final output until settlement completes or labels earlier output provisional and binds the later terminal receipt to it. A `RESUME_CAPABLE` adapter may append the authenticated steering result to framework state and safely resume or re-enter from a verified checkpoint. A `BLOCK_ONLY` adapter cannot transparently steer the model. Unless a separately configured durable orchestrator explicitly accepts ownership of the suspension, it aborts or reconciles staged effects before returning a structured finalization failure and never strands a physical lease. Failure to deliver output after successful settlement does not undo settlement; the receipt must make the delivery failure distinguishable from execution failure.

## 10. Expiry domains

The following clocks are independent and require separate configuration and telemetry:

- logical operation/session TTL;
- action and supervisor credential expiry;
- child budget lease;
- physical database transaction lease; and
- provider reservation or draft expiry.

None may transition the epoch to `PREPARED` or `SETTLED`. Expiry causes denial, rollback, cancellation, abort, or explicit recovery according to the affected adapter and lifecycle state.

## 11. Other lifecycle methods

- `intent/status` reports lifecycle state, fence generation, admission high-water mark, in-flight admissions, registered children, budgets, physical leases, provider reservations, obligation revision and outstanding obligations, suspension/repair counters, settlement operation, and receipt.
- `intent/amend` fences the current generation, drains already-admitted work, and suspends the epoch while a trusted principal decides. A physical database transaction must be rolled back or converted to a durable checkpoint before an unbounded human pause.
- `intent/abort` is supervisor- or policy-authorized and follows adapter evidence to decide between `ABORTED` and recovery.
- `intent/revoke` is an administrative kill switch that fences parent and child authority immediately and then follows the same evidence-based abort/recovery rules.

## 12. Compatibility with `v0alpha1`

The implemented `v0alpha1` method `intent/seal` is an explicit, block-only harness boundary and remains unchanged in that profile. Terminal revalidation failure aborts its simulated staging and returns an error; it has no `intent/resume`. An implementation of this draft MAY expose `intent/seal` as a versioned compatibility alias only to a lifecycle-authorized supervisor. The alias MUST perform the same admission fence, child snapshot, drain, prepare, and idempotency behavior as `intent/quiesce`; an action capability cannot call it.

## 13. Error registry additions

| Code | Meaning |
| --- | --- |
| `-32007` | Lifecycle authority required |
| `-32008` | Admission fenced by quiescence, abort, or revocation |
| `-32009` | Draining action outcome is ambiguous |
| `-32010` | Unsupported or uncertified live runtime boundary |
| `-32011` | Physical adapter lease expired |
| `-32012` | Resume is unsafe for the suspended adapter or checkpoint state |
| `-32013` | Repair attempt, time, model, or tool budget exhausted |
| `-32014` | Parent or child lifecycle conflict |

Final numeric allocation is subject to interoperability review before promotion from draft.

## 14. Required conformance scenarios

A live-capable implementation MUST publish tests for:

1. a tool admission racing quiescence;
2. duplicate and conflicting quiescence replay;
3. an action capability attempting a lifecycle operation;
4. concurrent child completion in multiple orders;
5. detached child work after the parent fence;
6. root exception and cancellation;
7. an async stream abandoned before authoritative exhaustion;
8. process death in `ACTIVE` and after durable preparation;
9. a human pause after a database mutation;
10. physical database and provider reservation lease expiry;
11. session TTL without quiescence;
12. an unsupported runtime requesting live mode;
13. an ambiguous provider result during drain;
14. settlement success followed by final-output delivery failure;
15. candidate output withheld when finalization is rejected;
16. monotonic additive repair followed by successful re-quiescence and settlement;
17. non-monotonic repair forced to checkpoint restart rather than same-epoch mutation;
18. duplicate and conflicting `intent/resume`, including stale work from the previous generation;
19. repair attempt, wall-time, and model/tool budget exhaustion;
20. a block-only framework adapter that prevents false success and cleans up staging without claiming transparent steering; and
21. physical or provider lease expiry while suspended;
22. an obligation opened and satisfied inside the original tool loop;
23. multiple deltas plus a compact snapshot that retains an earlier open obligation;
24. forged satisfaction, waiver, or obligation-revision replay;
25. provider-controlled text attempting to inject canonical guidance;
26. an outcome-only obligation that does not invent or grant a tool action;
27. deterministic closure replay with normal budget and evidence accounting; and
28. a runtime that strips guidance and is downgraded rather than certified for progressive self-correction; and
29. obligation disclosure or token limits reached without silently omitting a blocker.

## 15. Non-guarantees

This draft does not make arbitrary third-party APIs transactional, make an incorrect adapter safe, infer unobservable business intent, guarantee that a model follows obligation guidance, guarantee that every framework can resume, or eliminate partial outcomes. It defines a deterministic admission boundary and an evidence-preserving way to guide an active trajectory, validate it, repair where safe, and coordinate adapter-defined settlement after a trusted semantic candidate-completion signal.
