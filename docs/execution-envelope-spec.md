# Clared Execution Envelope Specification

> **Status:** Reference Specification  
> **Target Audience:** Systems engineers, harness authors, security teams  

---

## 1. Abstract

An Execution Envelope establishes bounded runtime constraints around an autonomous agent session. Rather than relying on prompt instructions to restrict behavior, the agent runtime (or harness) proposes an envelope before execution begins.

The Clared kernel evaluates the proposal against local Cedar policies and issues a signed capability token. Subsequent tool calls present this token and execute against staged isolation primitives.

---

## 2. Core Invariants

1. **Aggregate Multi-Dimensional Budgets**: Integer minor-unit caps (such as `money.minor.USD.capture: 50000` for $500.00) tracked on a Write-Ahead Log.
2. **Resource Scoping**: Whitelist of allowed resource identifiers (such as `["customer:cus_9918"]`).
3. **Session Generation Fencing**: Mid-flight amendments increment the generation counter (`gen: 1 -> gen: 2`), invalidating previous capability tokens across swarm subagents.
4. **Commit-Time Revalidation**: Invariants are checked both upon tool invocation and during final session settlement (`intent/seal`).

---

## 3. Session Wire Methods

* `intent/propose`: Opens an envelope with bounded resources, budgets, and allowed tool names.
* `tools/call`: Executes a tool presenting capability token metadata (`_dtbe_meta`).
* `intent/amend`: Requests privilege expansion with human approval, incrementing generation.
* `intent/seal`: Closes the execution epoch and initiates coordinated settlement.
* `intent/abort`: Discards staged memory buffers, cancels upstream holds, and rolls back database transactions.
