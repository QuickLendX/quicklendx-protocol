# Contract Errors and Input Validation

This document defines the typed error surface for `QuickLendXError`, the validation rules enforced by public entrypoints, and the protocol limits used to reject malformed input early.

## Error catalog

The complete `QuickLendXError` catalog, including numeric discriminants, ABI symbols, and descriptions extracted from the Rust source, is maintained in the [generated error code reference](../../../docs/ERROR_CODES.md).

Run `quicklendx-contracts/scripts/gen-error-docs.sh --check` from any directory to verify that reference is current.

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
