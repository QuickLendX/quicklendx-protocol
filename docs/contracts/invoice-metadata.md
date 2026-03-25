# Invoice Metadata and String Length Limits

To ensure storage safety, prevent Denial-of-Service (DoS) attacks, and maintain consistent error handling, the QuickLendX protocol enforces strict length limits on all user-provided string fields.

## Core Limits

| Field | Limit (Characters) | Constant |
|-------|-------|----------|
| Invoice Description | 1024 | `MAX_DESCRIPTION_LENGTH` |
| Tag Length | 50 | `MAX_TAG_LENGTH` |
| Max Tags per Invoice | 10 | `MAX_TAGS_PER_INVOICE` |
| Business Name | 150 | `MAX_NAME_LENGTH` |
| Business Address | 300 | `MAX_ADDRESS_LENGTH` |
| Tax ID | 50 | `MAX_TAX_ID_LENGTH` |
| Invoice Notes | 2000 | `MAX_NOTES_LENGTH` |
| Line Item Description| 1024 | `MAX_DESCRIPTION_LENGTH` |
| Dispute Reason | 1000 | `MAX_DISPUTE_REASON_LENGTH` |
| Dispute Evidence | 2000 | `MAX_DISPUTE_EVIDENCE_LENGTH` |
| Dispute Resolution | 2000 | `MAX_DISPUTE_RESOLUTION_LENGTH` |

## Enforcement Logic

Validation is performed at the contract entry points (e.g., `upload_invoice`, `set_metadata`, `create_dispute`). If a string exceeds its defined limit, the transaction will revert with an appropriate error code (e.g., `InvalidDescription`, `TagLimitExceeded`).

## Rationale

1. **Storage Economics**: Soroban storage is metered. Unbounded strings can lead to unpredictable and excessive resource consumption.
2. **Safety**: Prevents malicious actors from bloating contract storage with garbage data.
3. **Consistency**: Provides a uniform experience for integrated clients by mapping length violations to standard protocol errors.
