# Clared Execution Envelope

**Version:** `clared.dev/execution-envelope/v0alpha1`  
**Status:** Experimental reference specification  
**License:** Apache-2.0

## 1. Scope

An execution envelope establishes session-level authority for a non-deterministic agent operation. It complements per-call authorization by binding a trajectory to:

- An authenticated delegator and tenant.
- An agent role and task intent.
- An explicit tool allowlist.
- Target resource identifiers.
- Integer, multi-dimensional aggregate budgets.
- A short lifetime and monotonic capability generation.

The envelope is a constraint contract, not a fixed execution plan. The agent may choose actions dynamically inside the admitted bounds. Implementations may construct a dependency graph incrementally from staged actions and trusted effects, but that graph does not grant authority beyond the envelope.

The envelope does not define model reasoning, agent discovery, provider credentials, or universal distributed transactions.

An enforcing deployment must withhold downstream credentials from the agent and route every mutating path through the boundary. It can enforce only policies and effects that are formalized and observable; it does not infer correct business intent by itself.

## 2. Security boundary

The agent is an untrusted executor. A trusted harness authenticates the delegator and calls `intent/propose`. The reference implementation uses a single-use HMAC-SHA256 delegation proof shared between the harness and local proxy. A consumed token cannot admit or amend another session. Production implementations should integrate an enterprise identity provider and verify equivalent signed claims.

On admission, the proxy issues an Ed25519-signed capability token containing:

- `session_id`
- `tenant_id`
- `principal`
- `generation`
- `issued_at_ms`
- `expires_at_ms`
- `jti`

Every session method verifies the signature, active token, claims, generation, expiry, and lifecycle state. Possession of a session ID is not authority.

## 3. Lifecycle

```text
PROPOSED -> ADMITTED -> ACTIVE -> SEALING -> SETTLED
                         │   │
                         │   └──────────────> ABORTED
                         └-> SUSPENDED -> ACTIVE  (intent/amend)

Any non-terminal state -> EXPIRED
```

`SETTLED`, `ABORTED`, `EXPIRED`, and `REVOKED` are terminal for tool execution. Repeating a terminal method is allowed only with the same idempotency key and returns the cached receipt.

## 4. Methods

### `intent/propose`

```json
{
  "jsonrpc": "2.0",
  "id": "proposal-1",
  "method": "intent/propose",
  "params": {
    "delegation_token": "clared-delegation-v1...",
    "tenant_id": "acme",
    "principal": "alice",
    "agent_role": "checkout_agent",
    "task_intent": "authorize_order_1042",
    "target_resources": ["order:ord_1042", "customer:cus_9918"],
    "allowed_tools": [
      "postgres.orders.update",
      "stripe.payment_intents.create"
    ],
    "budgets": {
      "database.mutations.count": 1,
      "money.minor.USD.hold": 50000,
      "money.minor.USD.capture": 50000
    },
    "ttl_ms": 30000
  }
}
```

Admission fails closed if delegation is invalid or already consumed, the TTL is outside implementation bounds, the tool list is empty, any tool lacks a registered adapter, or a resource-bearing tool has no qualified target scope.

### `tools/call`

```json
{
  "jsonrpc": "2.0",
  "id": "call-1",
  "method": "tools/call",
  "params": {
    "name": "postgres.orders.update",
    "arguments": {"order_id": "ord_1042", "status": "authorized"},
    "_clared_meta": {
      "session_id": "ses_...",
      "capability_token": "clared-cap-v1...",
      "generation": 1,
      "idempotency_key": "order-1042-update-v1"
    }
  }
}
```

Evaluation order:

1. Verify capability signature, active token, claims, generation, and expiry.
2. Require `ADMITTED` or `ACTIVE` state.
3. Verify tool allowlist and adapter registration.
4. Return a cached result for an identical idempotent replay; reject key reuse with a different request.
5. Qualify adapter-declared resource arguments with their scope type and match every value to an envelope target.
6. Evaluate deterministic policy independently for every matched resource. Reserved admission and approval context comes from the trusted proxy, not tool arguments.
7. Reserve every adapter-declared budget charge atomically.
8. Stage the adapter action.

Monetary dimensions use non-negative integers in minor units. Floating-point monetary arguments are invalid.

### `intent/amend`

An amendment requires the current capability and a fresh valid delegation proof. The session is fenced, budgets are widened, the generation increments, and a new signed capability replaces the previous token. The old token immediately fails active-token and generation checks.

### `intent/seal`

Seal requires an idempotency key. The implementation revalidates policy for every staged action, performs adapter settlement in the declared execution backend, and returns outcome evidence. A repeated seal with the same key returns the same receipt; a different key is rejected.

### `intent/abort`

Abort requires an idempotency key and valid active capability. It discards staged work through adapter rollback semantics. It cannot change an already settled session.

## 5. Evidence

The reference receipt contains a JSON evidence object, `sha256:<hex>` digest, `ed25519:<base64url>` signature over the digest, and the base64url public key. The current profile signs the deterministic `serde_json` serialization used by the Rust implementation; it does not yet define a cross-implementation canonical JSON format. Verifiers must reconstruct the bytes for the implementation profile exactly.

## 6. Error registry

| Code | Meaning |
| --- | --- |
| `-32602` | Invalid method parameters |
| `-32603` | Internal implementation or adapter error |
| `-32001` | Insufficient typed budget |
| `-32002` | Resource outside envelope |
| `-32003` | Tool outside envelope |
| `-32004` | Invalid, stale, or expired capability |
| `-32005` | Unknown session |
| `-32006` | Policy invariant violation |
| `-32007` | Missing settlement adapter |
| `-32008` | Invalid lifecycle transition |
| `-32009` | Idempotency conflict |
| `-32010` | Invalid, expired, or replayed delegation proof |

## 7. Non-guarantees

This protocol does not make irreversible third-party APIs atomic. An execution backend must expose partial outcomes honestly and must not report `SETTLED` unless its adapter settlement rules succeeded. The current Clared backend is explicitly an in-memory simulator and contacts no providers.
