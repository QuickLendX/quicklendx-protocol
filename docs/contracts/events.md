# Event System Documentation

## Overview

The QuickLendX protocol emits structured events for all major contract operations to enable off-chain indexing and frontend updates. All events include timestamps and relevant data for comprehensive tracking.

## Event Schema

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
