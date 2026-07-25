# KYC Guide for Operators

This document explains the two types of KYC (Know Your Customer) used in QuickLendX: Business KYC and Investor KYC. As an operator, you will manage both processes to ensure platform compliance.

## Business KYC
Business KYC is required for businesses that want to create invoices on the platform. It gates the ability to list invoices and receive funds.

Only businesses with a `Verified` KYC status — referred to as **tier-N businesses** — may call `store_invoice` or `upload_invoice`. This check is enforced at the **contract level**: no front-end bypass can circumvent it.

**What it gates:**
- Creating new invoices (`store_invoice`, `upload_invoice`)
- Receiving funds from investors
- Updating business profile details

**Error codes returned by the contract:**

| KYC state | Error returned |
|---|---|
| No record (unknown address) | `BusinessNotVerified` (1600) |
| `Pending` (awaiting admin review) | `KYCAlreadyPending` (1601) |
| `Rejected` | `BusinessNotVerified` (1600) |
| `Verified` ✓ | allowed to proceed |

The distinction between `KYCAlreadyPending` and `BusinessNotVerified` lets callers
give actionable feedback: "your application is under review" vs. "you have not
submitted KYC".

**Concrete Example:**
When a new business signs up, they cannot list an invoice until their KYC status is updated to `Verified`.
```json
{
  "business_id": "B-12345",
  "kyc_status": "Verified",
  "max_invoice_limit": 50000
}
```


## Investor KYC
Investor KYC is required for users who want to fund invoices. It gates the ability to place bids and earn yields.

**What it gates:**
- Placing bids on invoices
- Withdrawing funds
- Viewing detailed business financials

**Concrete Example:**
An investor attempting to place a bid of $10,000 on an invoice will be blocked if their KYC status is `Pending`.
```json
{
  "investor_id": "I-67890",
  "kyc_status": "Verified",
  "investment_limit": 100000
}
```
