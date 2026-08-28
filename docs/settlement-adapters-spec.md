# Clared Settlement Adapters Specification

> **Status:** Reference Specification  
> **Target Audience:** Integration developers, API authors  

---

## 1. Abstract

Settlement Adapters define how external tools execute across Clared's 4 Execution Modes. They map target APIs to staging hooks, settlement capture methods, rollback endpoints, and saga compensators.

---

## 2. The 4 Execution Modes (+ Egress Sinks)

| Mode | Staging Hook | Settlement Hook (`intent/seal`) | Abort Hook (`intent/abort`) | Example APIs |
| :--- | :--- | :--- | :--- | :--- |
| **`MODE_1_SQL`** | Pinned connection (`BEGIN ... SAVEPOINT`) | `COMMIT` | `ROLLBACK` | PostgreSQL, MySQL, SQLite |
| **`MODE_2_MOCK`** | Synthetic Token (`virt_*`) in RAM | Execute live write | Discard RAM buffer | Internal CRUD tools |
| **`MODE_3_RESERVATION`** | `AUTH_HOLD` (`capture_method=manual`) | `CAPTURE` hold | `CANCEL` hold | Stripe, HubSpot Deals |
| **`MODE_4_CHECKPOINT`** | Pre-flight policy simulation | Irreversible live execution | Execute Saga Compensator | Wire transfers, Cloud teardowns |
| **`EGRESS_SINK`** | RAM Buffer (Topological Delay) | Live Network Dispatch | Discard RAM buffer | Twilio SMS, Slack, SendGrid |

---

## 3. Degraded Settlement & Partial Outcome Handling

When an upstream network partition occurs during settlement flush:
1. State is immediately isolated as `PARTIALLY_SETTLED`.
2. Clared runs the declared `compensation` hook in the adapter.
3. If unrecoverable, an Ed25519-signed incident receipt is generated and flagged as `RECOVERY_REQUIRED` for human review.
