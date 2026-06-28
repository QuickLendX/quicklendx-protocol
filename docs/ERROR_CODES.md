# Contract Error Codes

**Audience:** Smart-contract contributors and reviewers.

Every error the QuickLendX contract can return, grouped by domain. The source of truth is the
`QuickLendXError` enum in `quicklendx-contracts/src/errors.rs` (and `FreshnessError` in
`quicklendx-contracts/src/freshness.rs`). The Soroban host surfaces these as
`ContractError(ErrorContractResult::Err(u32))` where the `u32` is the discriminant below.

## Using error codes in tests

```rust
use crate::errors::QuickLendXError;

// Match a returned error
let result = contract.try_create_invoice(&env, &inv);
assert_eq!(result, Err(Ok(QuickLendXError::InvoiceAmountInvalid)));

// Extract the numeric discriminant
assert_eq!(
    QuickLendXError::InvoiceNotFound as u32,
    1000
);
```

Off-chain integrators can inspect the `u32` value in the transaction result meta — the mapping
below is stable. **Do not renumber any variant marked "public ABI" in the source.**

---

## QuickLendXError (primary contract error enum)

### Invoice lifecycle — 1000–1007

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 1000  | `InvoiceNotFound` | `INV_NF`  | Invoice lookup failed — the supplied invoice ID does not exist. |
| 1001  | `InvoiceNotAvailableForFunding` | `INV_NAF`  | Funding was attempted while the invoice is not in a fundable state. |
| 1002  | `InvoiceAlreadyFunded` | `INV_AF`   | A funding path was entered for an invoice that already has funding. |
| 1003  | `InvoiceAmountInvalid` | `INV_AI`   | Amount violates invoice-specific constraints (e.g., bid-to-invoice bounds). |
| 1004  | `InvoiceDueDateInvalid` | `INV_DI`   | Due date is zero, in the past, or exceeds the configured horizon. |
| 1005  | `InvoiceNotFunded` | `INV_NFD`  | Settlement or post-funding action was attempted before the invoice was funded. |
| 1006  | `InvoiceAlreadyDefaulted` | `INV_AD`   | Default processing was run on an invoice already in the default lifecycle. |
| 1007  | `InvoiceFrozen` | *(missing from Symbol map)* | Invoice is frozen and cannot be acted on. Raised in `contract.rs` and `settlement.rs`. |

### Authorization — 1100–1104

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 1100  | `Unauthorized` | `UNAUTH`  | General authorization failure — caller cannot perform the action. |
| 1101  | `NotBusinessOwner` | `NOT_OWN` | The caller is not the business owner of the targeted invoice or record. |
| 1102  | `NotInvestor` | `NOT_INV` | The caller is not the required investor for the operation. |
| 1103  | `NotAdmin` | `NOT_ADM` | An admin-only entrypoint was called without admin authorization. |
| 1104  | `SelfCallNotAllowed` | `SELF_NA` | The contract called itself — confused-deputy prevention. |

### Input validation — 1200–1205

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 1200  | `InvalidAmount` | `INV_AMT` | Amount is zero, negative, below minimums, or above limits. |
| 1201  | `InvalidAddress` | `INV_ADR` | Address input is malformed or disallowed. |
| 1202  | `InvalidCurrency` | `INV_CR`  | Currency is not on the whitelist or fails validation. |
| 1203  | `InvalidTimestamp` | `INV_TM`  | Timestamp is malformed or outside the accepted window. |
| 1204  | `InvalidDescription` | `INV_DS`  | Free-form string input is empty or exceeds the byte limit. |
| 1205  | `SelfTransfer` | *(missing from Symbol map)* | The caller attempted to transfer tokens to their own address. |

### Storage — 1300–1301

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 1300  | `StorageError` | `STORE`  | Generic storage read or write failure. |
| 1301  | `StorageKeyNotFound` | `KEY_NF` | A required key was missing from contract storage. |

### Business logic — 1400–1409

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 1400  | `InsufficientFunds` | `INSUF`   | Account or escrow balance is insufficient. |
| 1401  | `InvalidStatus` | `INV_ST`   | Operation is not allowed for the current lifecycle state. |
| 1402  | `OperationNotAllowed` | `OP_NA`    | The protocol blocks the call (business rules, duplicate actions, disabled flows). |
| 1403  | `PaymentTooLow` | `PAY_LOW`  | Payment is below the minimum for the settlement path. |
| 1404  | `PlatformAccountNotConfigured` | `PLT_NC`   | Platform account setup is missing. Reserved — no current production raising site. |
| 1405  | `InvalidCoveragePercentage` | `INS_CV`   | Insurance / coverage percentage is outside the allowed range. |
| 1406  | `MaxBidsPerInvoiceExceeded` | `MAX_BIDS` | Active bid count for the invoice hit the per-invoice cap. |
| 1407  | `MaxActiveBidsPerInvestorExceeded` | `MAX_ACT`  | Active bid count for the investor hit the per-investor cap. |
| 1408  | `MaxInvoicesPerBusinessExceeded` | `MAX_INV`  | Business hit the configured active-invoice cap. |
| 1409  | `InvalidBidTtl` | `INV_TTL`  | Bid TTL is zero or outside the `1..=30` day range. |

### Rating — 1500–1503

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 1500  | `InvalidRating` | `INV_RT` | Rating value is outside the accepted range. |
| 1501  | `NotFunded` | `NOT_FD` | A funded-only action was attempted before funding existed. |
| 1502  | `AlreadyRated` | `ALR_RT` | A duplicate rating was submitted. |
| 1503  | `NotRater` | `NOT_RT` | The caller is not the authorized rating participant. Reserved — no current production raising site. |

### KYC / Verification — 1600–1605, 1660

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 1600  | `BusinessNotVerified` | `BUS_NV`  | Business KYC / verification is not complete. |
| 1601  | `KYCAlreadyPending` | `KYC_PD`  | A KYC submission is already under review. |
| 1602  | `KYCAlreadyVerified` | `KYC_VF`  | Verification was attempted after it already completed. |
| 1603  | `KYCNotFound` | `KYC_NF`  | No KYC record exists for the address. |
| 1604  | `InvalidKYCStatus` | `KYC_IS`  | A KYC transition is invalid for the current status. |
| 1605  | `InvestorNotVerified` | `INV_NV`  | Investor verification is required but not complete. |
| 1660  | `BusinessDeleted` | `BUS_DEL` | The business account has been deleted. |

### Audit — 1700–1702

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 1700  | `AuditLogNotFound` | `AUD_NF` | Requested audit log entry was not found. |
| 1701  | `AuditIntegrityError` | `AUD_IE` | Audit record integrity check failed. |
| 1702  | `AuditQueryError` | `AUD_QE` | Audit query parameters are invalid. |

### Tags — 1800–1801

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 1800  | `InvalidTag` | `INV_TAG`  | Tag is empty after normalization, too long, or duplicates another tag. |
| 1801  | `TagLimitExceeded` | `TAG_LIM` | The invoice tag vector exceeds the maximum count. |

### Fees and treasury — 1850–1857

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 1850  | `InvalidFeeConfiguration` | `FEE_CFG`  | Fee initialization or configuration is inconsistent or duplicated. |
| 1851  | `TreasuryNotConfigured` | `TRS_NC`   | Treasury-dependent fee distribution was attempted before setup. |
| 1852  | `InvalidFeeBasisPoints` | `FEE_BPS`  | Fee basis points are negative or above the maximum. |
| 1853  | `RotationAlreadyPending` | `ROT_PND`  | A treasury rotation was requested while one is already pending. |
| 1854  | `RotationNotFound` | `ROT_NF`   | The requested treasury rotation state was not found. |
| 1855  | `RotationExpired` | `ROT_EXP`  | The treasury rotation confirmation window expired. |
| 1856  | `ArithmeticOverflow` | `ARITH_OF` | A fee or profit calculation overflowed. |
| 1857  | `RotationTimelockNotElapsed` | `ROT_TLK`  | The rotation timelock is still active. |

### Disputes — 1900–1906

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 1900  | `DisputeNotFound` | `DSP_NF` | The requested dispute record does not exist. |
| 1901  | `DisputeAlreadyExists` | `DSP_EX` | A dispute was opened for an invoice that already has one. |
| 1902  | `DisputeNotAuthorized` | `DSP_NA` | The caller is not authorized to create or resolve the dispute. |
| 1903  | `DisputeAlreadyResolved` | `DSP_RS` | Resolution was attempted for an already-finalized dispute. |
| 1904  | `DisputeNotUnderReview` | `DSP_UR` | A dispute transition requires the dispute to be under review. |
| 1905  | `InvalidDisputeReason` | `DSP_RN` | The dispute reason is empty or exceeds the configured limit. |
| 1906  | `InvalidDisputeEvidence` | `DSP_EV` | The dispute evidence payload is empty or exceeds the limit. |

### Notifications — 2000–2002

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 2000  | `NotificationNotFound` | `NOT_NF`  | The requested notification record was not found. |
| 2001  | `NotificationBlocked` | `NOT_BL`  | Delivery is blocked by user or system settings. |
| 2002  | `NotificationDuplicate` | `NOT_DUP` | A duplicate notification was detected. |

### Emergency and pause — 2100–2106

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 2100  | `ContractPaused` | `PAUSED` | A write-path operation was attempted while the contract is paused. |
| 2101  | `EmergencyWithdrawNotFound` | `EMG_NF`   | No pending emergency withdrawal exists. |
| 2102  | `EmergencyWithdrawTimelockNotElapsed` | `EMG_TLK`  | Execution was attempted before the timelock elapsed. |
| 2103  | `EmergencyWithdrawExpired` | `EMG_EXP`  | The withdrawal expired before execution. |
| 2104  | `EmergencyWithdrawCancelled` | `EMG_CNL`  | The withdrawal was already cancelled. |
| 2105  | `EmergencyWithdrawAlreadyExists` | `EMG_EX`   | A new withdrawal was requested while one already exists. |
| 2106  | `EmergencyWithdrawInsufficientBalance` | `EMG_BAL`  | The contract balance is insufficient for the requested withdrawal. |

### Transfers, maintenance, defaults, backup — 2200–2204

| Code  | Variant | ABI symbol | Meaning |
|-------|---------|-----------|---------|
| 2200  | `TokenTransferFailed` | `TKN_FAIL` | The underlying token contract transfer or transfer-from call failed. |
| 2201  | `MaintenanceModeActive` | `MAINT`    | A mutating operation was attempted during maintenance mode. |
| 2202  | `DuplicateDefaultTransition` | `DEF_DUP`  | A default transition was run more than once for the same invoice. |
| 2203  | `BackupVersionUnsupported` | `BKP_VER`  | The backup data version is not supported by the current contract. |
| 2204  | `DuplicateBid` | *(missing from Symbol map)* | A duplicate bid was detected (idempotency check). |

---

## FreshnessError (data freshness sub-error)

Defined in `quicklendx-contracts/src/freshness.rs`.

| Code | Variant | Meaning |
|------|---------|---------|
| 1    | `NotAuthorized` | The caller lacks authorization for the freshness operation. |
| 2    | `StaleDataRejected` | Off-chain data exceeds the configured freshness drift threshold. |
| 3    | `InvalidConfigValue` | The freshness configuration value is invalid. |

---

## Code example: matching errors in Rust

```rust
use crate::errors::QuickLendXError;

/// Returns the user-facing message for common error codes.
pub fn describe_error_code(code: u32) -> &'static str {
    match code {
        1000 => "Invoice not found",
        1100 => "Caller not authorized",
        1401 => "Operation not allowed in current state",
        2100 => "Contract is paused",
        _ => "Unknown error",
    }
}
```

## Maintaining this document

This document is generated by auditing `src/errors.rs` and `src/freshness.rs`. When adding a new
variant:

1. Add it to the appropriate domain group in `QuickLendXError`.
2. Add the corresponding `symbol_short!` entry in the `From<QuickLendXError> for Symbol` impl.
3. Update the table above.
4. Add the variant to the stability snapshot if `test_error_code_stability` covers it.
5. **Never renumber an existing variant** — numeric values are public ABI.
