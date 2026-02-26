# Event System Documentation

## Overview

The QuickLendX protocol emits structured events for all critical contract operations to enable off-chain indexing, monitoring, and transparent audit trails. 

**Key Principles:**
- **Complete**: Every critical state change emits an event
- **Transparent**: All relevant data included (amounts, addresses, identifiers)
- **Immutable**: Blockchain-recorded events cannot be modified
- **Chronological**: Timestamps enable time-based ordering and analytics
- **Efficient**: Short 6-character topics minimize storage costs

## Comprehensive Event Schema

### Invoice Events

#### InvoiceUploaded
Emitted when a business uploads an invoice.

**Topic:** `inv_up`

**Data:**
- `invoice_id: BytesN<32>` - Unique invoice identifier
- `business: Address` - Business address that uploaded the invoice
- `amount: i128` - Invoice amount
- `currency: Address` - Currency token address
- `due_date: u64` - Due date timestamp
- `timestamp: u64` - Event timestamp

#### InvoiceVerified
Emitted when an invoice is verified by admin.

**Topic:** `inv_ver`

**Data:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `business: Address` - Business address
- `timestamp: u64` - Event timestamp

#### InvoiceCancelled
Emitted when an invoice is cancelled by the business owner.

**Topic:** `inv_canc`

**Data:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `business: Address` - Business address
- `timestamp: u64` - Event timestamp

#### InvoiceSettled
Emitted when an invoice is fully settled.

**Topic:** `inv_set`

**Data:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `business: Address` - Business address
- `investor: Address` - Investor address
- `investor_return: i128` - Amount returned to investor
- `platform_fee: i128` - Platform fee collected
- `timestamp: u64` - Event timestamp

#### InvoiceDefaulted
Emitted when an invoice defaults after grace period.

**Topic:** `inv_def`

**Data:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `business: Address` - Business address
- `investor: Address` - Investor address
- `timestamp: u64` - Event timestamp

### Bid Events

#### BidPlaced
Emitted when an investor places a bid on an invoice.

**Topic:** `bid_plc`

**Data:**
- `bid_id: BytesN<32>` - Unique bid identifier
- `invoice_id: BytesN<32>` - Invoice identifier
- `investor: Address` - Investor address
- `bid_amount: i128` - Bid amount
- `expected_return: i128` - Expected return amount
- `timestamp: u64` - Bid placement timestamp
- `expiration_timestamp: u64` - Bid expiration timestamp

#### BidAccepted
Emitted when a business accepts a bid.

**Topic:** `bid_acc`

**Data:**
- `bid_id: BytesN<32>` - Bid identifier
- `invoice_id: BytesN<32>` - Invoice identifier
- `investor: Address` - Investor address
- `business: Address` - Business address
- `bid_amount: i128` - Accepted bid amount
- `expected_return: i128` - Expected return
- `timestamp: u64` - Event timestamp

#### BidWithdrawn
Emitted when an investor withdraws their bid.

**Topic:** `bid_wdr`

**Data:**
- `bid_id: BytesN<32>` - Bid identifier
- `invoice_id: BytesN<32>` - Invoice identifier
- `investor: Address` - Investor address
- `bid_amount: i128` - Withdrawn bid amount
- `timestamp: u64` - Event timestamp

#### BidExpired
Emitted when a bid expires.

**Topic:** `bid_exp`

**Data:**
- `bid_id: BytesN<32>` - Bid identifier
- `invoice_id: BytesN<32>` - Invoice identifier
- `investor: Address` - Investor address
- `bid_amount: i128` - Expired bid amount
- `expiration_timestamp: u64` - Expiration timestamp

### Escrow Events

#### EscrowCreated
Emitted when escrow is created for an invoice.

**Topic:** `esc_cr`

**Data:**
- `escrow_id: BytesN<32>` - Escrow identifier
- `invoice_id: BytesN<32>` - Invoice identifier
- `investor: Address` - Investor address
- `business: Address` - Business address
- `amount: i128` - Escrow amount

#### EscrowReleased
Emitted when escrow funds are released to business.

**Topic:** `esc_rel`

**Data:**
- `escrow_id: BytesN<32>` - Escrow identifier
- `invoice_id: BytesN<32>` - Invoice identifier
- `business: Address` - Business address
- `amount: i128` - Released amount

#### EscrowRefunded
Emitted when escrow funds are refunded to investor.

**Topic:** `esc_ref`

**Data:**
- `escrow_id: BytesN<32>` - Escrow identifier
- `invoice_id: BytesN<32>` - Invoice identifier
- `investor: Address` - Investor address
- `amount: i128` - Refunded amount

### Other Events

#### PartialPayment
Emitted when a partial payment is made towards an invoice.

**Topic:** `inv_pp`

**Data:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `business: Address` - Business address
- `payment_amount: i128` - Payment amount
- `total_paid: i128` - Total paid so far
- `progress: u32` - Payment progress percentage (0-100)
- `transaction_id: String` - Transaction identifier

#### InvoiceExpired
Emitted when an invoice expires.

**Topic:** `inv_exp`

**Data:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `business: Address` - Business address
- `due_date: u64` - Due date timestamp

#### InvoiceFunded
Emitted when an invoice receives funding.

**Topic:** `inv_fnd`

**Data:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `investor: Address` - Investor funding address
- `amount: i128` - Funding amount
- `timestamp: u64` - Funding timestamp

#### InvoiceMetadataUpdated
Emitted when invoice metadata is set.

**Topic:** `inv_meta`

**Data:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `customer_name: String` - Customer name
- `tax_id: String` - Tax identification
- `line_item_count: u32` - Number of line items
- `total_value: i128` - Total value of items

## Critical Event Emission Guarantees

### Atomicity
- Events are emitted **after** state changes are committed
- No partial events for failed operations
- All-or-nothing event delivery

### Security Properties
- Events **cannot be forged or modified** once emitted
- Blockchain-recorded for immutability
- Authorization context is implicit in state transitions
- **Complete audit trail** for all financial transactions

### Data Completeness
- All relevant identifiers included (invoice_id, bid_id, addresses)
- All financial amounts included for reconciliation
- All timestamps for chronological ordering
- All status changes reflected in events

## Event Emission Checklist

For each critical operation, verify:

- [ ] Event is emitted **after** state is committed
- [ ] All required identifiers are included
- [ ] All financial amounts are present (investment, fees, returns)
- [ ] Timestamps are server-generated (not user input)
- [ ] Authorization context is verified before event
- [ ] Event topic is unique and memorable (6 chars)
- [ ] Event data matches operation results
- [ ] Event is logged for audit trail

## Testing Event Coverage

Every test should verify:

1. **Event Emission**: Action triggers expected event
2. **Event Data**: All fields have correct values
3. **Timestamp**: Event has valid timestamp
4. **Authorization**: Only authorized actors emit events
5. **Sequence**: Events in correct chronological order
6. **Completeness**: No financial amounts missing

Example test pattern:
```rust
#[test]
fn test_critical_operation_emits_event() {
    let env = Env::default();
    // Setup...
    
    // Perform operation that should emit event
    let result = contract.critical_operation(&param);
    
    // Verify: operation succeeded
    assert!(result.is_ok());
    
    // Verify: state changed correctly
    assert_eq!(state_after, expected_state);
    
    // Verify: all data is present for event
    assert!(!id.is_empty());
    assert!(amount > 0);
    assert!(timestamp_valid());
}
```

## Future Enhancements

Potential areas for event system expansion:

1. **Event Filters**: Off-chain subscribers can filter by criteria
2. **Event Aggregation**: Combine related events into transactions
3. **Event Replay**: Reconstruct state from event history
4. **Event Compression**: Archive old events with compression
5. **Event Versioning**: Support schema evolution over time
- `invoice_id: BytesN<32>` - Invoice identifier
- `total_paid: i128` - Total payment amount
- `timestamp: u64` - Finalization timestamp

## Dispute Events

### DisputeCreated
Emitted when a dispute is created on an invoice.

**Topic:** `dsp_cr`

**Data:**
- `invoice_id: BytesN<32>` - Disputed invoice
- `created_by: Address` - Dispute creator
- `reason: String` - Dispute reason
- `timestamp: u64` - Creation timestamp

### DisputeUnderReview
Emitted when dispute is escalated for review.

**Topic:** `dsp_ur`

**Data:**
- `invoice_id: BytesN<32>` - Disputed invoice
- `reviewed_by: Address` - Admin reviewer
- `timestamp: u64` - Review start time

### DisputeResolved
Emitted when dispute is resolved.

**Topic:** `dsp_rs`

**Data:**
- `invoice_id: BytesN<32>` - Disputed invoice
- `resolved_by: Address` - Admin resolver
- `resolution: String` - Resolution details
- `timestamp: u64` - Resolution timestamp

## Insurance Events

### InsuranceAdded
Emitted when insurance coverage is added.

**Topic:** `ins_add`

**Data:**
- `investment_id: BytesN<32>` - Investment identifier
- `invoice_id: BytesN<32>` - Insured invoice
- `investor: Address` - Investor address
- `provider: Address` - Insurance provider
- `coverage_percentage: u32` - Coverage percentage
- `coverage_amount: i128` - Maximum coverage
- `premium_amount: i128` - Premium paid

### InsuranceClaimed
Emitted when insurance claim is paid.

**Topic:** `ins_clm`

**Data:**
- `investment_id: BytesN<32>` - Investment identifier
- `invoice_id: BytesN<32>` - Associated invoice
- `provider: Address` - Insurance provider
- `coverage_amount: i128` - Claim payout

## Verification Events

### InvestorVerified
Emitted when investor KYC is verified.

**Topic:** `inv_veri`

**Data:**
- `investor: Address` - Verified investor
- `investment_limit: i128` - Investment authorization
- `verified_at: u64` - Verification timestamp

### InvestorAnalyticsUpdated
Emitted when investor metrics are calculated.

**Topic:** `inv_anal`

**Data:**
- `investor: Address` - Investor address
- `success_rate: i128` - Investment success percentage
- `risk_score: u32` - Investor risk score
- `compliance_score: u32` - Compliance score

## Fee and Treasury Events

### PlatformFeeRouted
Emitted when platform fees are transferred.

**Topic:** `fee_rout`

**Data:**
- `invoice_id: BytesN<32>` - Associated invoice
- `recipient: Address` - Treasury recipient
- `fee_amount: i128` - Fee amount transferred
- `timestamp: u64` - Transfer timestamp

### TreasuryConfigured
Emitted when treasury address is configured.

**Topic:** `trs_cfg`

**Data:**
- `treasury_address: Address` - Treasury address
- `configured_by: Address` - Configuring admin
- `timestamp: u64` - Configuration timestamp

### PlatformFeeConfigUpdated
Emitted when fee configuration is updated.

**Topic:** `fee_cfg`

**Data:**
- `old_fee_bps: u32` - Previous fee rate (basis points)
- `new_fee_bps: u32` - New fee rate
- `updated_by: Address` - Admin address
- `timestamp: u64` - Update timestamp

### ProfitFeeBreakdown
Emitted with detailed settlement breakdown.

**Topic:** `pf_brk`

**Data:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `investment_amount: i128` - Original investment
- `payment_amount: i128` - Total payment
- `gross_profit: i128` - Profit before fees
- `platform_fee: i128` - Platform fee deducted
- `investor_return: i128` - Net investor return
- `fee_bps_applied: i128` - Fee rate applied
- `timestamp: u64` - Calculation timestamp

## Backup and Recovery Events

### BackupCreated
Emitted when system backup is created.

**Topic:** `bkup_crt`

**Data:**
- `backup_id: BytesN<32>` - Backup identifier
- `invoice_count: u32` - Invoices in backup
- `timestamp: u64` - Creation timestamp

### BackupRestored
Emitted when backup is restored.

**Topic:** `bkup_rstr`

**Data:**
- `backup_id: BytesN<32>` - Restored backup
- `invoice_count: u32` - Restored invoices
- `timestamp: u64` - Restore timestamp

### RetentionPolicyUpdated
Emitted when retention policy changes.

**Topic:** `ret_pol`

**Data:**
- `max_backups: u32` - Maximum backups to retain
- `max_age_seconds: u64` - Maximum age in seconds
- `auto_cleanup_enabled: bool` - Cleanup flag
- `timestamp: u64` - Policy update time

## Analytics Events

### PlatformMetricsUpdated
Emitted when platform-wide metrics are calculated.

**Topic:** `plt_met`

**Data:**
- `total_invoices: u32` - Total invoices processed
- `total_volume: i128` - Total funding volume
- `total_fees: i128` - Total fees collected
- `success_rate: i128` - Overall success rate
- `timestamp: u64` - Calculation timestamp

### UserBehaviorAnalyzed
Emitted when user behavior metrics are calculated.

**Topic:** `usr_beh`

**Data:**
- `user: Address` - User address
- `total_investments: u32` - Total investments
- `success_rate: i128` - Success percentage
- `risk_score: u32` - Risk score
- `timestamp: u64` - Analysis timestamp

## Indexing Guidelines

### Event Topics
All events use short symbol topics (6 characters) for efficient storage and querying.

### Timestamps
All events include timestamps for chronological indexing and filtering.

### Addresses
All relevant addresses (business, investor) are included for efficient filtering by participant.

### Amounts
All financial amounts are included for analytics and reporting.

## Usage for Indexers

Indexers should:
1. Listen for all event topics
2. Store events with their full data payload
3. Index by invoice_id, business, investor for fast lookups
4. Track timestamps for time-based queries
5. Maintain bid history and investment history

## Security Notes

- Events are emitted after state changes are committed
- Events cannot be forged or modified
- All events include authorization context (who performed the action)
- Events are immutable once emitted

