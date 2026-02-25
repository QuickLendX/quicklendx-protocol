# QuickLendX Contract Storage Schema

## Overview

This document describes the on-chain data model and storage schema for the QuickLendX invoice factoring protocol. The schema is designed for the MVP flow: invoice upload → bids → accept → settlement.

## Core Types

### Invoice
- **ID**: `BytesN<32>` - Unique identifier
- **Business**: `Address` - Business that uploaded the invoice
- **Amount**: `i128` - Total invoice amount
- **Currency**: `Address` - Currency token address
- **Due Date**: `u64` - Due date timestamp
- **Status**: `InvoiceStatus` - Current lifecycle status
- **Metadata**: `InvoiceMetadata` - Customer info, line items, etc.
- **Payments**: `Vec<PaymentRecord>` - Payment history
- **Ratings**: `Vec<InvoiceRating>` - Investor feedback

### Bid
- **ID**: `BytesN<32>` - Unique bid identifier
- **Invoice ID**: `BytesN<32>` - Invoice being bid on
- **Investor**: `Address` - Investor making the bid
- **Amount**: `i128` - Bid amount
- **Expected Return**: `i128` - Expected return amount
- **Status**: `BidStatus` - Current bid status
- **Expiration**: `u64` - Bid expiration timestamp

### Investment
- **ID**: `BytesN<32>` - Unique investment identifier
- **Invoice ID**: `BytesN<32>` - Invoice being invested in
- **Investor**: `Address` - Investor address
- **Amount**: `i128` - Investment amount
- **Status**: `InvestmentStatus` - Current investment status
- **Insurance**: `Vec<InsuranceCoverage>` - Insurance coverages

## Status Enums

### InvoiceStatus
- `Pending` - Awaiting verification
- `Verified` - Available for bidding
- `Funded` - Has been funded
- `Paid` - Settled successfully
- `Defaulted` - Payment overdue
- `Cancelled` - Cancelled by business
- `Refunded` - Escrow funds returned to investor

### BidStatus
- `Placed` - Active bid
- `Withdrawn` - Withdrawn by investor
- `Accepted` - Accepted by business
- `Expired` - Expired without acceptance
- `Cancelled` - Cancelled due to refund or withdrawal

### InvestmentStatus
- `Active` - Currently funding invoice
- `Withdrawn` - Withdrawn by investor
- `Completed` - Invoice paid successfully
- `Defaulted` - Invoice defaulted
- `Refunded` - Investment refunded to investor

## Storage Keys

### Primary Storage
- `invoice_id` → `Invoice`
- `bid_id` → `Bid`
- `investment_id` → `Investment`

### Instance Storage
- `fees` → `PlatformFeeConfig`

### Counters
- `inv_count` → `u64` - Invoice counter
- `bid_count` → `u64` - Bid counter
- `invst_count` → `u64` - Investment counter

## Secondary Indexes

### Invoices
- `inv_bus + business_address` → `Vec<BytesN<32>>` - Invoices by business
- `inv_stat + status` → `Vec<BytesN<32>>` - Invoices by status

### Bids
- `bids_inv + invoice_id` → `Vec<BytesN<32>>` - Bids by invoice
- `bids_invstr + investor` → `Vec<BytesN<32>>` - Bids by investor
- `bids_stat + status` → `Vec<BytesN<32>>` - Bids by status

### Investments
- `invst_inv + invoice_id` → `Vec<BytesN<32>>` - Investments by invoice
- `invst_invstr + investor` → `Vec<BytesN<32>>` - Investments by investor
- `invst_stat + status` → `Vec<BytesN<32>>` - Investments by status

## Security Considerations

### Storage Collisions
- All keys use unique symbols to prevent collisions
- Primary keys use entity IDs (BytesN<32>) for uniqueness
- Index keys combine symbols with entity-specific data

### Upgrade Safety
- Storage keys are designed to be backward compatible
- New fields can be added to structs without breaking existing data
- Index keys use stable symbols that won't change

### Access Control
- Only authorized addresses can modify data
- Business can only modify their own invoices
- Investors can only modify their own bids/investments

### Data Integrity
- All monetary amounts use `i128` to prevent overflow
- Timestamps use `u64` for Unix timestamps
- Addresses use Soroban's `Address` type for built-in validation

## Performance Characteristics

### Read Operations
- Primary entity lookup: O(1)
- Index queries: O(n) where n is number of entities in index
- Status-based queries: Efficient for filtering active entities

### Write Operations
- Entity updates: O(1) for primary storage
- Index updates: O(n) for index maintenance
- Batch operations: Optimized for common workflows

### Storage Costs
- Persistent storage used for long-term data
- Instance storage for frequently accessed config
- Indexes increase storage costs but improve query performance

## MVP Flow Storage Usage

1. **Invoice Upload**: Store invoice, update business and status indexes
2. **Bid Placement**: Store bid, update invoice, investor, and status indexes
3. **Bid Acceptance**: Update bid status, create investment, update indexes
4. **Settlement**: Update invoice and investment statuses, record payments

## Future Extensions

The schema is designed to support future features:
- Dispute resolution (already included in Invoice struct)
- Insurance claims (included in Investment struct)
- Analytics and reporting (separate analytics storage)
- Multi-currency support (currency field in Invoice)
- Partial payments (payments vector in Invoice)