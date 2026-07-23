# KYC Guide for Operators

This document explains the two types of KYC (Know Your Customer) used in QuickLendX: Business KYC and Investor KYC. As an operator, you will manage both processes to ensure platform compliance.

## Business KYC
Business KYC is required for businesses that want to create invoices on the platform. It gates the ability to list invoices and receive funds.

**What it gates:**
- Creating new invoices
- Receiving funds from investors
- Updating business profile details

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
