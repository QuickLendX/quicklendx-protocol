# QuickLendX Event System - Complete Reference

## Overview

The QuickLendX protocol emits structured events for all critical operations to enable off-chain indexing, monitoring, and frontend updates. This comprehensive guide documents every event in the system.

### Event Architecture

- **Event Topics**: 6-character short symbols for efficient storage and querying
- **Timestamps**: All events include server-generated timestamps for chronological indexing
- **Authorization**: Events reflect the action taken; authorization context is implicit
- **Immutability**: Events are blockchain-recorded and immutable
- **Completeness**: All relevant identifiers and amounts are included for comprehensive tracking

---

## Invoice Events

### InvoiceUploaded (`inv_up`)
Emitted when a business uploads a new invoice to the protocol.

**Data Fields:**
- `invoice_id: BytesN<32>` - Unique invoice identifier
- `business: Address` - Business uploading the invoice
- `amount: i128` - Invoice principal amount
- `currency: Address` - Currency token address
- `due_date: u64` - Invoice due date (Unix timestamp)
- `timestamp: u64` - Event creation timestamp

**Use Case:** Index new invoices, enable dashboard updates, track submission metrics

---

### InvoiceVerified (`inv_ver`)
Emitted when an admin verifies an invoice for funding.

**Data Fields:**
- `invoice_id: BytesN<32>` - Verified invoice identifier
- `business: Address` - Business owner
- `timestamp: u64` - Verification timestamp

**Use Case:** Notify businesses of verification, display status changes, track approvals

---

### InvoiceCancelled (`inv_canc`)
Emitted when an invoice is cancelled by the business owner.

**Data Fields:**
- `invoice_id: BytesN<32>` - Cancelled invoice identifier
- `business: Address` - Business owner
- `timestamp: u64` - Cancellation timestamp

**Use Case:** Update invoice listings, notify investors, handle refunds, audit trail

---

### InvoiceSettled (`inv_set`)
Emitted when an invoice is fully settled (payment received and distributed).

**Data Fields:**
- `invoice_id: BytesN<32>` - Settled invoice identifier
- `business: Address` - Recipient of settlement
- `investor: Address` - Investor who funded the invoice
- `investor_return: i128` - Amount paid to investor
- `platform_fee: i128` - Platform fee collected
- `timestamp: u64` - Settlement timestamp

**Use Case:** Calculate investor returns, track earnings, update portfolio metrics

---

### InvoiceDefaulted (`inv_def`)
Emitted when an invoice defaults after the grace period expires.

**Data Fields:**
- `invoice_id: BytesN<32>` - Defaulted invoice identifier
- `business: Address` - Business that defaulted
- `investor: Address` - Investor affected
- `timestamp: u64` - Default timestamp

**Use Case:** Flag risky businesses, update risk scores, trigger insurance claims, alert investors

---

### InvoiceExpired (`inv_exp`)
Emitted when an invoice's bidding window expires without acceptance.

**Data Fields:**
- `invoice_id: BytesN<32>` - Expired invoice identifier
- `business: Address` - Business owner
- `due_date: u64` - Original due date

**Use Case:** Clean up expired auctions, notify businesses, update dashboard

---

### InvoiceFunded (`inv_fnd`)
Emitted when an invoice receives funding from an accepted bid.

**Data Fields:**
- `invoice_id: BytesN<32>` - Funded invoice identifier
- `investor: Address` - Investor providing funding
- `amount: i128` - Funding amount
- `timestamp: u64` - Funding timestamp

**Use Case:** Track investment activity, update investor portfolio, calculate allocations

---

### InvoiceMetadataUpdated (`inv_meta`)
Emitted when invoice metadata (line items, tax ID, customer name) is added.

**Data Fields:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `customer_name: String` - End customer name
- `tax_id: String` - Tax identification
- `line_item_count: u32` - Number of line items
- `total_value: i128` - Total value of line items

**Use Case:** Enhance invoice details, support detailed reporting

---

### InvoiceMetadataCleared (`inv_mclr`)
Emitted when invoice metadata is removed.

**Data Fields:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `business: Address` - Business owner

**Use Case:** Track metadata lifecycle, audit changes

---

### PaymentRecorded (`pay_rec`)
Emitted for each individual payment transaction.

**Data Fields:**
- `payer: Address` - Address making payment
- `amount: i128` - Payment amount
- `transaction_id: String` - External transaction identifier
- `timestamp: u64` - Payment timestamp

**Use Case:** Reconcile external payments, track cash flow

---

### PartialPayment (`inv_pp`)
Emitted when a partial payment is applied to an invoice.

**Data Fields:**
- `invoice_id: BytesN<32>` - Invoice receiving payment
- `business: Address` - Business owner
- `payment_amount: i128` - Amount of this payment
- `total_paid: i128` - Cumulative amount paid
- `progress: u32` - Payment progress (0-100%)
- `transaction_id: String` - Transaction identifier

**Use Case:** Track settlement progress, update payment status, enable partial payment dashboards

---

### InvoiceSettledFinal (`inv_stlf`)
Emitted when invoice settlement is finalized (distinct from initial settlement event).

**Data Fields:**
- `invoice_id: BytesN<32>` - Settled invoice identifier
- `total_paid: i128` - Total payment received
- `timestamp: u64` - Finalization timestamp

**Use Case:** Mark settlement as complete in off-chain systems

---

### InvoiceCategoryUpdated (`cat_upd`)
Emitted when invoice classification category is changed.

**Data Fields:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `business: Address` - Business owner
- `old_category: InvoiceCategory` - Previous category
- `new_category: InvoiceCategory` - New category

**Use Case:** Update invoice classification, recalculate category metrics

---

### InvoiceTagAdded (`tag_add`)
Emitted when a tag is added to an invoice.

**Data Fields:**
- `invoice_id: BytesN<32>` - Tagged invoice identifier
- `business: Address` - Business owner
- `tag: String` - Tag value added

**Use Case:** Enable tag-based filtering and search

---

### InvoiceTagRemoved (`tag_rm`)
Emitted when a tag is removed from an invoice.

**Data Fields:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `business: Address` - Business owner
- `tag: String` - Tag value removed

**Use Case:** Update tag indices, remove from search results

---

## Bid Events

### BidPlaced (`bid_plc`)
Emitted when an investor places a bid on an invoice.

**Data Fields:**
- `bid_id: BytesN<32>` - Unique bid identifier
- `invoice_id: BytesN<32>` - Target invoice identifier
- `investor: Address` - Investor placing bid
- `bid_amount: i128` - Amount investor will fund
- `expected_return: i128` - Expected return on investment
- `timestamp: u64` - Bid placement timestamp
- `expiration_timestamp: u64` - When bid expires

**Use Case:** Display bids to business, calculate auction metrics, track investor activity

---

### BidAccepted (`bid_acc`)
Emitted when a business accepts a bid.

**Data Fields:**
- `bid_id: BytesN<32>` - Accepted bid identifier
- `invoice_id: BytesN<32>` - Associated invoice
- `investor: Address` - Winning investor
- `business: Address` - Accepting business
- `bid_amount: i128` - Accepted bid amount
- `expected_return: i128` - Agreed return amount
- `timestamp: u64` - Acceptance timestamp

**Use Case:** Trigger escrow creation, update bids to rejected status, notify losing bidders

---

### BidWithdrawn (`bid_wdr`)
Emitted when an investor withdraws their bid.

**Data Fields:**
- `bid_id: BytesN<32>` - Withdrawn bid identifier
- `invoice_id: BytesN<32>` - Associated invoice
- `investor: Address` - Bidding investor
- `bid_amount: i128` - Bid amount that was withdrawn
- `timestamp: u64` - Withdrawal timestamp

**Use Case:** Update bid status, free up investor capital, refresh auction display

---

### BidExpired (`bid_exp`)
Emitted when a bid expires without being accepted.

**Data Fields:**
- `bid_id: BytesN<32>` - Expired bid identifier
- `invoice_id: BytesN<32>` - Associated invoice
- `investor: Address` - Original bidder
- `bid_amount: i128` - Bid amount that expired
- `expiration_timestamp: u64` - Expiration time

**Use Case:** Clean up expired bids, release frozen capital, update auction status

---

## Escrow Events

### EscrowCreated (`esc_cr`)
Emitted when escrow is established to hold investor funds.

**Data Fields:**
- `escrow_id: BytesN<32>` - Unique escrow identifier
- `invoice_id: BytesN<32>` - Associated invoice
- `investor: Address` - Funds owner
- `business: Address` - Invoice recipient (business)
- `amount: i128` - Amount held in escrow

**Use Case:** Confirm fund lock-up, display escrow status, enable dispute resolution

---

### EscrowReleased (`esc_rel`)
Emitted when escrow funds are released to the business.

**Data Fields:**
- `escrow_id: BytesN<32>` - Escrow identifier
- `invoice_id: BytesN<32>` - Associated invoice
- `business: Address` - Recipient address
- `amount: i128` - Released amount
- `timestamp: u64` - Release timestamp (implicit)

**Use Case:** Confirm business received funds, update business cash position

---

### EscrowRefunded (`esc_ref`)
Emitted when escrow funds are refunded to the investor.

**Data Fields:**
- `escrow_id: BytesN<32>` - Escrow identifier
- `invoice_id: BytesN<32>` - Associated invoice
- `investor: Address` - Refund recipient
- `amount: i128` - Refunded amount

**Use Case:** Confirm investor refund, restore capital allocation, resolve disputes

---

## Dispute Events

### DisputeCreated (`dsp_cr`)
Emitted when a dispute is opened on an invoice.

**Data Fields:**
- `invoice_id: BytesN<32>` - Disputed invoice
- `created_by: Address` - Party creating dispute
- `reason: String` - Dispute reason summary
- `timestamp: u64` - Creation timestamp

**Use Case:** Alert counterparties, escalate to review queue, send notifications

---

### DisputeUnderReview (`dsp_ur`)
Emitted when a dispute is escalated for admin review.

**Data Fields:**
- `invoice_id: BytesN<32>` - Disputed invoice
- `reviewed_by: Address` - Admin reviewer
- `timestamp: u64` - Review start timestamp

**Use Case:** Track review progress, set SLA timers, assign to case management

---

### DisputeResolved (`dsp_rs`)
Emitted when a dispute is resolved.

**Data Fields:**
- `invoice_id: BytesN<32>` - Disputed invoice
- `resolved_by: Address` - Admin resolver
- `resolution: String` - Resolution summary
- `timestamp: u64` - Resolution timestamp

**Use Case:** Notify parties, release held funds, update compliance records

---

## Insurance Events

### InsuranceAdded (`ins_add`)
Emitted when insurance coverage is added to an invoice.

**Data Fields:**
- `investment_id: BytesN<32>` - Investment identifier
- `invoice_id: BytesN<32>` - Insured invoice
- `investor: Address` - Investor beneficiary
- `provider: Address` - Insurance provider
- `coverage_percentage: u32` - Coverage level (0-100%)
- `coverage_amount: i128` - Maximum coverage amount
- `premium_amount: i128` - Insurance premium paid

**Use Case:** Calculate risk exposure, display coverage details, track insurance costs

---

### InsurancePremiumCollected (`ins_prm`)
Emitted when insurance premiums are collected from investors.

**Data Fields:**
- `investment_id: BytesN<32>` - Investment identifier
- `provider: Address` - Insurance provider receiving premium
- `premium_amount: i128` - Premium collected

**Use Case:** Reconcile provider payments, track premium revenue

---

### InsuranceClaimed (`ins_clm`)
Emitted when an insurance claim is paid out.

**Data Fields:**
- `investment_id: BytesN<32>` - Investment identifier
- `invoice_id: BytesN<32>` - Associated invoice
- `provider: Address` - Insurance provider
- `coverage_amount: i128` - Claim payout amount

**Use Case:** Record insurance recovery, update portfolio metrics, reconcile claims

---

## Verification Events

### InvestorVerified (`inv_veri`)
Emitted when an investor is KYC-verified and approved for investment.

**Data Fields:**
- `investor: Address` - Verified investor address
- `investment_limit: i128` - Maximum investment authorization
- `verified_at: u64` - Verification timestamp

**Use Case:** Enable trading activity, update investor status dashboard, send approval email

---

### InvestorAnalyticsUpdated (`inv_anal`)
Emitted when investor analytics/metrics are calculated.

**Data Fields:**
- `investor: Address` - Investor address
- `success_rate: i128` - Percentage of successful investments
- `risk_score: u32` - Calculated risk score (0-1000)
- `compliance_score: u32` - Compliance score (0-100)

**Use Case:** Track investor health, adjust allocations, identify patterns

---

### InvestorPerformanceUpdated (`inv_perf`)
Emitted when overall investor population metrics are calculated.

**Data Fields:**
- `total_investors: u32` - Total investor count
- `verified_investors: u32` - Verified investor count
- `platform_success_rate: i128` - Overall platform success rate
- `average_risk_score: u32` - Average investor risk score

**Use Case:** Generate platform dashboards, monitor ecosystem health

---

## Fee & Treasury Events

### PlatformFeeUpdated (`fee_upd`)
Emitted when platform fee configuration is modified.

**Data Fields:**
- `fee_bps: u32` - Fee rate in basis points (0-10000)
- `updated_at: u64` - Update timestamp
- `updated_by: Address` - Admin who updated

**Use Case:** Notify users of fee changes, update fee calculations

---

### PlatformFeeConfigUpdated (`fee_cfg`)
Emitted when fee configuration is changed by admin.

**Data Fields:**
- `old_fee_bps: u32` - Previous fee rate
- `new_fee_bps: u32` - New fee rate
- `updated_by: Address` - Admin address
- `timestamp: u64` - Update timestamp

**Use Case:** Compliance audit, user notifications, calculation updates

---

### PlatformFeeRouted (`fee_rout`)
Emitted when platform fees are transferred to treasury.

**Data Fields:**
- `invoice_id: BytesN<32>` - Associated invoice
- `recipient: Address` - Treasury/recipient address
- `fee_amount: i128` - Fee amount transferred
- `timestamp: u64` - Transfer timestamp

**Use Case:** Reconcile treasury balance, track fee collection

---

### TreasuryConfigured (`trs_cfg`)
Emitted when treasury address is configured.

**Data Fields:**
- `treasury_address: Address` - Treasury recipient address
- `configured_by: Address` - Admin address
- `timestamp: u64` - Configuration timestamp

**Use Case:** Compliance audit, verify payment routing

---

### ProfitFeeBreakdown (`pf_brk`)
Emitted with detailed settlement calculation transparency.

**Data Fields:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `investment_amount: i128` - Original investment principal
- `payment_amount: i128` - Total payment received
- `gross_profit: i128` - Profit before fees
- `platform_fee: i128` - Platform fee deducted
- `investor_return: i128` - Net investor return
- `fee_bps_applied: i128` - Fee rate applied (basis points)
- `timestamp: u64` - Calculation timestamp

**Use Case:** Transparent reporting, audit trail, dispute resolution

---

## Backup & Recovery Events

### BackupCreated (`bkup_crt`)
Emitted when a system backup is created.

**Data Fields:**
- `backup_id: BytesN<32>` - Backup identifier
- `invoice_count: u32` - Number of invoices backed up
- `timestamp: u64` - Backup creation time

**Use Case:** Monitor backup frequency, verify coverage

---

### BackupRestored (`bkup_rstr`)
Emitted when a backup is restored.

**Data Fields:**
- `backup_id: BytesN<32>` - Restored backup identifier
- `invoice_count: u32` - Restored invoice count
- `timestamp: u64` - Restore timestamp

**Use Case:** Document recovery operations, compliance audit

---

### BackupValidated (`bkup_vd`)
Emitted when backup integrity is verified.

**Data Fields:**
- `backup_id: BytesN<32>` - Backup identifier
- `success: bool` - Validation result
- `timestamp: u64` - Validation timestamp

**Use Case:** Confirm backup health, alert on failures

---

### BackupArchived (`bkup_ar`)
Emitted when a backup is archived for long-term retention.

**Data Fields:**
- `backup_id: BytesN<32>` - Archived backup identifier
- `timestamp: u64` - Archive timestamp

**Use Case:** Track archival history, manage storage

---

### RetentionPolicyUpdated (`ret_pol`)
Emitted when backup retention policy is modified.

**Data Fields:**
- `max_backups: u32` - Maximum backup count to retain
- `max_age_seconds: u64` - Maximum backup age in seconds
- `auto_cleanup_enabled: bool` - Automatic cleanup flag
- `timestamp: u64` - Policy update timestamp

**Use Case:** Configuration audit, retention tracking

---

### BackupsCleaned (`bkup_cln`)
Emitted when old backups are deleted per retention policy.

**Data Fields:**
- `removed_count: u32` - Number of backups removed
- `timestamp: u64` - Cleanup timestamp

**Use Case:** Monitor cleanup operations, verify retention compliance

---

## Audit Events

### AuditValidation (`aud_val`)
Emitted when invoice data passes audit validation.

**Data Fields:**
- `invoice_id: BytesN<32>` - Audited invoice
- `is_valid: bool` - Validation result
- `timestamp: u64` - Validation timestamp

**Use Case:** Track data quality, identify anomalies

---

### AuditQuery (`aud_qry`)
Emitted when audit logs are queried.

**Data Fields:**
- `query_type: String` - Type of audit query performed
- `result_count: u32` - Results returned

**Use Case:** Monitor audit access, security tracking

---

## Analytics Events

### PlatformMetricsUpdated (`plt_met`)
Emitted when overall platform metrics are calculated.

**Data Fields:**
- `total_invoices: u32` - Total invoices processed
- `total_volume: i128` - Total funding volume
- `total_fees: i128` - Total fees collected
- `success_rate: i128` - Overall success percentage
- `timestamp: u64` - Calculation timestamp

**Use Case:** Generate platform dashboards, market reports

---

### PerformanceMetricsUpdated (`perf_met`)
Emitted when performance metrics are calculated.

**Data Fields:**
- `average_settlement_time: u64` - Average time to settlement
- `transaction_success_rate: i128` - Transaction success percentage
- `user_satisfaction_score: u32` - Average satisfaction score
- `timestamp: u64` - Calculation timestamp

**Use Case:** Performance dashboards, SLA tracking

---

### UserBehaviorAnalyzed (`usr_beh`)
Emitted when individual user behavior metrics are calculated.

**Data Fields:**
- `user: Address` - User address
- `total_investments: u32` - Number of investments made
- `success_rate: i128` - Success percentage
- `risk_score: u32` - User risk score
- `timestamp: u64` - Analysis timestamp

**Use Case:** Personalized recommendations, KYC updates

---

### FinancialMetricsCalculated (`fin_met`)
Emitted when financial metrics are calculated for a period.

**Data Fields:**
- `period: TimePeriod` - Reporting period
- `total_volume: i128` - Volume for period
- `total_fees: i128` - Fees collected
- `average_return_rate: i128` - Average investor return
- `timestamp: u64` - Calculation timestamp

**Use Case:** Financial reporting, period-end reconciliation

---

### BusinessReportGenerated (`biz_rpt`)
Emitted when periodic business report is generated.

**Data Fields:**
- `report_id: BytesN<32>` - Report identifier
- `business: Address` - Business reported on
- `period: TimePeriod` - Reporting period
- `invoices_uploaded: u32` - Invoices submitted in period
- `success_rate: i128` - Success rate for period
- `timestamp: u64` - Report generation time

**Use Case:** Business dashboards, quarterly reports

---

### InvestorReportGenerated (`inv_rpt`)
Emitted when periodic investor report is generated.

**Data Fields:**
- `report_id: BytesN<32>` - Report identifier
- `investor: Address` - Investor reported on
- `period: TimePeriod` - Reporting period
- `investments_made: u32` - Investments in period
- `average_return_rate: i128` - Return rate for period
- `timestamp: u64` - Report generation time

**Use Case:** Investor statements, performance tracking

---

### AnalyticsQuery (`anal_qry`)
Emitted when analytics are queried.

**Data Fields:**
- `query_type: String` - Type of query executed
- `filters_applied: u32` - Number of filters used
- `result_count: u32` - Results returned
- `timestamp: u64` - Query timestamp

**Use Case:** API analytics, usage tracking

---

### AnalyticsExported (`anal_exp`)
Emitted when analytics data is exported.

**Data Fields:**
- `export_type: String` - Export format (CSV, JSON, etc.)
- `requested_by: Address` - User requesting export
- `record_count: u32` - Records included
- `timestamp: u64` - Export timestamp

**Use Case:** Audit access, data governance

---

## Protocol Management Events

### ProtocolInitialized (`proto_in`)
Emitted during initial protocol setup (one-time event).

**Data Fields:**
- `admin: Address` - Initial admin address
- `treasury: Address` - Treasury address
- `fee_bps: u32` - Initial platform fee rate
- `min_invoice_amount: i128` - Minimum invoice amount allowed
- `max_due_date_days: u64` - Maximum due date window
- `grace_period_seconds: u64` - Default grace period
- `timestamp: u64` - Initialization timestamp

**Use Case:** Protocol startup verification, configuration audit

---

### ProtocolConfigUpdated (`proto_cfg`)
Emitted when protocol configuration is updated.

**Data Fields:**
- `admin: Address` - Admin making change
- `min_invoice_amount: i128` - Updated minimum amount
- `max_due_date_days: u64` - Updated max due date
- `grace_period_seconds: u64` - Updated grace period
- `timestamp: u64` - Update timestamp

**Use Case:** Configuration audit, user notifications

---

### AdminSet (`adm_set`)
Emitted when admin address is initially set.

**Data Fields:**
- `admin: Address` - Newly set admin
- `timestamp: u64` - Set timestamp

**Use Case:** Permission audit, startup tracking

---

### AdminTransferred (`adm_trf`)
Emitted when admin role is transferred to new address.

**Data Fields:**
- `old_admin: Address` - Previous admin
- `new_admin: Address` - New admin
- `timestamp: u64` - Transfer timestamp

**Use Case:** Access control audit, permission tracking

---

## Event Usage Guidelines

### For Off-Chain Indexers

1. **Listen** to all event topics listed above
2. **Parse** event data according to schema specifications
3. **Index** by primary identifiers:
   - `invoice_id` for Invoice events
   - `bid_id` for Bid events
   - `investor` address for investor-related events
   - `business` address for business-related events
4. **Store** with full context for audit trails
5. **Query** by indexed fields for efficient lookups

### For Frontend Applications

1. **Subscribe** to relevant event topics for your use case
2. **Update** UI state upon event reception
3. **Display** event data transparently to users
4. **Track** event sequences for audit trails
5. **Notify** users of important state changes

### For Compliance & Auditing

1. **Archive** all events for statutory retention periods
2. **Verify** event immutability through blockchain
3. **Trace** authorization context from events
4. **Reconcile** financial transactions with payment events
5. **Generate** audit reports from event history

---

## Security Considerations

### Event Integrity
- Events are blockchain-recorded and immutable
- Cannot be modified or deleted once emitted
- Provides authoritative audit trail

### Authorization Context
- All events implicitly contain authorization context
- Business operations require business authorization
- Admin operations require admin authorization
- Investment operations require investor authorization

### Financial Accuracy
- All financial events include complete amount breakdowns
- Fee events enable transparent reconciliation
- Payment events support cash flow audits

### Timestamp Reliability
- All timestamps are server-generated (not user input)
- Chronological ordering guaranteed by blockchain
- Enables time-based analytics and SLA tracking

---

## Performance & Scalability

### Event Emission
- Events are emitted synchronously after state commitment
- No performance impact on contract execution
- Gas-efficient via symbol_short topics

### Indexing Strategy
- Use event topics as primary filters
- Combine with invoice_id/address for secondary filtering
- Archive old events for historical analysis

### Query Optimization
- Index by frequently-queried fields
- Use time-range filters for historical data
- Implement pagination for large result sets

---

