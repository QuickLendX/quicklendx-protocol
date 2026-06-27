# QuickLendX Query Entrypoints

This guide catalogs common read-only QuickLendX contract entrypoints for
downstream integrators. It focuses on calls exposed through
`quicklendx-contracts/src/lib.rs`, which is the main Soroban contract surface.

Use these queries from dashboards, indexers, SDKs, monitoring jobs, and support
tools when no state mutation is required.

## Conventions

Examples use Stellar CLI placeholders:

| Placeholder | Meaning |
| --- | --- |
| `$CONTRACT_ID` | Deployed QuickLendX contract ID. |
| `$NETWORK` | Network name such as `testnet`. |
| `$SOURCE` | Caller account used by the CLI. Read calls do not require protocol authorization. |
| `$INVOICE_ID` | `BytesN<32>` invoice identifier. |
| `$BID_ID` | `BytesN<32>` bid identifier. |
| `$ADDRESS` | Soroban address for a business, investor, admin, or user. |

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network "$NETWORK" \
  --source "$SOURCE" \
  -- get_total_invoice_count
```

## Pagination Rules

Paginated query entrypoints use `offset` and `limit` arguments. The contract
caps result size with `MAX_QUERY_LIMIT = 100`, so requests above 100 return at
most 100 records.

| Input | Behavior |
| --- | --- |
| `limit = 0` | Returns an empty vector. |
| `limit > 100` | Caps to 100 records. |
| `offset` beyond the collection | Returns an empty vector. |
| Missing record in a collection query | Usually returns `None` or an empty vector. |
| Missing record in a direct lookup | Usually returns a typed `QuickLendXError`. |

## Protocol And Admin State

| Entrypoint | Arguments | Returns | Use case |
| --- | --- | --- | --- |
| `is_initialized` | none | `bool` | Check whether protocol initialization has completed. |
| `get_version` | none | `u32` | Read the deployed protocol version. |
| `get_current_admin` | none | `Option<Address>` | Display the current admin address when configured. |
| `get_admin` | none | `Option<Address>` | Compatibility admin lookup. |
| `get_protocol_limits` | none | `ProtocolLimits` | Read invoice, bid, and query limit configuration. |
| `preview_protocol_config` | proposed config fields | config diff/result | Show the impact of an admin config change without writing state. |
| `get_fee_bps` | none | `u32` | Read the legacy/platform fee basis points. |
| `get_treasury` | none | `Option<Address>` | Read the configured treasury address. |
| `get_min_invoice_amount` | none | `i128` | Display the minimum allowed invoice amount. |
| `get_max_due_date_days` | none | `u64` | Display the due-date horizon. |
| `get_grace_period_seconds` | none | `u64` | Display the default grace period used for overdue/default flows. |

Example:

```bash
stellar contract invoke --id "$CONTRACT_ID" --network "$NETWORK" --source "$SOURCE" -- get_protocol_limits
```

## Health, Pause, And Operations

| Entrypoint | Arguments | Returns | Use case |
| --- | --- | --- | --- |
| `is_paused` | none | `bool` | Decide whether write flows should be hidden or disabled. |
| `is_entrypoint_paused` | `entrypoint: String` | `bool` | Check a specific write entrypoint pause override. |
| `is_maintenance_mode` | none | `bool` | Show maintenance banners or block mutating UI. |
| `get_maintenance_reason` | none | `Option<String>` | Explain maintenance mode to operators or users. |
| `get_health_status` | none | `HealthStatus` | Read contract health monitor status. |
| `get_protocol_health` | none | `ProtocolHealth` | Read detailed protocol health state. |
| `get_protocol_diagnostics` | none | `ProtocolDiagnostics` | Power operator dashboards and diagnostics views. |
| `get_operational_limits` | none | `OperationalLimits` | Display operational guardrail values. |

Example:

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network "$NETWORK" \
  --source "$SOURCE" \
  -- is_entrypoint_paused \
  --entrypoint '"accept_bid"'
```

## Currency Whitelist

| Entrypoint | Arguments | Returns | Use case |
| --- | --- | --- | --- |
| `is_allowed_currency` | `currency: Address` | `bool` | Validate a token before invoice creation or bidding UX. |
| `get_whitelisted_currencies` | none | `Vec<Address>` | Display every currently allowed token. |
| `get_whitelisted_currencies_paged` | `offset: u32`, `limit: u32` | `Vec<Address>` | Page through the whitelist with the 100-record cap. |

Example:

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network "$NETWORK" \
  --source "$SOURCE" \
  -- get_whitelisted_currencies_paged \
  --offset 0 \
  --limit 100
```

## Invoices

| Entrypoint | Arguments | Returns | Use case |
| --- | --- | --- | --- |
| `get_invoice` | `invoice_id: BytesN<32>` | `Result<Invoice, QuickLendXError>` | Fetch the full invoice record. Missing IDs return `InvoiceNotFound`. |
| `get_invoice_by_business` | `business: Address` | `Vec<BytesN<32>>` | Fetch invoice IDs for a business. |
| `get_business_invoices` | `business: Address` | `Vec<BytesN<32>>` | Compatibility alias for business invoice IDs. |
| `get_business_invoices_paged` | `business`, `status_filter`, `offset`, `limit` | `Vec<BytesN<32>>` | Page a business invoice list with an optional status filter. |
| `get_available_invoices` | none | `Vec<BytesN<32>>` | Fetch currently fundable invoice IDs. |
| `get_available_invoices_paged` | `min_amount`, `max_amount`, `category_filter`, `offset`, `limit` | `Vec<BytesN<32>>` | Page available invoices for marketplace views. |
| `get_invoices_by_status` | `status: InvoiceStatus` | `Vec<BytesN<32>>` | Build status-specific queues. |
| `get_invoice_count_by_status` | `status: InvoiceStatus` | `u32` | Show queue counts without loading all records. |
| `get_total_invoice_count` | none | `u32` | Display protocol-wide invoice count. |
| `get_invoices_by_customer` | `customer_name: String` | `Vec<BytesN<32>>` | Search invoices by customer name index. |
| `get_invoices_by_tax_id` | `tax_id: String` | `Vec<BytesN<32>>` | Search invoices by tax ID index. |
| `get_invoices_by_status_batch` | `ids: Vec<BytesN<32>>` | `Vec<Option<InvoiceStatus>>` | Resolve statuses for a known list of invoice IDs. |
| `get_invoices_by_category` | `category: InvoiceCategory` | `Vec<BytesN<32>>` | Build category pages. |
| `get_invoices_by_cat_status` | `category`, `status` | `Vec<BytesN<32>>` | Build category plus status pages. |
| `get_invoices_by_tag` | `tag: String` | `Vec<BytesN<32>>` | Search by a single normalized tag. |
| `get_invoices_by_tags` | `tags: Vec<String>` | `Vec<BytesN<32>>` | Search by multiple tags. |
| `get_invoice_count_by_category` | `category: InvoiceCategory` | `u32` | Show category counts. |
| `get_invoice_count_by_tag` | `tag: String` | `u32` | Show tag counts. |
| `get_invoice_tags` | `invoice_id: BytesN<32>` | tag vector/result | Display tags for one invoice. |

Example:

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network "$NETWORK" \
  --source "$SOURCE" \
  -- get_invoice \
  --invoice_id "$INVOICE_ID"
```

## Bids

| Entrypoint | Arguments | Returns | Use case |
| --- | --- | --- | --- |
| `get_bid` | `bid_id: BytesN<32>` | `Option<Bid>` | Fetch one bid. Missing IDs return `None`. |
| `get_bids_for_invoice` | `invoice_id: BytesN<32>` | `Vec<Bid>` | Show every bid for an invoice. |
| `get_ranked_bids` | `invoice_id: BytesN<32>` | `Vec<Bid>` | Display bids in contract-defined ranking order. |
| `get_best_bid` | `invoice_id: BytesN<32>` | `Option<Bid>` | Highlight the current best bid. |
| `get_bids_by_status` | `invoice_id`, `status` | `Vec<Bid>` | Filter invoice bids by bid status. |
| `get_bids_by_investor` | `invoice_id`, `investor` | `Vec<Bid>` | Filter invoice bids by investor. |
| `get_all_bids_by_investor` | `investor: Address` | `Vec<Bid>` | Show a user's global bid history. |
| `get_bid_history` | `invoice_id: BytesN<32>` | `Vec<Bid>` | Compatibility history lookup for invoice bids. |
| `get_bid_history_paged` | `invoice_id`, `status_filter`, `offset`, `limit` | `Vec<Bid>` | Page bid history with optional status filter. |
| `get_investor_bids_paged` | `investor`, `status_filter`, `offset`, `limit` | `Vec<Bid>` | Page a user's bids with optional status filter. |
| `get_bid_ttl_days` | none | `u64` | Display current bid expiration TTL. |
| `get_bid_ttl_config` | none | `BidTtlConfig` | Display full bid TTL configuration. |
| `get_max_active_bids_per_investor` | none | `u32` | Display active bid cap. |
| `get_bid_limit_config` | none | `BidLimitConfig` | Display bid limit configuration. |

Example:

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network "$NETWORK" \
  --source "$SOURCE" \
  -- get_bid_history_paged \
  --invoice_id "$INVOICE_ID" \
  --status_filter none \
  --offset 0 \
  --limit 25
```

## Investments And Portfolio

| Entrypoint | Arguments | Returns | Use case |
| --- | --- | --- | --- |
| `get_investment` | `investment_id: BytesN<32>` | `Result<Investment, QuickLendXError>` | Fetch one investment. |
| `get_invoice_investment` | `invoice_id: BytesN<32>` | `Result<Investment, QuickLendXError>` | Resolve the investment backing an invoice. |
| `get_active_investment_ids` | none | `Vec<BytesN<32>>` | Display active investment IDs. |
| `get_investments_by_investor` | `investor: Address` | `Vec<BytesN<32>>` | Fetch investment IDs for a user. |
| `get_investor_investments_paged` | `investor`, `status_filter`, `offset`, `limit` | `Vec<BytesN<32>>` | Page investor investments. |
| `query_investment_insurance` | investment identifiers | insurance result | Inspect insurance coverage for an investment. |
| `get_investor_portfolio_summary` | `investor: Address` | portfolio summary | Power investor dashboard totals. |
| `get_address_summary` | `address: Address` | address summary | Summarize protocol activity for one address. |
| `calculate_profit` | `investment_amount`, `payment_amount` | profit tuple/result | Preview investor return and platform fee. |

Example:

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network "$NETWORK" \
  --source "$SOURCE" \
  -- get_investor_investments_paged \
  --investor "$ADDRESS" \
  --status_filter none \
  --offset 0 \
  --limit 50
```

## Verification And KYC

| Entrypoint | Arguments | Returns | Use case |
| --- | --- | --- | --- |
| `get_verified_businesses` | none | `Vec<Address>` | List businesses approved for invoice upload. |
| `get_pending_businesses` | none | `Vec<Address>` | Build admin review queues. |
| `get_rejected_businesses` | none | `Vec<Address>` | Build rejected-business review views. |
| `get_business_verification_status` | `business: Address` | status/result | Display one business KYC state. |
| `get_investor_verification` | `investor: Address` | `Option<InvestorVerification>` | Display one investor KYC state. |
| `get_verified_investors` | none | `Vec<Address>` | List approved investors. |
| `get_pending_investors` | none | `Vec<Address>` | Build investor KYC review queues. |
| `get_rejected_investors` | none | `Vec<Address>` | Build rejected-investor views. |
| `get_investors_by_tier` | `tier: InvestorTier` | `Vec<Address>` | Segment investors by tier. |
| `get_investors_by_risk_level` | `risk_level: InvestorRiskLevel` | `Vec<Address>` | Segment investors by risk. |
| `is_investor_verified` | `investor: Address` | `bool` | Gate investor-only UI. |
| `get_investor_analytics` | investor arguments | analytics result | Display investor risk and limit analytics. |
| `calculate_investor_risk_score` | risk inputs | score/result | Preview investor risk score. |
| `calculate_investment_limit` | risk/tier inputs | limit/result | Preview investment limit. |

Example:

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network "$NETWORK" \
  --source "$SOURCE" \
  -- is_investor_verified \
  --investor "$ADDRESS"
```

## Escrow, Settlement, And Defaults

| Entrypoint | Arguments | Returns | Use case |
| --- | --- | --- | --- |
| `get_escrow_details` | `invoice_id: BytesN<32>` | `Result<Escrow, QuickLendXError>` | Display escrow state for an invoice. |
| `get_escrow_status` | `invoice_id: BytesN<32>` | `Result<EscrowStatus, QuickLendXError>` | Display compact escrow status. |
| `get_overdue_scan_cursor` | none | `u32` | Show overdue scan progress. |
| `get_overdue_scan_batch_limit` | none | `u32` | Show current overdue scan batch size. |
| `get_overdue_scan_batch_limit_max` | none | `u32` | Show max allowed overdue scan batch size. |
| `get_invoice_dispute_status` | `invoice_id: BytesN<32>` | dispute status/result | Display dispute/default interaction status. |

Example:

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network "$NETWORK" \
  --source "$SOURCE" \
  -- get_escrow_status \
  --invoice_id "$INVOICE_ID"
```

## Fees And Revenue

| Entrypoint | Arguments | Returns | Use case |
| --- | --- | --- | --- |
| `get_platform_fee_config` | none | `Result<PlatformFeeConfig, QuickLendXError>` | Display fee configuration. |
| `get_treasury_address` | none | `Option<Address>` | Display current treasury recipient. |
| `get_fee_structure` | transaction context | fee structure/result | Preview fee tier logic. |
| `calculate_transaction_fees` | amount/context | fee breakdown/result | Preview fees before submitting a transaction. |
| `get_user_volume_data` | `user: Address` | `UserVolumeData` | Show fee tier volume data for one user. |
| `get_revenue_split_config` | none | `Result<RevenueConfig, QuickLendXError>` | Display revenue split setup. |
| `get_fee_analytics` | `period: u64` | `Result<FeeAnalytics, QuickLendXError>` | Build fee analytics dashboards. |

Example:

```bash
stellar contract invoke --id "$CONTRACT_ID" --network "$NETWORK" --source "$SOURCE" -- get_treasury_address
```

## Audit, Disputes, Notifications, And Analytics

| Entrypoint | Arguments | Returns | Use case |
| --- | --- | --- | --- |
| `get_invoice_audit_trail` | `invoice_id: BytesN<32>` | `Vec<BytesN<32>>` | Fetch audit entry IDs for an invoice. |
| `get_audit_entry` | `audit_id: BytesN<32>` | `Option<AuditLogEntry>` | Fetch one audit log record. |
| `get_audit_entries_by_operation` | `operation`, pagination/filter args | `Vec<BytesN<32>>` | Search audit logs by operation. |
| `get_audit_entries_by_actor` | `actor: Address` | `Vec<BytesN<32>>` | Search audit logs by actor. |
| `query_audit_logs` | `filter`, `limit` | `Vec<AuditLogEntry>` | Run filtered audit queries. |
| `get_audit_stats` | none | `AuditStats` | Display audit subsystem counters. |
| `verify_audit_chain` | `invoice_id: BytesN<32>` | `bool` | Check invoice audit-chain integrity. |
| `first_audit_chain_divergence` | `invoice_id: BytesN<32>` | `Option<u32>` | Locate first audit-chain mismatch. |
| `get_dispute_details` | `invoice_id: BytesN<32>` | dispute result | Display dispute metadata. |
| `get_invoices_with_disputes` | none | `Vec<BytesN<32>>` | Build dispute queue. |
| `get_dispute_timeline` | `invoice_id: BytesN<32>` | timeline result | Display dispute timeline events. |
| `get_invoices_by_dispute_status` | `status` | `Vec<BytesN<32>>` | Filter dispute queue by status. |
| `get_notification` | `notification_id: BytesN<32>` | notification result | Fetch one notification. |
| `get_user_notifications` | `user: Address` | `Vec<BytesN<32>>` | Display user notification IDs. |
| `get_notification_preferences` | `user: Address` | preferences | Display notification preferences. |
| `get_user_notification_stats` | `user: Address` | stats | Display unread/read counts and delivery stats. |
| `get_platform_metrics` | none | `PlatformMetrics` | Display platform metrics. |
| `get_performance_metrics` | none | `PerformanceMetrics` | Display performance metrics. |
| `get_business_report` | report identifiers | `BusinessReport` | Fetch business analytics report. |
| `get_financial_metrics` | report/filter args | `FinancialMetrics` | Fetch financial analytics. |
| `get_investor_report` | report identifiers | `InvestorReport` | Fetch investor analytics report. |
| `get_analytics_summary` | summary args | summary | Display compact analytics summary. |
| `get_freshness` | feed/key args | freshness result | Check data freshness state. |

Example:

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network "$NETWORK" \
  --source "$SOURCE" \
  -- get_invoice_audit_trail \
  --invoice_id "$INVOICE_ID"
```

## Backup And Vesting Reads

| Entrypoint | Arguments | Returns | Use case |
| --- | --- | --- | --- |
| `get_backup_details` | `backup_id: BytesN<32>` | `Option<Backup>` | Inspect one backup record. |
| `get_backups` | none | `Vec<BytesN<32>>` | List backup IDs. |
| `validate_backup` | `backup_id: BytesN<32>` | `bool` | Check backup validity. |
| `get_backup_retention_policy` | none | `BackupRetentionPolicy` | Display retention policy. |
| `get_vesting_schedule` | `id: u64` | `Option<VestingSchedule>` | Fetch one vesting schedule. |
| `get_vesting_vested` | `id: u64` | `Option<i128>` | Read vested amount. |
| `get_vesting_releasable` | `id: u64` | `Option<i128>` | Read releasable amount. |
| `get_vesting_summary` | `user: Address` | `VestingSummary` | Summarize user vesting state. |

## Related Docs

- `docs/contracts/queries.md`: hard caps, missing-record behavior, and query resilience.
- `quicklendx-contracts/docs/contracts/queries.md`: investment pagination details.
- `docs/ERROR_CODES.md`: error codes returned by direct lookup failures.
