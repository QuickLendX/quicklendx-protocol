# Invoice Default Handling Documentation

## Overview

The QuickLendX contract implements comprehensive default handling for invoices that are not paid by their due date. The system includes a grace period mechanism to protect investor interests while allowing for recovery processes.

## Default Handling Flow

### 1. Invoice Lifecycle

```
Pending → Verified → Funded → [Due Date] → [Grace Period] → Defaulted
```

### 2. Grace Period

- **Default Grace Period**: 7 days (604,800 seconds)
- **Configurable**: Can be specified per invoice when calling `mark_invoice_defaulted`
- **Purpose**: Provides a buffer period after the due date before marking an invoice as defaulted

### 3. Default Detection

An invoice can be marked as defaulted when:
1. Invoice status is `Funded`
2. Current timestamp > (due_date + grace_period)
3. Invoice has not already been defaulted

## Entry Points

### `mark_invoice_defaulted`

Marks an invoice as defaulted after checking the grace period.

**Parameters:**
- `invoice_id: BytesN<32>` - The invoice ID to mark as defaulted
- `grace_period: Option<u64>` - Optional grace period in seconds (defaults to 7 days)

**Returns:**
- `Ok(())` if successful
- `Err(QuickLendXError)` if operation fails

**Authorization:** Requires admin authentication. Only the configured admin address can call this function.

**Error Conditions:**
- `NotAdmin` (1005) - No admin configured or caller is not admin
- `InvoiceNotFound` (1000) - Invoice does not exist
- `InvoiceAlreadyDefaulted` (1049) - Invoice is already defaulted (no double default)
- `InvoiceNotAvailableForFunding` (1047) - Invoice is not in Funded status
- `OperationNotAllowed` (1009) - Grace period has not expired yet

**Example:**
```rust
// Use default grace period (7 days)
contract.mark_invoice_defaulted(invoice_id, None)?;

// Use custom grace period (3 days)
let custom_grace = 3 * 24 * 60 * 60;
contract.mark_invoice_defaulted(invoice_id, Some(custom_grace))?;
```

### `handle_default` (Internal)

Internal function that performs the actual defaulting. Assumes all validations have been done.

**Authorization:** Requires admin authentication.

**Note**: This function is called internally by `mark_invoice_defaulted` after validation.

## State Transitions

When an invoice is marked as defaulted:

1. **Invoice Status**: `Funded` → `Defaulted`
2. **Status Lists**: Removed from `Funded` list, added to `Defaulted` list
3. **Investment Status**: `Active` → `Defaulted`
4. **Insurance Claims**: Processed if insurance coverage exists
5. **Events Emitted**:
   - `invoice_expired`
   - `invoice_defaulted`
   - `insurance_claimed` (if applicable)
6. **Notifications**: Default notification sent to relevant parties

## Grace Period Logic

### Calculation

```rust
grace_deadline = due_date + grace_period
can_default = current_timestamp > grace_deadline
```

### Examples

**Example 1: Default Grace Period**
- Due Date: Day 0
- Grace Period: 7 days (default)
- Grace Deadline: Day 7
- Can Default: After Day 7

**Example 2: Custom Grace Period**
- Due Date: Day 0
- Grace Period: 3 days (custom)
- Grace Deadline: Day 3
- Can Default: After Day 3

**Example 3: Before Grace Period**
- Due Date: Day 0
- Current Time: Day 2
- Grace Period: 7 days
- Grace Deadline: Day 7
- Can Default: No (Day 2 < Day 7)

## Recovery Options

### For Investors

1. **Insurance Claims**: If insurance coverage exists, claims are automatically processed
2. **Dispute Resolution**: Investors can create disputes for defaulted invoices
3. **Analytics Tracking**: Defaulted investments are tracked for risk assessment

### For Businesses

1. **Payment Recovery**: Businesses can still pay defaulted invoices (partial payments)
2. **Dispute Resolution**: Businesses can respond to disputes
3. **Reputation Impact**: Defaults affect business verification status

## Testing

Comprehensive tests are available in `test_default.rs`:

- ✅ Default after grace period
- ✅ No default before grace period
- ✅ Cannot default unfunded invoices
- ✅ Cannot default already defaulted invoices
- ✅ Custom grace period support
- ✅ Default grace period when none provided
- ✅ Status transition verification
- ✅ Investment status update
- ✅ Edge cases (exactly at deadline, multiple invoices)
- ✅ Zero grace period (immediate default after due date)
- ✅ Cannot default paid invoices

## Security Considerations

1. **Authorization**: Default marking requires admin authentication (`require_auth`)
2. **State Validation**: Only funded invoices can be defaulted
3. **Idempotency**: Multiple default attempts are prevented with `InvoiceAlreadyDefaulted` error
4. **Grace Period Protection**: Investors are protected during grace period
5. **No Double Default**: Already defaulted invoices return a specific `InvoiceAlreadyDefaulted` error
6. **Check Ordering**: Defaulted status is checked before funded status to ensure correct error reporting

## Frontend Integration

### Checking Default Status

```typescript
const invoice = await contract.get_invoice(invoiceId);
const isDefaulted = invoice.status === InvoiceStatus.Defaulted;
```

### Marking as Defaulted

```typescript
try {
  // Use default grace period
  await contract.mark_invoice_defaulted(invoiceId, null);
  
  // Or use custom grace period (3 days)
  const customGrace = 3 * 24 * 60 * 60;
  await contract.mark_invoice_defaulted(invoiceId, customGrace);
} catch (error) {
  if (error.code === 1005) {
    // NotAdmin
    console.error("Only admin can mark invoices as defaulted");
  } else if (error.code === 1049) {
    // InvoiceAlreadyDefaulted
    console.error("Invoice is already defaulted");
  } else if (error.code === 1047) {
    // InvoiceNotAvailableForFunding
    console.error("Invoice must be in Funded status");
  } else if (error.code === 1009) {
    // OperationNotAllowed
    console.error("Grace period has not expired");
  }
}
```

### Monitoring Defaults

```typescript
// Get all defaulted invoices
const defaulted = await contract.get_invoices_by_status(InvoiceStatus.Defaulted);

// Check if invoice is overdue (before grace period expires)
const invoice = await contract.get_invoice(invoiceId);
const now = Date.now() / 1000; // Convert to seconds
const gracePeriod = 7 * 24 * 60 * 60; // 7 days
const isOverdue = now > invoice.dueDate;
const canDefault = now > (invoice.dueDate + gracePeriod);
```

## Configuration

### Default Grace Period

The default grace period is defined in `defaults.rs`:

```rust
pub const DEFAULT_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days
```

This can be overridden per invoice when calling `mark_invoice_defaulted`.

## Events

### `invoice_defaulted`

Emitted when an invoice is marked as defaulted.

**Event Data:**
- `invoice_id: BytesN<32>`
- `business: Address`
- `investor: Address`
- `amount: i128`
- `defaulted_at: u64`

### `invoice_expired`

Emitted when an invoice expires (due date + grace period).

**Event Data:**
- `invoice_id: BytesN<32>`
- `due_date: u64`
- `expired_at: u64`

### `insurance_claimed`

Emitted when insurance is claimed for a defaulted invoice.

**Event Data:**
- `investment_id: BytesN<32>`
- `invoice_id: BytesN<32>`
- `provider: Address`
- `coverage_amount: i128`

## Best Practices

1. **Monitor Grace Periods**: Regularly check for invoices approaching default
2. **Automated Defaulting**: Use automated processes to mark defaults after grace period
3. **Notify Stakeholders**: Send notifications before and after default
4. **Track Analytics**: Monitor default rates for risk assessment
5. **Recovery Processes**: Implement recovery workflows for defaulted invoices

