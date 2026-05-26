# Contract Errors and Input Validation

This document defines the typed error surface for `QuickLendXError`, the validation rules enforced by public entrypoints, and the protocol limits used to reject malformed input early.

## Error catalog

| Code | Name | When raised |
| --- | --- | --- |
| 1000 | `InvoiceNotFound` | Invoice lookup failed for the supplied invoice ID. |
| 1001 | `InvoiceNotAvailableForFunding` | Funding or acceptance was attempted while the invoice was not in a fundable state. |
| 1002 | `InvoiceAlreadyFunded` | A funding path was attempted for an invoice that is already funded. |
| 1003 | `InvoiceAmountInvalid` | Invoice amount violates invoice-specific constraints such as bid-to-invoice bounds. |
| 1004 | `InvoiceDueDateInvalid` | Due date is zero, in the past, or exceeds the configured future horizon. |
| 1005 | `InvoiceNotFunded` | Settlement, payment, or post-funding action was attempted before funding. |
| 1006 | `InvoiceAlreadyDefaulted` | Default processing was attempted after the invoice already defaulted. |
| 1100 | `Unauthorized` | Caller attempted an action they are not permitted to perform, such as bidding on their own invoice. |
| 1101 | `NotBusinessOwner` | Caller is not the business owner associated with the targeted invoice or record. |
| 1102 | `NotInvestor` | Caller is not recognized as the required investor for the operation. |
| 1103 | `NotAdmin` | Admin-only entrypoint was called without a configured or authorized admin. |
| 1200 | `InvalidAmount` | Amount is zero, negative, below protocol minimums, above investor limits, or otherwise out of bounds. |
| 1201 | `InvalidAddress` | Address input is malformed or disallowed for the target action. |
| 1202 | `InvalidCurrency` | Currency is not permitted by the whitelist or fails currency validation. |
| 1203 | `InvalidTimestamp` | Timestamp-like configuration value is malformed or outside allowed bounds. |
| 1204 | `InvalidDescription` | Free-form string input is empty when required or exceeds the allowed byte length. |
| 1300 | `StorageError` | Generic storage read/write failure was encountered. |
| 1301 | `StorageKeyNotFound` | Required persisted state was missing from contract storage. |
| 1400 | `InsufficientFunds` | Account or escrow balance is insufficient for the requested transfer. |
| 1401 | `InvalidStatus` | Operation is not allowed for the current object lifecycle state. |
| 1402 | `OperationNotAllowed` | Call is rejected by business rules such as duplicate actions or disabled flows. |
| 1403 | `PaymentTooLow` | Payment amount is below the minimum needed for the requested settlement path. |
| 1404 | `PlatformAccountNotConfigured` | Platform account setup required for the action is missing. |
| 1405 | `InvalidCoveragePercentage` | Insurance or coverage percentage is outside the allowed range. |
| 1406 | `MaxBidsPerInvoiceExceeded` | Active bid count for an invoice exceeds the bounded per-invoice cap. |
| 1407 | `MaxInvoicesPerBusinessExceeded` | Business exceeded the configured active-invoice cap. |
| 1408 | `InvalidBidTtl` | Bid TTL is zero or outside the allowed `1..=30` day range. |
| 1500 | `InvalidRating` | Rating value or rating payload is outside the accepted range. |
| 1501 | `NotFunded` | Rating or other funded-only action was attempted before funding existed. |
| 1502 | `AlreadyRated` | Duplicate rating was attempted after a prior rating was recorded. |
| 1503 | `NotRater` | Caller is not the authorized rating participant. |
| 1600 | `BusinessNotVerified` | Business or investor KYC/verification requirement is not satisfied. |
| 1601 | `KYCAlreadyPending` | Duplicate KYC submission was attempted while review is already pending. |
| 1602 | `KYCAlreadyVerified` | Verification submission or approval was attempted after verification already completed. |
| 1603 | `KYCNotFound` | Verification record does not exist for the supplied address. |
| 1604 | `InvalidKYCStatus` | KYC transition is invalid for the current status. |
| 1700 | `AuditLogNotFound` | Requested audit log entry was not found. |
| 1701 | `AuditIntegrityError` | Audit record integrity validation failed. |
| 1702 | `AuditQueryError` | Audit query parameters were invalid or could not be processed. |
| 1800 | `InvalidTag` | Tag is empty after normalization, too long, or duplicates another normalized tag. |
| 1801 | `TagLimitExceeded` | Invoice tag vector exceeds the maximum allowed number of tags. |
| 1850 | `InvalidFeeConfiguration` | Fee system initialization or configuration is inconsistent or duplicated. |
| 1851 | `TreasuryNotConfigured` | Treasury-dependent fee distribution was attempted before treasury setup. |
| 1852 | `InvalidFeeBasisPoints` | Fee basis points are negative or above the maximum allowed value. |
| 1853 | `RotationAlreadyPending` | Treasury rotation was requested while another rotation is already pending. |
| 1854 | `RotationNotFound` | Requested treasury rotation state was not found. |
| 1855 | `RotationExpired` | Treasury rotation confirmation window expired. |
| 1900 | `DisputeNotFound` | Requested dispute record does not exist. |
| 1901 | `DisputeAlreadyExists` | Duplicate dispute was opened for the same invoice or context. |
| 1902 | `DisputeNotAuthorized` | Caller is not authorized to create or resolve the dispute. |
| 1903 | `DisputeAlreadyResolved` | Resolution was attempted for a dispute already finalized. |
| 1904 | `DisputeNotUnderReview` | Dispute transition requires the dispute to be under review first. |
| 1905 | `InvalidDisputeReason` | Dispute reason is empty or exceeds the configured limit. |
| 1906 | `InvalidDisputeEvidence` | Dispute evidence payload is empty or exceeds the configured limit. |
| 2000 | `NotificationNotFound` | Requested notification record does not exist. |
| 2001 | `NotificationBlocked` | Notification delivery is blocked by current user or system settings. |
| 2100 | `ContractPaused` | Write path was attempted while the contract is paused. |
| 2101 | `EmergencyWithdrawNotFound` | No pending emergency withdrawal exists to inspect or execute. |
| 2102 | `EmergencyWithdrawTimelockNotElapsed` | Emergency withdrawal execution was attempted before timelock expiry. |
| 2103 | `EmergencyWithdrawExpired` | Emergency withdrawal expired before execution. |
| 2104 | `EmergencyWithdrawCancelled` | Emergency withdrawal has already been cancelled and cannot proceed. |
| 2105 | `EmergencyWithdrawAlreadyExists` | A new emergency withdrawal was requested while one already exists. |
| 2106 | `EmergencyWithdrawInsufficientBalance` | Contract balance is insufficient for the requested emergency withdrawal. |
| 2200 | `TokenTransferFailed` | Underlying token contract transfer or transfer-from failed. |
| 2201 | `MaintenanceModeActive` | Mutating operation was attempted while maintenance mode is enabled. |
| 2202 | `DuplicateDefaultTransition` | Default transition was attempted more than once for the same invoice. |

## Input validation strategy

The contract follows four validation rules across public entrypoints:

1. Reject invalid input before storage writes or token movement.
2. Return typed `QuickLendXError` values instead of panicking on expected bad input.
3. Normalize user-controlled strings before validating canonical limits where needed, especially tags.
4. Keep vectors and strings bounded so execution cost and storage growth remain predictable.

The shared invalid-input matrix in `src/test_input_matrix.rs` exercises representative public entrypoints with malformed values and asserts that they return contract errors rather than host-level failures.

## String length limits

These limits are defined in `src/protocol_limits.rs`.

| Constant | Max bytes | Used for |
| --- | ---: | --- |
| `MAX_DESCRIPTION_LENGTH` | 1024 | Invoice descriptions and line-item descriptions |
| `MAX_NAME_LENGTH` | 150 | Customer and entity name fields |
| `MAX_ADDRESS_LENGTH` | 300 | Customer address fields |
| `MAX_TAX_ID_LENGTH` | 50 | Tax identifiers |
| `MAX_NOTES_LENGTH` | 2000 | Invoice notes |
| `MAX_TAG_LENGTH` | 50 | Individual normalized invoice tags |
| `MAX_TRANSACTION_ID_LENGTH` | 124 | Payment and transaction identifiers |
| `MAX_DISPUTE_REASON_LENGTH` | 1000 | Dispute reason text |
| `MAX_DISPUTE_EVIDENCE_LENGTH` | 2000 | Dispute evidence text |
| `MAX_DISPUTE_RESOLUTION_LENGTH` | 2000 | Dispute resolution text |
| `MAX_NOTIFICATION_TITLE_LENGTH` | 150 | Notification titles |
| `MAX_NOTIFICATION_MESSAGE_LENGTH` | 1000 | Notification messages |
| `MAX_KYC_DATA_LENGTH` | 5000 | Business and investor KYC payloads |
| `MAX_REJECTION_REASON_LENGTH` | 500 | Admin rejection reasons |
| `MAX_FEEDBACK_LENGTH` | 1000 | Ratings and feedback text |

## Amount and timestamp validation rules

### Amount rules

- Invoice amounts must be strictly positive.
- Bid amounts must be strictly positive.
- Bid amounts must satisfy both the configured absolute minimum and percentage-based minimum.
- Bid amounts cannot exceed the target invoice amount.
- Investor verification and fee configuration entrypoints reject out-of-range numeric inputs with typed errors.
- Platform fee basis points must stay within the configured upper bound of `1000` basis points.

### Timestamp rules

- Invoice due dates must be strictly greater than the current ledger timestamp.
- Invoice due dates must stay within the configured due-date horizon (`max_due_date_days`).
- Bid TTL must stay within `1..=30` days.
- Protocol grace-period and related config timestamps are validated before committing state.

## Security note: DoS prevention

Input validation is part of the contract's denial-of-service defense:

- Oversized strings are rejected before storage writes.
- Oversized vectors are rejected before iteration-heavy paths grow without bound.
- Tag normalization prevents duplicate or empty canonical tags from consuming storage slots.
- Amount and timestamp bounds prevent pathological values from pushing later calculations into invalid or expensive states.
- The result is deterministic rejection with typed errors, which keeps contract behavior auditable and avoids panic-driven aborts on malformed user input.
