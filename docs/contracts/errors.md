# Error Handling Documentation

## Overview

The QuickLendX contract uses a comprehensive error enum (`QuickLendXError`) to provide clear, typed error responses for all contract operations. All errors are properly typed and never result in panics, ensuring secure and predictable behavior.

## Error Code Mapping

### Invoice Errors (1000-1099)

| Error Code | Enum Variant | Symbol | Description |
|------------|--------------|--------|-------------|
| 1000 | `InvoiceNotFound` | `INV_NF` | The specified invoice ID does not exist |
| 1001 | `InvoiceAlreadyExists` | `INV_EX` | An invoice with this ID already exists |
| 1002 | `InvoiceNotAvailableForFunding` | `INV_NA` | Invoice is not available for funding (wrong status) |
| 1003 | `InvoiceAlreadyFunded` | `INV_FD` | Invoice has already been funded |
| 1004 | `InvoiceAmountInvalid` | `INV_AI` | Invoice amount is invalid (zero or negative) |
| 1005 | `InvoiceDueDateInvalid` | `INV_DI` | Invoice due date is in the past or invalid |
| 1006 | `InvoiceNotVerified` | `INV_NV` | Invoice has not been verified yet |
| 1007 | `InvoiceNotFunded` | `INV_NF` | Invoice has not been funded |
| 1008 | `InvoiceAlreadyPaid` | `INV_PD` | Invoice has already been paid |
| 1009 | `InvoiceAlreadyDefaulted` | `INV_DF` | Invoice has already been marked as defaulted |

### Authorization Errors (1100-1199)

| Error Code | Enum Variant | Symbol | Description |
|------------|--------------|--------|-------------|
| 1100 | `Unauthorized` | `UNAUTH` | Caller is not authorized for this operation |
| 1101 | `NotBusinessOwner` | `NOT_OWN` | Caller is not the business owner |
| 1102 | `NotInvestor` | `NOT_INV` | Caller is not an investor |
| 1103 | `NotAdmin` | `NOT_ADM` | Caller is not an admin |

### Validation Errors (1200-1299)

| Error Code | Enum Variant | Symbol | Description |
|------------|--------------|--------|-------------|
| 1200 | `InvalidAmount` | `INV_AMT` | Amount is invalid (zero, negative, or exceeds limit) |
| 1201 | `InvalidAddress` | `INV_ADR` | Address is invalid |
| 1202 | `InvalidCurrency` | `INV_CR` | Currency is invalid |
| 1203 | `InvalidTimestamp` | `INV_TM` | Timestamp is invalid |
| 1204 | `InvalidDescription` | `INV_DS` | Description is empty or invalid |

### Storage Errors (1300-1399)

| Error Code | Enum Variant | Symbol | Description |
|------------|--------------|--------|-------------|
| 1300 | `StorageError` | `STORE` | General storage error |
| 1301 | `StorageKeyNotFound` | `KEY_NF` | Storage key not found |

### Business Logic Errors (1400-1499)

| Error Code | Enum Variant | Symbol | Description |
|------------|--------------|--------|-------------|
| 1400 | `InsufficientFunds` | `INSUF` | Insufficient funds for operation |
| 1401 | `InvalidStatus` | `INV_ST` | Invalid invoice or operation status |
| 1402 | `OperationNotAllowed` | `OP_NA` | Operation is not allowed in current state |
| 1403 | `PaymentTooLow` | `PAY_LOW` | Payment amount is too low |
| 1404 | `PlatformAccountNotConfigured` | `PLT_NC` | Platform account is not configured |
| 1405 | `InvalidCoveragePercentage` | `INS_CV` | Insurance coverage percentage is invalid |

### Rating Errors (1500-1599)

| Error Code | Enum Variant | Symbol | Description |
|------------|--------------|--------|-------------|
| 1500 | `InvalidRating` | `INV_RT` | Rating value is invalid (must be 1-5) |
| 1501 | `NotFunded` | `NOT_FD` | Invoice must be funded before rating |
| 1502 | `AlreadyRated` | `ALR_RT` | Invoice has already been rated by this user |
| 1503 | `NotRater` | `NOT_RT` | Caller is not authorized to rate this invoice |

### KYC/Verification Errors (1600-1699)

| Error Code | Enum Variant | Symbol | Description |
|------------|--------------|--------|-------------|
| 1600 | `BusinessNotVerified` | `BUS_NV` | Business is not verified |
| 1601 | `KYCAlreadyPending` | `KYC_PD` | KYC application is already pending |
| 1602 | `KYCAlreadyVerified` | `KYC_VF` | KYC application is already verified |
| 1603 | `KYCNotFound` | `KYC_NF` | KYC application not found |
| 1604 | `InvalidKYCStatus` | `KYC_IS` | Invalid KYC status |

### Audit Errors (1700-1799)

| Error Code | Enum Variant | Symbol | Description |
|------------|--------------|--------|-------------|
| 1700 | `AuditLogNotFound` | `AUD_NF` | Audit log entry not found |
| 1701 | `AuditIntegrityError` | `AUD_IE` | Audit log integrity check failed |
| 1702 | `AuditQueryError` | `AUD_QE` | Audit query error |

### Category and Tag Errors (1800-1899)

| Error Code | Enum Variant | Symbol | Description |
|------------|--------------|--------|-------------|
| 1802 | `InvalidTag` | `INV_TAG` | Tag is invalid (empty, too long, or not found) |
| 1803 | `TagLimitExceeded` | `TAG_LIM` | Maximum number of tags (10) exceeded |

### Dispute Errors (1900-1999)

| Error Code | Enum Variant | Symbol | Description |
|------------|--------------|--------|-------------|
| 1900 | `DisputeNotFound` | `DSP_NF` | Dispute not found |
| 1901 | `DisputeAlreadyExists` | `DSP_EX` | Dispute already exists for this invoice |
| 1902 | `DisputeNotAuthorized` | `DSP_NA` | Caller is not authorized to create dispute |
| 1903 | `DisputeAlreadyResolved` | `DSP_RS` | Dispute has already been resolved |
| 1904 | `DisputeNotUnderReview` | `DSP_UR` | Dispute is not under review |
| 1905 | `InvalidDisputeReason` | `DSP_RN` | Dispute reason is invalid (empty or too long) |
| 1906 | `InvalidDisputeEvidence` | `DSP_EV` | Dispute evidence is invalid (empty or too long) |

### Notification Errors (2000-2099)

| Error Code | Enum Variant | Symbol | Description |
|------------|--------------|--------|-------------|
| 2000 | `NotificationNotFound` | `NOT_NF` | Notification not found |
| 2001 | `NotificationBlocked` | `NOT_BL` | Notification is blocked by user preferences |

## Frontend Integration

### Error Handling Pattern

When calling contract functions, always check for errors:

```typescript
try {
  const result = await contract.mark_invoice_defaulted(invoiceId, gracePeriod);
  // Handle success
} catch (error) {
  if (error.code === 1009) {
    // InvoiceAlreadyDefaulted
    console.error("Invoice is already defaulted");
  } else if (error.code === 1007) {
    // InvoiceNotFunded
    console.error("Invoice must be funded before defaulting");
  } else if (error.code === 1402) {
    // OperationNotAllowed
    console.error("Grace period has not expired yet");
  }
  // Handle other errors...
}
```

### Error Code Ranges

- **1000-1099**: Invoice-related errors
- **1100-1199**: Authorization errors
- **1200-1299**: Validation errors
- **1300-1399**: Storage errors
- **1400-1499**: Business logic errors
- **1500-1599**: Rating errors
- **1600-1699**: KYC/Verification errors
- **1700-1799**: Audit errors
- **1800-1899**: Category/Tag errors
- **1900-1999**: Dispute errors
- **2000-2099**: Notification errors

## Best Practices

1. **Always check return values**: All contract functions return `Result<T, QuickLendXError>`
2. **Handle errors gracefully**: Never ignore errors; provide user-friendly messages
3. **Use error codes for logic**: Error codes can be used for conditional logic in frontend
4. **Log errors**: Log all errors for debugging and monitoring
5. **No panics**: The contract never panics; all errors are typed and returned

## Security Notes

- All errors are typed and cannot be exploited
- Error messages do not leak sensitive information
- Authorization errors prevent unauthorized access
- Validation errors prevent invalid state transitions
- All error conditions are tested (see `test_errors.rs`)

