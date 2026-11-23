# Insurance Claim Time Limits

This document details the time limits for filing an insurance claim on the QuickLendX platform following an invoice default.

## Claim Filing Window

Once an invoice enters the \Defaulted\ state, the investor has a strict, contract-enforced window to file an insurance claim. This prevents indefinite liabilities for the insurance pool and ensures prompt dispute resolution and recovery actions.

### 1. Primary Filing Window (30 Days)
The primary window to file a claim is exactly **30 days** (or the equivalent ledger sequence count) starting from the timestamp the invoice status changed to \Defaulted\.

If a claim is submitted within this window, it is processed normally according to the investor's coverage tier.

### 2. Grace Period (Late Filing)
Claims submitted after 30 days but before **60 days** are considered "late". While the contract may still accept the claim, a late penalty or reduced payout ratio may be applied by the insurance administrator depending on the specific insurance tier terms.

### 3. Expiration (After 60 Days)
After **60 days** from the default timestamp, the right to file an insurance claim on the invoice is permanently forfeited. Any attempt to invoke the \ile_claim\ entrypoint will be rejected with an expiration error (e.g., \QuickLendXError::ClaimExpired\).

## Implementation Notes

- The time elapsed is calculated using the on-chain ledger timestamp (Unix epoch time).
- Investors are strongly encouraged to use the QuickLendX frontend dashboard, which surfaces default alerts and remaining claim windows.
- In the event of a platform pause or maintenance window that overlaps with a claim deadline, the protocol admin may manually extend claim windows or honor late claims on a case-by-case basis through admin overrides.
