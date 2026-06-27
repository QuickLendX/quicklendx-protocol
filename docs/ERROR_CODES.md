# QuickLendX Error Codes

This document is the client-facing catalog for QuickLendX contract error codes.
Use it when decoding Soroban `Error(Contract, code)` values in frontends,
indexers, SDKs, and support tooling.

## Sources Of Truth

| Contract surface | Source file | Error enum |
| --- | --- | --- |
| Full protocol contract | `quicklendx-contracts/src/errors.rs` | `QuickLendXError` |
| Root compatibility contract | `src/errors.rs` | `ContractError` |

The numeric values are part of the public ABI because both enums use
`#[repr(u32)]` with `#[contracterror]`. Do not renumber an existing variant.
If a variant is renamed or replaced, update this document in the same PR as the
code change.

## Full Protocol Contract: `QuickLendXError`

| Code | Variant | Symbol | Meaning |
| ---: | --- | --- | --- |
| 1000 | `InvoiceNotFound` | `INV_NF` | Invoice lookup failed for the supplied invoice ID. |
| 1001 | `InvoiceNotAvailableForFunding` | `INV_NAF` | Funding or bid acceptance was attempted while the invoice is not fundable. |
| 1002 | `InvoiceAlreadyFunded` | `INV_AF` | A funding path was attempted for an invoice that is already funded. |
| 1003 | `InvoiceAmountInvalid` | `INV_AI` | Invoice amount violates invoice-specific constraints. |
| 1004 | `InvoiceDueDateInvalid` | `INV_DI` | Due date is zero, in the past, or outside the configured horizon. |
| 1005 | `InvoiceNotFunded` | `INV_NFD` | Settlement, payment, or post-funding action requires a funded invoice. |
| 1006 | `InvoiceAlreadyDefaulted` | `INV_AD` | Default processing was attempted after the invoice already defaulted. |
| 1100 | `Unauthorized` | `UNAUTH` | Caller is not authorized for the requested action. |
| 1101 | `NotBusinessOwner` | `NOT_OWN` | Caller is not the business owner for the targeted record. |
| 1102 | `NotInvestor` | `NOT_INV` | Caller is not the investor required by the operation. |
| 1103 | `NotAdmin` | `NOT_ADM` | Admin-only entrypoint was called by a non-admin. |
| 1104 | `SelfCallNotAllowed` | `SELF_NA` | Caller address is the contract address itself. |
| 1200 | `InvalidAmount` | `INV_AMT` | Amount is zero, negative, below minimums, above limits, or otherwise out of bounds. |
| 1201 | `InvalidAddress` | `INV_ADR` | Address input is malformed or disallowed for the action. |
| 1202 | `InvalidCurrency` | `INV_CR` | Currency token is not permitted by whitelist validation. |
| 1203 | `InvalidTimestamp` | `INV_TM` | Timestamp-like input or configuration value is outside allowed bounds. |
| 1204 | `InvalidDescription` | `INV_DS` | Free-form text input is empty when required or exceeds length limits. |
| 1300 | `StorageError` | `STORE` | Generic storage read/write failure. |
| 1301 | `StorageKeyNotFound` | `KEY_NF` | Required persisted state is missing from contract storage. |
| 1400 | `InsufficientFunds` | `INSUF` | Account, escrow, or contract balance is insufficient for the transfer. |
| 1401 | `InvalidStatus` | `INV_ST` | Operation is not valid for the current lifecycle status. |
| 1402 | `OperationNotAllowed` | `OP_NA` | Business rule blocks the requested operation. |
| 1403 | `PaymentTooLow` | `PAY_LOW` | Payment amount is below the required minimum. |
| 1404 | `PlatformAccountNotConfigured` | `PLT_NC` | Platform fee account is required but not configured. |
| 1405 | `InvalidCoveragePercentage` | `INS_CV` | Insurance or coverage percentage is outside the allowed range. |
| 1406 | `MaxBidsPerInvoiceExceeded` | `MAX_BIDS` | Per-invoice active bid cap would be exceeded. |
| 1407 | `MaxActiveBidsPerInvestorExceeded` | `MAX_ACT` | Per-investor active bid cap would be exceeded. |
| 1408 | `MaxInvoicesPerBusinessExceeded` | `MAX_INV` | Business active-invoice cap would be exceeded. |
| 1409 | `InvalidBidTtl` | `INV_TTL` | Bid time-to-live is zero or outside the allowed range. |
| 1500 | `InvalidRating` | `INV_RT` | Rating value or payload is outside the accepted range. |
| 1501 | `NotFunded` | `NOT_FD` | Funded-only action was attempted before funding exists. |
| 1502 | `AlreadyRated` | `ALR_RT` | Duplicate rating was attempted after a prior rating was recorded. |
| 1503 | `NotRater` | `NOT_RT` | Caller is not the participant allowed to rate. |
| 1600 | `BusinessNotVerified` | `BUS_NV` | Business verification requirement is not satisfied. |
| 1601 | `KYCAlreadyPending` | `KYC_PD` | Duplicate KYC submission was attempted while review is pending. |
| 1602 | `KYCAlreadyVerified` | `KYC_VF` | KYC submission or approval was attempted after verification completed. |
| 1603 | `KYCNotFound` | `KYC_NF` | Verification record does not exist for the supplied address. |
| 1604 | `InvalidKYCStatus` | `KYC_IS` | KYC transition is invalid for the current status. |
| 1605 | `InvestorNotVerified` | `INV_NV` | Investor verification requirement is not satisfied. |
| 1660 | `BusinessDeleted` | `BUS_DEL` | Business record is deleted or no longer usable. |
| 1700 | `AuditLogNotFound` | `AUD_NF` | Requested audit log entry was not found. |
| 1701 | `AuditIntegrityError` | `AUD_IE` | Audit record integrity validation failed. |
| 1702 | `AuditQueryError` | `AUD_QE` | Audit query parameters are invalid or cannot be processed. |
| 1800 | `InvalidTag` | `INV_TAG` | Tag is empty after normalization, too long, invalid, or duplicated. |
| 1801 | `TagLimitExceeded` | `TAG_LIM` | Tag vector would exceed the maximum allowed number of tags. |
| 1850 | `InvalidFeeConfiguration` | `FEE_CFG` | Fee configuration is missing required values or is inconsistent. |
| 1851 | `TreasuryNotConfigured` | `TRS_NC` | Treasury-dependent operation was attempted before treasury setup. |
| 1852 | `InvalidFeeBasisPoints` | `FEE_BPS` | Fee basis points are negative or above the configured maximum. |
| 1853 | `RotationAlreadyPending` | `ROT_PND` | Treasury rotation was requested while another rotation is pending. |
| 1854 | `RotationNotFound` | `ROT_NF` | Requested treasury rotation state was not found. |
| 1855 | `RotationExpired` | `ROT_EXP` | Treasury rotation confirmation window expired. |
| 1856 | `ArithmeticOverflow` | `ARITH_OF` | Checked arithmetic detected overflow or underflow. |
| 1857 | `RotationTimelockNotElapsed` | `ROT_TLK` | Treasury rotation confirmation was attempted before timelock expiry. |
| 1900 | `DisputeNotFound` | `DSP_NF` | Requested dispute record does not exist. |
| 1901 | `DisputeAlreadyExists` | `DSP_EX` | Duplicate dispute was opened for the same invoice or context. |
| 1902 | `DisputeNotAuthorized` | `DSP_NA` | Caller is not authorized to create or resolve the dispute. |
| 1903 | `DisputeAlreadyResolved` | `DSP_RS` | Resolution was attempted for a dispute already finalized. |
| 1904 | `DisputeNotUnderReview` | `DSP_UR` | Dispute transition requires the dispute to be under review. |
| 1905 | `InvalidDisputeReason` | `DSP_RN` | Dispute reason is empty or exceeds the configured limit. |
| 1906 | `InvalidDisputeEvidence` | `DSP_EV` | Dispute evidence payload is empty or exceeds the configured limit. |
| 2000 | `NotificationNotFound` | `NOT_NF` | Requested notification record does not exist. |
| 2001 | `NotificationBlocked` | `NOT_BL` | Notification delivery is blocked by user or system settings. |
| 2002 | `NotificationDuplicate` | `NOT_DUP` | Duplicate notification was detected. |
| 2100 | `ContractPaused` | `PAUSED` | Mutating path was attempted while the contract is paused. |
| 2101 | `EmergencyWithdrawNotFound` | `EMG_NF` | No pending emergency withdrawal exists to inspect or execute. |
| 2102 | `EmergencyWithdrawTimelockNotElapsed` | `EMG_TLK` | Emergency withdrawal execution was attempted before timelock expiry. |
| 2103 | `EmergencyWithdrawExpired` | `EMG_EXP` | Emergency withdrawal expired before execution. |
| 2104 | `EmergencyWithdrawCancelled` | `EMG_CNL` | Emergency withdrawal has already been cancelled. |
| 2105 | `EmergencyWithdrawAlreadyExists` | `EMG_EX` | New emergency withdrawal was requested while one already exists. |
| 2106 | `EmergencyWithdrawInsufficientBalance` | `EMG_BAL` | Emergency withdrawal exceeds available balance after reserved funds. |
| 2200 | `TokenTransferFailed` | `TKN_FAIL` | Underlying token contract transfer or transfer-from failed. |
| 2201 | `MaintenanceModeActive` | `MAINT` | Mutating operation was attempted while maintenance mode is enabled. |
| 2202 | `DuplicateDefaultTransition` | `DEF_DUP` | Default transition was attempted more than once for the same invoice. |
| 2203 | `BackupVersionUnsupported` | `BKP_VER` | Backup restore or decode was attempted with an unsupported version. |
| 2204 | `DuplicateBid` | Not mapped | Duplicate bid submission was detected. |

`Symbol` values come from the `impl From<QuickLendXError> for Symbol` mapping
in `quicklendx-contracts/src/errors.rs`. `DuplicateBid` currently has a numeric
contract code but no short-symbol mapping; clients should match code `2204` or
the Rust variant name until a symbol is added.

## Root Compatibility Contract: `ContractError`

| Code | Variant | Meaning |
| ---: | --- | --- |
| 1 | `NotInitialized` | Contract has not been initialized. |
| 2 | `AlreadyInitialized` | Contract has already been initialized. |
| 3 | `NotAdmin` | Caller is not the admin. |
| 4 | `OperationNotAllowed` | Operation is not allowed in the current state. |
| 5 | `InvalidAmount` | Amount input is invalid, such as zero or negative. |
| 6 | `InvalidFee` | Fee input is invalid. |
| 7 | `InvalidParameter` | Generic parameter is outside the accepted range. |

## Client Handling Guidance

Rust callers should prefer matching enum variants. Frontend, indexer, and
support tooling should match numeric codes because Soroban host errors expose the
contract error as a stable integer.

```ts
switch (error.code) {
  case 1000:
    return "Invoice was not found";
  case 1402:
    return "Operation is not allowed in the current state";
  case 2204:
    return "Duplicate bid";
  default:
    return "Unexpected QuickLendX contract error";
}
```

When adding or replacing an error variant:

1. Update the Rust enum and symbol mapping together.
2. Keep numeric values stable for existing variants.
3. Update this file, `docs/contracts/errors.md`, and any SDK mappings in the
   same PR.
