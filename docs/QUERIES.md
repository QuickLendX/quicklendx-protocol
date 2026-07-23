# Read Queries — Entrypoint Catalog

Audience: **contributors** — developers reading, modifying, or integrating with the QuickLendX smart contracts.

This document catalogs the most common read-only (query) entrypoints on the `QuickLendXContract` trait defined in [`quicklendx-contracts/src/lib.rs`](../quicklendx-contracts/src/lib.rs). Each entry shows the Rust signature, a concrete invocation example with realistic arguments, and the shape of the returned data.

Read queries never modify contract state. They are the primary way to inspect invoices, bids, investments, protocol configuration, and analytics from off-chain clients or other contracts.

---

## Table of Contents

- [Configuration & Protocol State](#configuration--protocol-state)
- [Invoice Queries](#invoice-queries)
- [Bid Queries](#bid-queries)
- [Investment & Portfolio Queries](#investment--portfolio-queries)
- [Escrow Queries](#escrow-queries)
- [KYC / Verification Queries](#kyc--verification-queries)
- [Dispute Queries](#dispute-queries)
- [Analytics & Reporting](#analytics--reporting)
- [Audit Queries](#audit-queries)
- [Notification Queries](#notification-queries)
- [Vesting Queries](#vesting-queries)
- [Pagination Conventions](#pagination-conventions)
- [Error Handling](#error-handling)

---

## Configuration & Protocol State

### `get_protocol_limits`

Returns the current `ProtocolLimits` struct — the tunable bounds that constrain invoice amounts, bid sizes, due-date windows, grace periods, and maximum invoices per business.

```rust
pub fn get_protocol_limits(env: Env) -> protocol_limits::ProtocolLimits
```

**Invocation example** (Soroban CLI / RPC):

```json
{
  "contract_id": "CCJZ5DGJ7Z3Q5KJ5KJ5KJ5KJ5KJ5KJ5KJ5KJ5KJ5",
  "function": "get_protocol_limits",
  "args": []
}
```

**Return value:**

```rust
ProtocolLimits {
    min_invoice_amount: 1_000_000,    // 1 token (6 decimals)
    min_bid_amount: 10,               // minimum allowable bid
    min_bid_bps: 100,                 // 1% minimum bid rate
    max_due_date_days: 90,
    grace_period_seconds: 2_592_000,  // 30 days
    max_invoices_per_business: 0,     // 0 = unlimited
}
```

**Zero-config edge case:** If the contract has never been initialized, this function still returns the hard-coded defaults defined in `protocol_limits.rs`.

### `is_initialized`

```rust
pub fn is_initialized(env: Env) -> bool
```

Returns `true` after `initialize` has been called. Before initialization, all mutating entrypoints return `NotInitialized`.

### `get_current_admin`

```rust
pub fn get_current_admin(env: Env) -> Option<Address>
```

Returns `Some(admin_address)` if an admin has been set, `None` otherwise. Use this to verify admin configuration before calling admin-guarded entrypoints.

### `get_platform_fee`

```rust
pub fn get_platform_fee(env: Env) -> types::PlatformFeeConfig
```

Return example:

```rust
PlatformFeeConfig {
    fee_bps: 50,                              // 0.5%
    treasury_address: Some(treasury_addr),
    updated_at: 1700000000,
    updated_by: admin_addr,
}
```

### `is_allowed_currency`

```rust
pub fn is_allowed_currency(env: Env, currency: Address) -> bool
```

**Query pattern:** Before calling `store_invoice` with a token address, check whether it is whitelisted. When the whitelist is empty, every address passes (allow-all mode).

### `get_whitelisted_currencies_paged`

```rust
pub fn get_whitelisted_currencies_paged(env: Env, offset: u32, limit: u32) -> Vec<Address>
```

Returns a page of whitelisted token addresses. For empty lists or out-of-range offsets, returns an empty vector.

### Pause / Maintenance

```rust
pub fn is_paused(env: Env) -> bool
pub fn is_entrypoint_paused(env: Env, entrypoint: String) -> bool
pub fn is_maintenance_mode(env: Env) -> bool
pub fn get_maintenance_reason(env: Env) -> Option<String>
```

Check these before dispatching mutating calls; paused entrypoints will reject with `ContractPaused`.

---

## Invoice Queries

### `get_invoice`

The primary read entrypoint for individual invoice data.

```rust
pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<Invoice, QuickLendXError>
```

**Invocation example:**

```json
{
  "function": "get_invoice",
  "args": [
    { "type": "bytes", "value": "0xabc123def456abc123def456abc123def456abc123def456abc123def456abc1" }
  ]
}
```

**Return value (success):**

```rust
Invoice {
    id: 0xabc1…,                              // BytesN<32>
    business: business_addr,
    amount: 50_000_000_000,                    // 50,000 USDC (6 decimals)
    currency: usdc_token_addr,
    due_date: 1704067200,                      // Unix seconds
    status: Verified,
    created_at: 1701388800,
    description: String("Q4 marketing campaign invoice"),
    metadata_customer_name: Some("Acme Corp"),
    metadata_customer_address: None,
    metadata_tax_id: Some("12-3456789"),
    metadata_notes: Some("Net-30 payment terms"),
    metadata_line_items: Vec(…),
    category: Services,
    tags: Vec(["urgent", "quarterly"]),
    funded_amount: 0,
    funded_at: None,
    investor: None,
    settled_at: None,
    average_rating: None,
    total_ratings: 0,
    ratings: Vec(…),
    dispute_status: None,
    dispute: Dispute { … },
    total_paid: 0,
    payment_history: Vec(…),
}
```

**Error cases:**

| Condition | Error |
|---|---|
| `invoice_id` not found | `Err(QuickLendXError::InvoiceNotFound)` |
| Contract used before initialization | `Err(QuickLendXError::NotInitialized)` |

### `get_business_invoices_paged`

```rust
pub fn get_business_invoices_paged(
    env: Env,
    business: Address,
    status_filter: Option<InvoiceStatus>,
    offset: u32,
    limit: u32,
) -> Vec<BytesN<32>>
```

Lists invoice IDs for a given business, optionally filtered by status, with cursor-based pagination.

**Example — first 10 active invoices:**

```rust
let ids = contract.get_business_invoices_paged(
    env.clone(),
    business_addr,
    Some(InvoiceStatus::Verified),  // filter
    0,                              // offset
    10,                             // limit
);
```

**Edge cases:**
- `offset >= total_count` → empty `Vec`
- `limit == 0` → empty `Vec`
- `limit > MAX_QUERY_LIMIT (100)` → capped to `MAX_QUERY_LIMIT`

### `get_available_invoices_paged`

```rust
pub fn get_available_invoices_paged(
    env: Env,
    min_amount: Option<i128>,
    max_amount: Option<i128>,
    category_filter: Option<InvoiceCategory>,
    offset: u32,
    limit: u32,
) -> Vec<BytesN<32>>
```

The marketplace view — invoices available for bidding, with optional amount range and category filters.

### `search_invoices`

```rust
pub fn search_invoices(env: Env, query: String) -> Result<Vec<SearchResult>, QuickLendXError>
```

Full-text search across invoice descriptions, customer names, and tax IDs. Returns `SearchResult` entries ranked by relevance (`ExactId > PartialMatch > Other`).

### `get_total_invoice_count` / `get_invoice_count_by_status`

```rust
pub fn get_total_invoice_count(env: Env) -> u32
pub fn get_invoice_count_by_status(env: Env, status: InvoiceStatus) -> u32
```

Lightweight counters — useful for building dashboard summary cards without fetching full invoice data.

### `get_category_breakdown`

```rust
pub fn get_category_breakdown(env: Env) -> analytics::CategoryBreakdown
```

Returns counts and total amounts per `InvoiceCategory`. Example caller: a marketplace overview page.

---

## Bid Queries

### `get_bid`

```rust
pub fn get_bid(env: Env, bid_id: BytesN<32>) -> Option<Bid>
```

Returns `None` (not `Err`) when the bid does not exist — a missing bid is not an error.

**Return value:**

```rust
Some(Bid {
    bid_id: 0xdef456…,
    invoice_id: 0xabc123…,
    investor: investor_addr,
    bid_amount: 45_000_000_000,        // 45,000 USDC
    expected_return: 46_125_000_000,   // 2.5% return
    timestamp: 1701475200,
    status: Placed,
    expiration_timestamp: 1704067200,
})
```

### `get_best_bid`

```rust
pub fn get_best_bid(env: Env, invoice_id: BytesN<32>) -> Option<Bid>
```

Returns the highest-ranked bid for an invoice under the protocol's bid-ranking algorithm (lowest expected return wins). Returns `None` if no bids exist.

### `get_ranked_bids`

```rust
pub fn get_ranked_bids(env: Env, invoice_id: BytesN<32>) -> Vec<Bid>
```

Full bid list sorted by rank. Useful for investor-facing bid comparison views.

### `get_investor_bids_paged`

```rust
pub fn get_investor_bids_paged(
    env: Env,
    investor: Address,
    status_filter: Option<BidStatus>,
    offset: u32,
    limit: u32,
) -> Vec<Bid>
```

History of all bids placed by a specific investor, with optional status filter (`Placed`, `Accepted`, `Withdrawn`, `Expired`, `Cancelled`).

---

## Investment & Portfolio Queries

### `get_investment`

```rust
pub fn get_investment(env: Env, investment_id: BytesN<32>) -> Result<Investment, QuickLendXError>
```

**Return value:**

```rust
Investment {
    investment_id: 0x…,
    invoice_id: 0x…,
    investor: investor_addr,
    amount: 45_000_000_000,
    funded_at: 1701561600,
    status: Active,
    insurance: Vec([InsuranceCoverage {
        provider: insurance_provider_addr,
        coverage_percentage: 80,
        coverage_amount: 36_000_000_000,
        premium_amount: 450_000_000,
        active: true,
    }]),
}
```

### `get_investor_portfolio_summary`

```rust
pub fn get_investor_portfolio_summary(
    env: Env,
    investor: Address,
) -> Result<investment_queries::InvestorPortfolioSummary, QuickLendXError>
```

Aggregated view — total active, completed, and defaulted investment amounts plus weighted-average return.

### `get_investor_investments_paged`

```rust
pub fn get_investor_investments_paged(
    env: Env,
    investor: Address,
    status_filter: Option<InvestmentStatus>,
    offset: u32,
    limit: u32,
) -> Vec<BytesN<32>>
```

Paginated list of investment IDs for a given investor. See [Pagination Conventions](#pagination-conventions) for boundary behaviour.

### `get_active_investment_ids`

```rust
pub fn get_active_investment_ids(env: Env) -> Vec<BytesN<32>>
```

Returns the full list of currently active investment IDs. For large datasets prefer paged queries.

---

## Escrow Queries

### `get_escrow_details`

```rust
pub fn get_escrow_details(env: Env, invoice_id: BytesN<32>) -> Result<payments::Escrow, QuickLendXError>
```

**Return value:**

```rust
Escrow {
    escrow_id: 0x…,
    invoice_id: 0x…,
    investor: investor_addr,
    business: business_addr,
    amount: 45_000_000_000,
    currency: usdc_token_addr,
    created_at: 1701561600,
    status: Held,  // | Released | Refunded
}
```

### `get_escrow_status`

```rust
pub fn get_escrow_status(env: Env, invoice_id: BytesN<32>) -> Result<payments::EscrowStatus, QuickLendXError>
```

Lighter-weight than `get_escrow_details` when only the status is needed. Returns `Held`, `Released`, or `Refunded`.

---

## KYC / Verification Queries

### `get_business_verification_status`

```rust
pub fn get_business_verification_status(
    env: Env,
    business: Address,
) -> Option<verification::BusinessVerification>
```

Returns `None` if the address has never submitted a KYC application. When `Some`, the struct contains submission timestamp, reviewer notes, and current status (Pending / Approved / Rejected).

### `get_investor_verification`

```rust
pub fn get_investor_verification(
    env: Env,
    investor: Address,
) -> Option<InvestorVerification>
```

Same pattern as business verification but scoped to investor data (tier, risk score, investment limit).

### `is_investor_verified`

```rust
pub fn is_investor_verified(env: Env, investor: Address) -> bool
```

Quick boolean check — returns `true` only when the investor has an approved verification with an active status.

### `get_verified_businesses` / `get_pending_businesses` / `get_rejected_businesses`

```rust
pub fn get_verified_businesses(env: Env) -> Vec<Address>
pub fn get_pending_businesses(env: Env) -> Vec<Address>
pub fn get_rejected_businesses(env: Env) -> Vec<Address>
```

Admin-facing lists for the verification dashboard. Each returns full address lists (not paginated; expected to be moderate size).

### `get_investors_by_tier` / `get_investors_by_risk_level`

```rust
pub fn get_investors_by_tier(env: Env, tier: InvestorTier) -> Vec<Address>
pub fn get_investors_by_risk_level(env: Env, risk_level: InvestorRiskLevel) -> Vec<Address>
```

Filtered investor lists. Used by the admin dashboard for cohort analysis.

### `calculate_investor_risk_score`

```rust
pub fn calculate_investor_risk_score(
    env: Env,
    investor: Address,
    kyc_data: String,
) -> Result<u32, QuickLendXError>
```

Pure computation — does not write state. The risk score is derived from KYC data and on-chain behaviour metrics. Used by the front-end to preview what tier an applicant would land in before submitting KYC.

---

## Dispute Queries

### `get_invoice_dispute_status`

```rust
pub fn get_invoice_dispute_status(
    env: Env,
    invoice_id: BytesN<32>,
) -> Result<DisputeStatus, QuickLendXError>
```

Returns the current dispute lifecycle phase: `None`, `Disputed`, `UnderReview`, or `Resolved`. Returns `InvoiceNotFound` if the invoice does not exist.

### `get_dispute_details`

```rust
pub fn get_dispute_details(
    env: Env,
    invoice_id: BytesN<32>,
) -> Result<Option<Dispute>, QuickLendXError>
```

Full dispute record including evidence, resolution outcome, and timestamps. Returns `Ok(None)` when no dispute has ever been opened.

### `get_dispute_timeline`

```rust
pub fn get_dispute_timeline(
    env: Env,
    invoice_id: BytesN<32>,
    offset: u32,
    limit: u32,
) -> Result<dispute_timeline::DisputeTimeline, QuickLendXError>
```

Paginated chronological events for a dispute (opened, evidence added, escalated, resolved). Timeline entries include the actor address and a block timestamp.

### `get_invoices_by_dispute_status`

```rust
pub fn get_invoices_by_dispute_status(
    env: Env,
    dispute_status: DisputeStatus,
) -> Vec<BytesN<32>>
```

Used by admin dashboards to surface invoices that need attention.

---

## Analytics & Reporting

### `get_platform_metrics`

```rust
pub fn get_platform_metrics(env: Env) -> analytics::PlatformMetrics
```

Platform-wide aggregated data: total volume, total fees collected, active invoice count, active investor count.

### `get_performance_metrics`

```rust
pub fn get_performance_metrics(env: Env) -> analytics::PerformanceMetrics
```

Throughput and health indicators: average time-to-fund, average time-to-payment, default rate, and dispute rate.

### `get_analytics_summary`

```rust
pub fn get_analytics_summary(env: Env)
    -> (analytics::PlatformMetrics, analytics::PerformanceMetrics)
```

One-shot combination of the two metric types above — saves an RPC round trip when building a dashboard overview.

### `get_financial_metrics`

```rust
pub fn get_financial_metrics(
    env: Env,
    period: analytics::TimePeriod,
) -> Result<analytics::FinancialMetrics, QuickLendXError>
```

Time-windowed financial data (volume, fees, interest earned). Supported periods: `Daily`, `Weekly`, `Monthly`, `Quarterly`, `Yearly`, `AllTime`.

### `get_user_behavior_metrics`

```rust
pub fn get_user_behavior_metrics(
    env: Env,
    user: Address,
) -> analytics::UserBehaviorMetrics
```

Per-user engagement data: total actions, last active timestamp, action frequency.

### `get_address_summary`

```rust
pub fn get_address_summary(
    env: Env,
    addr: Address,
) -> Result<address_summary::AddressSummary, QuickLendXError>
```

Unified view that aggregates invoices, bids, investments, and verification status for any address. The returned enum differentiates between business and investor roles. Returns `AddressNotFound` when the address has no on-chain activity.

---

## Audit Queries

### `get_invoice_audit_trail`

```rust
pub fn get_invoice_audit_trail(
    env: Env,
    invoice_id: BytesN<32>,
) -> Vec<BytesN<32>>
```

Returns audit entry IDs for the invoice, ordered by creation time. Each ID can be dereferenced with `get_audit_entry`.

### `get_audit_entry`

```rust
pub fn get_audit_entry(
    env: Env,
    audit_id: BytesN<32>,
) -> Option<audit::AuditLogEntry>
```

Single audit event: operation type, actor, affected entity, timestamp, and a cryptographic link (hash-chain) to the previous entry for integrity verification.

### `query_audit_logs`

```rust
pub fn query_audit_logs(
    env: Env,
    filter: audit::AuditQueryFilter,
    limit: u32,
) -> Vec<audit::AuditLogEntry>
```

Advanced search across audit logs. The `AuditQueryFilter` supports filtering by operation type, actor address, and time range.

### `verify_audit_chain` / `validate_invoice_audit_integrity`

```rust
pub fn verify_audit_chain(env: Env, invoice_id: BytesN<32>) -> bool
pub fn validate_invoice_audit_integrity(
    env: Env,
    invoice_id: BytesN<32>,
) -> Result<bool, QuickLendXError>
```

Integrity checks that verify the hash chain has not been tampered with. Returns `false` / `Err` if a gap or hash mismatch is detected.

---

## Notification Queries

### `get_user_notifications`

```rust
pub fn get_user_notifications(
    env: Env,
    user: Address,
) -> Vec<BytesN<32>>
```

Returns notification IDs for a user, newest first. Dereference each ID with `get_notification`.

### `get_notification`

```rust
pub fn get_notification(
    env: Env,
    notification_id: BytesN<32>,
) -> Option<notifications::Notification>
```

Single notification: type (e.g. `BidAccepted`, `InvoicePaid`), title, body, read status, and timestamp.

### `get_user_notification_stats`

```rust
pub fn get_user_notification_stats(
    env: Env,
    user: Address,
) -> notifications::NotificationStats
```

Unread count and total count — useful for badge display in a UI header.

---

## Vesting Queries

### `get_vesting_schedule`

```rust
pub fn get_vesting_schedule(env: Env, id: u64) -> Option<vesting::VestingSchedule>
```

Returns the schedule configuration: total amount, cliff duration, vesting duration, start time, and beneficiary.

### `get_vesting_vested`

```rust
pub fn get_vesting_vested(env: Env, id: u64) -> Option<i128>
```

Amount currently vested (not yet released). Returns `None` if the schedule ID does not exist.

### `get_vesting_releasable`

```rust
pub fn get_vesting_releasable(env: Env, id: u64) -> Option<i128>
```

Amount that the beneficiary can release right now (vested minus already released).

### `get_vesting_summary`

```rust
pub fn get_vesting_summary(env: Env, user: Address) -> vesting::VestingSummary
```

Aggregate across all schedules for a user: total vested, total released, total remaining, next release time.

---

## Pagination Conventions

All paginated endpoints (`*_paged`) follow the same conventions:

| Rule | Behaviour |
|---|---|
| Max page size | `MAX_QUERY_LIMIT = 100` — larger values are silently capped |
| Zero limit | Returns empty `Vec` |
| Offset at or past end | Returns empty `Vec` |
| Offset == 0 | First page |
| Overflow safety | All internal arithmetic uses saturating operations |
| Filtering order | Status filters are applied before pagination slicing |

Example — safe iteration over all invoices for a business:

```rust
let mut offset = 0u32;
let limit = 100u32;
loop {
    let page = contract.get_business_invoices_paged(
        env.clone(),
        business_addr,
        None,
        offset,
        limit,
    );
    if page.is_empty() {
        break;
    }
    // process page
    offset = offset.saturating_add(limit);
}
```

---

## Error Handling

Read queries return errors in two patterns:

| Pattern | Convention | Examples |
|---|---|---|
| `Result<T, QuickLendXError>` | Missing resource returns `Err` | `get_invoice`, `get_investment`, `get_escrow_details` |
| `Option<T>` | Missing resource returns `None` | `get_bid`, `get_vesting_schedule`, `get_notification` |

The `Option` pattern is used for entity types that are expected to be missing in normal operation (e.g. "no bid yet for this invoice"). The `Result` pattern is used when a missing resource indicates a programmer error or invalid state.

All error variants are documented in [`docs/contracts/errors.md`](contracts/errors.md).

---

## Related Documentation

- [Full contract API — `lib.rs`](../quicklendx-contracts/src/lib.rs): Source code with inline rustdoc for every entrypoint.
- [`docs/contracts/queries.md`](contracts/queries.md): Deep dive into the investment-query pagination implementation, overflow safety, and boundary testing.
- [`docs/CURRENCY_WHITELIST.md`](CURRENCY_WHITELIST.md): How the currency allow-list affects invoice and bid entrypoints.
- [`docs/INVESTOR_TIER.md`](INVESTOR_TIER.md): Risk score and tier computation used by the verification queries.
- [`docs/VESTING.md`](VESTING.md): Full vesting model with worked examples.
