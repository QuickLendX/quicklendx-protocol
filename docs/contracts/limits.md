# Contract String Length Limits

To ensure predictable storage usage and prevent potential resource abuse, the QuickLendX protocol enforces maximum length limits on user-supplied strings. 

## Defined Limits

These limits are defined in `src/protocol_limits.rs` and enforced across the contract modules.

| Input Field | Maximum Length (Bytes) | Module |
|-------------|-------------------------|--------|
| Invoice Description | 500 | `invoice` |
| Rating Feedback | 200 | `invoice` |
| Customer Name (Metadata) | 100 | `invoice` |
| Customer Address (Metadata) | 200 | `invoice` |
| Tax ID (Metadata) | 50 | `invoice` |
| Notes (Metadata) | 1000 | `invoice` |
| Dispute Reason | 500 | `defaults` (disputes) |
| Dispute Evidence | 1000 | `defaults` (disputes) |
| Dispute Resolution | 2000 | `defaults` (disputes) |
| Notification Title | 100 | `notifications` |
| Notification Message | 500 | `notifications` |
| KYC Data | 5000 | `verification` |
| Rejection Reason | 1000 | `verification` |

## Error Handling

When a string exceeds its defined limit, the contract will return an `InvalidDescription` (Code 1012) error. 

> [!NOTE]
> `InvalidDescription` is used as a generic "invalid input string" error to maintain contract compatibility while adhering to SDK limitations on error variant counts.

## Validation Logic

Validation is performed using the `check_string_length` helper:

```rust
pub fn check_string_length(s: &String, max_len: u32) -> Result<(), QuickLendXError> {
    if s.len() > max_len {
        return Err(QuickLendXError::InvalidDescription);
    }
    Ok(())
}
```
