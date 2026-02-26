# Error Handling — `QuickLendXError`

## Overview

Every public contract function returns `Result<T, QuickLendXError>`.
The contract **never panics on invalid input**; all failure paths produce a typed error that callers can match on.

Error codes are grouped into ranges so integrators can quickly identify the category of failure without inspecting the variant name.

> **XDR limit:** The Soroban contract spec allows a maximum of **50 error variants** per contract.
> All 50 slots are currently occupied. Adding new variants requires replacing an existing one.

## Error Code Ranges

| Range | Category |
|-------|----------|
| 1000 – 1006 | Invoice lifecycle |
| 1100 – 1103 | Authorization |
| 1200 – 1204 | Input validation |
| 1300 – 1301 | Storage |
| 1400 – 1405 | Business logic |
| 1500 – 1503 | Rating |
| 1600 – 1604 | KYC / verification |
| 1700 – 1702 | Audit |
| 1800 – 1801 | Category / tag |
| 1850 – 1852 | Fee configuration |
| 1900 – 1906 | Dispute |
| 2000 – 2001 | Notification |

---

## Invoice Lifecycle Errors (1000 – 1006)

| Code | Variant | Symbol | Description |
|------|---------|--------|-------------|
| 1000 | `InvoiceNotFound` | `INV_NF` | The specified invoice ID does not exist in storage. |
| 1001 | `InvoiceNotAvailableForFunding` | `INV_NAF` | Invoice is not in a state that allows funding (wrong status). |
| 1002 | `InvoiceAlreadyFunded` | `INV_AF` | Invoice has already been funded by an investor. |
| 1003 | `InvoiceAmountInvalid` | `INV_AI` | Invoice amount is invalid (zero or negative). |
| 1004 | `InvoiceDueDateInvalid` | `INV_DI` | Invoice due date is in the past or otherwise invalid. |
| 1005 | `InvoiceNotFunded` | `INV_NFD` | Invoice has not been funded; operation requires a funded invoice. |
| 1006 | `InvoiceAlreadyDefaulted` | `INV_AD` | Invoice has already been marked as defaulted. |

---

## Authorization Errors (1100 – 1103)

| Code | Variant | Symbol | Description |
|------|---------|--------|-------------|
| 1100 | `Unauthorized` | `UNAUTH` | Caller is not authorized for this operation. |
| 1101 | `NotBusinessOwner` | `NOT_OWN` | Caller is not the business owner of this invoice. |
| 1102 | `NotInvestor` | `NOT_INV` | Caller is not a registered investor. |
| 1103 | `NotAdmin` | `NOT_ADM` | Caller is not the contract admin. |

---

## Input Validation Errors (1200 – 1204)

| Code | Variant | Symbol | Description |
|------|---------|--------|-------------|
| 1200 | `InvalidAmount` | `INV_AMT` | Amount is invalid (zero, negative, or exceeds the permitted limit). |
| 1201 | `InvalidAddress` | `INV_ADR` | Address is invalid or does not meet format requirements. |
| 1202 | `InvalidCurrency` | `INV_CR` | Currency token address is not on the whitelist. |
| 1203 | `InvalidTimestamp` | `INV_TM` | Timestamp is invalid (e.g., in the past where a future value is required). |
| 1204 | `InvalidDescription` | `INV_DS` | Description is empty, too short, or exceeds the maximum length. |

---

## Storage Errors (1300 – 1301)

| Code | Variant | Symbol | Description |
|------|---------|--------|-------------|
| 1300 | `StorageError` | `STORE` | A general storage read/write error occurred. |
| 1301 | `StorageKeyNotFound` | `KEY_NF` | The requested storage key does not exist. |

---

## Business Logic Errors (1400 – 1405)

| Code | Variant | Symbol | Description |
|------|---------|--------|-------------|
| 1400 | `InsufficientFunds` | `INSUF` | Caller or escrow account has insufficient funds. |
| 1401 | `InvalidStatus` | `INV_ST` | Invoice or operation is in an invalid status for the requested action. |
| 1402 | `OperationNotAllowed` | `OP_NA` | The operation is not permitted in the current contract state. |
| 1403 | `PaymentTooLow` | `PAY_LOW` | Payment amount is below the required minimum. |
| 1404 | `PlatformAccountNotConfigured` | `PLT_NC` | Platform fee recipient account has not been configured. |
| 1405 | `InvalidCoveragePercentage` | `INS_CV` | Insurance coverage percentage is out of the allowed range (0–100). |

---

## Rating Errors (1500 – 1503)

| Code | Variant | Symbol | Description |
|------|---------|--------|-------------|
| 1500 | `InvalidRating` | `INV_RT` | Rating value is outside the accepted range (1–5). |
| 1501 | `NotFunded` | `NOT_FD` | Invoice must be funded before it can be rated. |
| 1502 | `AlreadyRated` | `ALR_RT` | This investor has already submitted a rating for this invoice. |
| 1503 | `NotRater` | `NOT_RT` | Caller is not eligible to rate this invoice. |

---

## KYC / Verification Errors (1600 – 1604)

| Code | Variant | Symbol | Description |
|------|---------|--------|-------------|
| 1600 | `BusinessNotVerified` | `BUS_NV` | Business has not been verified; operation requires verification. |
| 1601 | `KYCAlreadyPending` | `KYC_PD` | A KYC application for this address is already pending review. |
| 1602 | `KYCAlreadyVerified` | `KYC_VF` | This address has already been KYC-verified. |
| 1603 | `KYCNotFound` | `KYC_NF` | No KYC application was found for this address. |
| 1604 | `InvalidKYCStatus` | `KYC_IS` | The supplied KYC status is not a valid transition from the current state. |

---

## Audit Errors (1700 – 1702)

| Code | Variant | Symbol | Description |
|------|---------|--------|-------------|
| 1700 | `AuditLogNotFound` | `AUD_NF` | The requested audit log entry does not exist. |
| 1701 | `AuditIntegrityError` | `AUD_IE` | Audit log integrity check failed; log may have been tampered with. |
| 1702 | `AuditQueryError` | `AUD_QE` | Audit log query failed due to invalid filter parameters. |

---

## Category / Tag Errors (1800 – 1801)

| Code | Variant | Symbol | Description |
|------|---------|--------|-------------|
| 1800 | `InvalidTag` | `INV_TAG` | Tag is empty, exceeds the maximum length, or was not found when removing. |
| 1801 | `TagLimitExceeded` | `TAG_LIM` | Adding this tag would exceed the maximum number of tags per invoice (10). |

---

## Fee Configuration Errors (1850 – 1852)

| Code | Variant | Symbol | Description |
|------|---------|--------|-------------|
| 1850 | `InvalidFeeConfiguration` | `FEE_CFG` | Fee configuration is missing required fields or contains invalid values. |
| 1851 | `TreasuryNotConfigured` | `TRS_NC` | Treasury account has not been configured for fee collection. |
| 1852 | `InvalidFeeBasisPoints` | `FEE_BPS` | Fee basis-points value is outside the allowed range (0–10 000). |

---

## Dispute Errors (1900 – 1906)

| Code | Variant | Symbol | Description |
|------|---------|--------|-------------|
| 1900 | `DisputeNotFound` | `DSP_NF` | No dispute exists for this invoice. |
| 1901 | `DisputeAlreadyExists` | `DSP_EX` | A dispute already exists for this invoice. |
| 1902 | `DisputeNotAuthorized` | `DSP_NA` | Caller is not authorized to raise or interact with this dispute. |
| 1903 | `DisputeAlreadyResolved` | `DSP_RS` | Dispute has already been resolved; no further changes are permitted. |
| 1904 | `DisputeNotUnderReview` | `DSP_UR` | Dispute must be in the `UnderReview` state to perform this action. |
| 1905 | `InvalidDisputeReason` | `DSP_RN` | Dispute reason is empty or exceeds the maximum allowed length. |
| 1906 | `InvalidDisputeEvidence` | `DSP_EV` | Dispute evidence is empty or exceeds the maximum allowed length. |

---

## Notification Errors (2000 – 2001)

| Code | Variant | Symbol | Description |
|------|---------|--------|-------------|
| 2000 | `NotificationNotFound` | `NOT_NF` | The requested notification record does not exist. |
| 2001 | `NotificationBlocked` | `NOT_BL` | Notification delivery is blocked by the recipient's preferences. |

---

## Frontend Integration

```typescript
try {
  await contract.mark_invoice_defaulted(invoiceId, gracePeriod);
} catch (error) {
  switch (error.code) {
    case 1006: /* InvoiceAlreadyDefaulted    */ break;
    case 1005: /* InvoiceNotFunded           */ break;
    case 1001: /* InvoiceNotAvailableForFunding */ break;
    case 1402: /* OperationNotAllowed        */ break;
    case 1401: /* InvalidStatus             */ break;
    default:   /* unexpected error           */
  }
}
```

---

## Best Practices

1. **Always check return values** — every contract function returns `Result<T, QuickLendXError>`.
2. **Match on the variant** (not just the numeric code) in Rust clients for forward-compatibility.
3. **Use error codes for logic** in frontend / TypeScript clients (numeric codes are stable).
4. **Log all errors** for debugging and monitoring.
5. **No panics** — the contract never panics; all errors are typed and returned.

---

## Security Notes

- Error messages do not leak internal contract state or sensitive information.
- Authorization errors (1100–1103) prevent unauthorized state transitions.
- Validation errors (1200–1204) prevent invalid data from reaching storage.
- All 50 error variants are covered by the test suite in `src/test_errors.rs`.
- The Soroban XDR spec hard-limits error enums to 50 cases; all slots are occupied.
