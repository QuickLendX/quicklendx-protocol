# Investment Lifecycle Diagram

This document provides a comprehensive state machine for investments within the QuickLendX protocol. It is primarily written for **contributors** to help them verify behavior against documented intent and easily get up to speed on the core contract lifecycle.

## Overview

The core of an investment revolves around its status as an invoice is financed and eventually settled. 

```mermaid
stateDiagram-v2
    [*] --> Pending
    Pending --> Active : Invoice Funded
    Active --> Repaid : Borrower repays on time
    Active --> PastDue : Deadline missed
    PastDue --> Grace : Grace period begins
    Grace --> Repaid : Borrower repays late but within grace
    Grace --> Default : Grace period ends without repayment
    Default --> Recovery : Administrative recovery initiated
    Recovery --> Settled : Partial or full recovery completed
    Repaid --> [*]
    Settled --> [*]
```

## Concrete Examples

### 1. Happy Path: Funding to Repayment

An invoice is listed, investors fund it, and the borrower repays on time.

*   **Entrypoint:** `fund_invoice(env, investor_id, invoice_id, amount)`
    *   *Resulting State:* The investment becomes `Active`.
*   **Entrypoint:** `repay_invoice(env, invoice_id, amount)`
    *   *Resulting State:* The investment transitions from `Active` to `Repaid`.

### 2. Default Flow: The Sad Path

The borrower misses the deadline, triggering the default sequence.

*   **State:** `Active` -> `PastDue`
    *   *Trigger:* A time-based condition (ledger time) is met without repayment.
*   **State:** `PastDue` -> `Grace`
    *   *Trigger:* `enter_grace_period(env, invoice_id)` transitions it, allowing a specific timeframe for late payments.
*   **State:** `Grace` -> `Default`
    *   *Trigger:* The grace period expires. `mark_default(env, invoice_id)` transitions the investment.
*   **State:** `Default` -> `Recovery`
    *   *Trigger:* Administrators or automated systems initiate recovery procedures via `start_recovery(env, invoice_id)`.

## Notes for Contributors

-   **Deterministic Time:** Remember that transitions often rely on ledger time. Ensure tests accurately mock or fast-forward time to test boundary conditions (e.g., exactly at the grace period expiration).
-   **No `std::`:** When implementing these state transitions in the smart contract, always adhere to `#![no_std]` and rely on the Soroban SDK.
