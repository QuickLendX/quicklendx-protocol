# Escrow Refund Security Hardening

This document outlines the security improvements made to the protocol's escrow refund mechanism.

## Authorization Matrix

The `refund_escrow_funds` operation now enforces an explicit authorization matrix. Mandatory `caller.require_auth()` is called at the beginning of the transaction.

| Caller | Authorized? | Rationale |
| :--- | :--- | :--- |
| **Contract Admin** | Yes | Required for emergency dispute resolution and protocol maintenance. |
| **Business Owner** | Yes | The business that uploaded the invoice is the owner of the deal. |
| **Investor** | No | Investors are the recipients of releases, not refund triggers. |
| **Others** | No | No unauthorized parties should be able to trigger movement of funds. |

## Status Invariants

To prevent unauthorized fund theft or protocol state corruption, strict status invariants are enforced at both the entry point and the payment logic layer.

### 1. Invoice Status
Refunds are strictly permitted ONLY when the invoice is in the **`Funded`** status.
- Once an invoice is `Paid`, `Cancelled`, or `Refunded`, no further refund operations are allowed.
- This prevents double-refunds and protects settled funds.

### 2. Escrow Status
The internal payment layer validates that the escrow record is in the **`Held`** status.
- If the escrow has already been released or refunded, the transaction will revert.

## Security Assumptions

- **Admin Control**: The protocol relies on the integrity of the designated Admin address.
- **State Consistency**: `InvoiceStorage` and `EscrowStorage` are assumed to be synchronized.
- **Reentrancy**: All refund operations are protected by the `with_payment_guard` reentrancy protection.
