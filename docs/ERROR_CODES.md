# Contract Error Codes

**Audience:** Smart-contract contributors and reviewers.

Every `QuickLendXError` variant and its explicit numeric discriminant, ABI symbol, and Rust documentation are listed below. The source of truth is `quicklendx-contracts/src/errors.rs`. Soroban surfaces these as `ContractError(ErrorContractResult::Err(u32))`; the `u32` is the discriminant.

## Using error codes in tests

```rust
use crate::errors::QuickLendXError;

// Match a returned error
let result = contract.try_create_invoice(&env, &inv);
assert_eq!(result, Err(Ok(QuickLendXError::InvoiceAmountInvalid)));

// Extract the numeric discriminant
assert_eq!(QuickLendXError::InvoiceNotFound as u32, 1000);
```

Off-chain integrators can inspect the `u32` value in transaction result metadata. Numeric values are public ABI; **do not renumber existing variants**.

## QuickLendXError

The table is generated from the enum in numeric order. Variants whose source order differs from their numeric order are still included at their declared code.

| Code | Variant | ABI symbol | Rust documentation |
| ---: | --- | --- | --- |
| 1000 | `InvoiceNotFound` | `INV_NF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1001 | `InvoiceNotAvailableForFunding` | `INV_NAF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1002 | `InvoiceAlreadyFunded` | `INV_AF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1003 | `InvoiceAmountInvalid` | `INV_AI` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1004 | `InvoiceDueDateInvalid` | `INV_DI` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1005 | `InvoiceNotFunded` | `INV_NFD` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1006 | `InvoiceAlreadyDefaulted` | `INV_AD` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1007 | `InvoiceFrozen` | `INV_FRZ` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1008 | `InvalidFreezeReason` | `FRZ_RSN` | Caller is a registered admin but is **not** a registered dispute<br>arbiter. Resolving, reviewing, or driving dispute lifecycle actions<br>requires explicit arbiter registration on top of admin authority —<br>this separates "who can configure the protocol" from "who can<br>adjudicate a dispute".<br>BREAKING: Do not renumber this variant. public ABI consumption. |
| 1009 | `InvoiceLockExpired` | `LK_EXP` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1010 | `NotArbiter` | `NOT_ARB` |  |
| 1100 | `Unauthorized` | `UNAUTH` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1101 | `NotBusinessOwner` | `NOT_OWN` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1102 | `NotInvestor` | `NOT_INV` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1103 | `NotAdmin` | `NOT_ADM` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1104 | `SelfCallNotAllowed` | `SELF_NA` | Caller address equals the contract's own address (confused-deputy prevention).<br>BREAKING: Do not renumber this variant. public ABI consumption. |
| 1200 | `InvalidAmount` | `INV_AMT` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1201 | `InvalidAddress` | `INV_ADR` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1202 | `InvalidCurrency` | `INV_CR` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1203 | `InvalidTimestamp` | `INV_TM` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1204 | `InvalidDescription` | `INV_DS` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1205 | `SelfTransfer` | `SLF_XFR` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1206 | `InvalidCursor` | `INV_CUR` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1300 | `StorageError` | `STORE` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1301 | `StorageKeyNotFound` | `KEY_NF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1400 | `InsufficientFunds` | `INSUF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1401 | `InvalidStatus` | `INV_ST` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1402 | `OperationNotAllowed` | `OP_NA` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1403 | `PaymentTooLow` | `PAY_LOW` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1404 | `PlatformAccountNotConfigured` | `PLT_NC` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1405 | `InvalidCoveragePercentage` | `INS_CV` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1406 | `MaxBidsPerInvoiceExceeded` | `MAX_BIDS` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1407 | `MaxActiveBidsPerInvestorExceeded` | `MAX_ACT` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1408 | `MaxInvoicesPerBusinessExceeded` | `MAX_INV` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1409 | `InvalidBidTtl` | `INV_TTL` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1410 | `InsuranceClaimWindowClosed` | `INS_WIN` | Insurance opt-in was rejected because the invoice due date has already<br>passed. Adding coverage after an invoice is overdue would allow an<br>attacker to insure a known-defaulting position and immediately collect<br>the payout — an adverse-selection exploit. The claim window closes at<br>the invoice `due_date`.<br>BREAKING: Do not renumber this variant. public ABI consumption. |
| 1500 | `InvalidRating` | `INV_RT` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1501 | `NotFunded` | `NOT_FD` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1502 | `AlreadyRated` | `ALR_RT` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1503 | `NotRater` | `NOT_RT` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1504 | `InvalidRatingOverrideReason` | `RT_OV_RSN` | Admin rating override was requested without a non-empty, bounded-length audit reason.<br>BREAKING: Do not renumber this variant. public ABI consumption. |
| 1600 | `BusinessNotVerified` | `BUS_NV` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1601 | `KYCAlreadyPending` | `KYC_PD` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1602 | `KYCAlreadyVerified` | `KYC_VF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1603 | `KYCNotFound` | `KYC_NF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1604 | `InvalidKYCStatus` | `KYC_IS` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1605 | `InvestorNotVerified` | `INV_NV` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1610 | `InvestorFrozen` | `INV_FRZ` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1660 | `BusinessDeleted` | `BUS_DEL` |  |
| 1700 | `AuditLogNotFound` | `AUD_NF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1701 | `AuditIntegrityError` | `AUD_IE` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1702 | `AuditQueryError` | `AUD_QE` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1800 | `InvalidTag` | `INV_TAG` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1801 | `TagLimitExceeded` | `TAG_LIM` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1850 | `InvalidFeeConfiguration` | `FEE_CFG` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1851 | `TreasuryNotConfigured` | `TRS_NC` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1852 | `InvalidFeeBasisPoints` | `FEE_BPS` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1853 | `RotationAlreadyPending` | `ROT_PND` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1854 | `RotationNotFound` | `ROT_NF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1855 | `RotationExpired` | `ROT_EXP` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1856 | `ArithmeticOverflow` | `ARITH_OF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1857 | `RotationTimelockNotElapsed` | `ROT_TLK` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1858 | `NoPendingTreasuryRotation` | `ROT_NOPND` | A treasury rotation was cancelled but no rotation was pending. |
| 1900 | `DisputeNotFound` | `DSP_NF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1901 | `DisputeAlreadyExists` | `DSP_EX` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1902 | `DisputeNotAuthorized` | `DSP_NA` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1903 | `DisputeAlreadyResolved` | `DSP_RS` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1904 | `DisputeNotUnderReview` | `DSP_UR` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1905 | `InvalidDisputeReason` | `DSP_RN` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1906 | `InvalidDisputeEvidence` | `DSP_EV` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1907 | `DisputeActive` | `DSP_ACT` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 1908 | `DisputeResolutionBlocksSettlement` | `DSP_RBLK` | A resolved dispute outcome forbids further fund movement on this<br>invoice:<br>- `FavorInvestor` — escrow was refunded to the investor; settlement<br>can never proceed.<br>- `Split` — funds are held pending a follow-up admin action.<br>Distinct from `DisputeActive` (1907), which covers *open* disputes. |
| 2000 | `NotificationNotFound` | `NOT_NF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2001 | `NotificationBlocked` | `NOT_BL` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2002 | `NotificationDuplicate` | `NOT_DUP` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2100 | `ContractPaused` | `PAUSED` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2101 | `EmergencyWithdrawNotFound` | `EMG_NF` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2102 | `EmergencyWithdrawTimelockNotElapsed` | `EMG_TLK` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2103 | `EmergencyWithdrawExpired` | `EMG_EXP` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2104 | `EmergencyWithdrawCancelled` | `EMG_CNL` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2105 | `EmergencyWithdrawAlreadyExists` | `EMG_EX` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2106 | `EmergencyWithdrawInsufficientBalance` | `EMG_BAL` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2200 | `TokenTransferFailed` | `TKN_FAIL` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2201 | `MaintenanceModeActive` | `MAINT` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2202 | `DuplicateDefaultTransition` | `DEF_DUP` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2203 | `BackfillInProgress` | `BKF_IP` | A destructive contract migration (e.g. `schedule_upgrade`) was<br>attempted while an in-progress backfill (e.g. `restore_from_backup`)<br>has not yet cleared its pending flag. Letting a migration race a<br>backfill leaves the new contract code interpreting partially<br>restored state, with no signal either side has to detect it.<br>BREAKING: Do not renumber this variant. public ABI consumption. |
| 2204 | `DuplicateBid` | `DUP_BID` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2205 | `InvalidLedgerSequence` | `INV_LS` | Settlement attempted while a dispute is open on the invoice.<br>Threat: a business could otherwise race to finalize settlement and<br>release escrowed funds before an admin resolves a pending dispute,<br>removing the investor's ability to recover their principal.<br>Distinct from `InvalidStatus` (1401) so callers and monitors can tell<br>"wrong lifecycle state" apart from "dispute must be resolved first".<br>BREAKING: Do not renumber this variant. public ABI consumption. |
| 2206 | `InsuranceNotActive` | `INS_NACT` | Insurance coverage is not active at the time of default/settlement. |
| 2207 | `ActiveDisputeExists` | `DSP_ACT` | A report/analytics-snapshot was requested while an invoice has an<br>unresolved (`Disputed` or `UnderReview`) dispute. |
| 2208 | `StaleInvestmentSnapshot` | `STL_INV` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2209 | `DuplicateNonce` | `DUP_NONCE` | A payment was submitted with a nonce that has already been recorded for<br>this invoice.  Duplicate nonces are rejected at the boundary to keep the<br>settlement ledger strictly auditable.<br>BREAKING: Do not renumber this variant. public ABI consumption. |
| 2210 | `InsufficientKYCTier` | `TIER_LOW` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2211 | `PendingGovernanceProposal` | `GOV_PROP` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2212 | `UnstableCursor` | `UNSTABLE` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2213 | `SettlementCurrencyNotAllowed` | `SETL_CR` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2214 | `UpgradePending` | `UPG_PEND` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2215 | `PerInvestorPositionCapExceeded` | `POS_CAP` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2216 | `BidBelowTierMinimum` | `TIER_BID` | The investor's KYC tier is too low for the requested operation.<br>BREAKING: Do not renumber this variant. public ABI consumption. |
| 2217 | `InvalidTransactionHash` | `TX_HASH` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2218 | `BatchSizeExceeded` | `BATCH_SZ` | BREAKING: Do not renumber this variant. public ABI consumption. |
| 2219 | `BidStale` | `BID_STL` | A bid was rejected because it is stale: the bid has been cancelled,<br>expired, or otherwise transitioned out of `Placed` status between<br>when the caller read it and when the operation was submitted.<br>Client retry contract: re-read bid state, select a new best bid<br>if needed, and resubmit.<br>BREAKING: Do not renumber this variant. public ABI consumption. |
| 2220 | `MutationLimitExceeded` | `MUT_LIM` | Per-address mutation rate limit exceeded within the current rate-limit<br>window.  The caller exceeded the maximum number of state-mutating<br>transactions allowed per address per window.  Retry after the window<br>resets (next `RATE_LIMIT_WINDOW_SEQUENCES` ledger sequences).<br>BREAKING: Do not renumber this variant. public ABI consumption. |
| 2221 | `InputTooLarge` | `IN_TOO_L` | An input field exceeds the hard size ceiling for the given resource.<br>This is an early-exit guard placed *before* expensive parsing, hashing,<br>or storage writes so that an oversized payload is rejected cheaply.<br>BREAKING: Do not renumber this variant. public ABI consumption. |
| 2222 | `UpgradeTimelockNotElapsed` | `UPG_TLK` | Attempted to execute a scheduled upgrade before the timelock delay elapsed.<br>The upgrade can only be executed after scheduled_at + UPGRADE_DELAY_SECS.<br>BREAKING: Do not renumber this variant. public ABI consumption. |

## FreshnessError (data freshness sub-error)

Defined in `quicklendx-contracts/src/freshness.rs`.

| Code | Variant | Meaning |
| ---: | --- | --- |
| 1 | `NotAuthorized` | The caller lacks authorization for the freshness operation. |
| 2 | `StaleDataRejected` | Off-chain data exceeds the configured freshness drift threshold. |
| 3 | `InvalidConfigValue` | The freshness configuration value is invalid. |

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

Run `quicklendx-contracts/scripts/gen-error-docs.sh` after changing `QuickLendXError`. Use `--check` to verify the committed documentation without modifying files. The generator reads the enum and its ABI symbol mapping directly and fails if it cannot parse every variant and mapping.

Any change to the public error surface requires review. **Never renumber an existing variant** — numeric values are public ABI.
