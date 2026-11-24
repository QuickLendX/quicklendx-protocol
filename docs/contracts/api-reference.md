# Contracts API Reference

## 📖 API Documentation

### Deterministic Ledger Time

See [Deterministic Ledger Time](../../quicklendx-contracts/docs/contracts/deterministic-time.md) for guidelines on using `env.ledger().timestamp()` instead of off-chain wall-clock time in contract logic.

### Settlement formula and update timing

See [Settlement formula, inputs, and update timing](settlement-formula.md) for the contributor-facing explanation of the settlement formula and when fee updates apply.

### Settlement Currencies

See [Settlement Currencies by Invoice Type](../../quicklendx-contracts/docs/SETTLEMENT_CURRENCIES.md) for a downstream integrator guide on which tokens are accepted for each invoice category.

### Contract entrypoints (current surface)

The deployed contract (`src/contract.rs`) currently exposes the protocol
initialization and admin slice and invoice creation and lifecycle. The
remaining modules (bids, escrow, settlement, disputes, KYC, fees, pause,
backup) are wired in by their own issues; the functions documented under *Core Functions* below describe those
modules and are not callable on-chain until they are wired.

Every entrypoint is a thin wrapper; validation, auth and events live in the
module function it delegates to.

| Entrypoint | Delegates to | Auth |
|---|---|---|
| `initialize(params: InitializationParams)` | `ProtocolInitializer::initialize` | `params.admin` (once) |
| `is_initialized()` | `ProtocolInitializer::is_initialized` | — |
| `get_version()` | `ProtocolInitializer::get_version` | — |
| `get_protocol_config()` | `ProtocolInitializer::get_protocol_config` | — |
| `preview_protocol_config(admin, params: ProtocolConfigParams)` | `ProtocolInitializer::preview_protocol_config` | admin (read-only) |
| `set_protocol_config(admin, min_invoice_amount, max_due_date_days, grace_period_seconds, backfill_max_batch_size)` | `ProtocolInitializer::set_protocol_config` | admin |
| `set_fee_config(admin, fee_bps)` | `ProtocolInitializer::set_fee_config` | admin |
| `set_treasury(admin, treasury)` | `ProtocolInitializer::set_treasury` | admin |
| `get_fee_bps()` | `ProtocolInitializer::get_fee_bps` | — |
| `get_treasury()` | `ProtocolInitializer::get_treasury` | — |
| `get_min_invoice_amount()` | `ProtocolInitializer::get_min_invoice_amount` | — |
| `get_max_due_date_days()` | `ProtocolInitializer::get_max_due_date_days` | — |
| `get_grace_period_seconds()` | `ProtocolInitializer::get_grace_period_seconds` | — |
| `get_corridors()` | `ProtocolInitializer::get_corridors` | — |
| `get_current_admin()` | `AdminStorage::get_admin` | — |
| `transfer_admin(current_admin, new_admin)` | `AdminStorage::transfer_admin` (records a pending transfer when two-step is on) | current admin |
| `initiate_admin_transfer(current_admin, pending_admin)` | `AdminStorage::initiate_admin_transfer` | current admin |
| `accept_admin_transfer(pending_admin)` | `AdminStorage::accept_admin_transfer` | pending admin |
| `cancel_admin_transfer(current_admin)` | `AdminStorage::cancel_admin_transfer` | current admin |
| `set_two_step_enabled(admin, enabled)` | `AdminStorage::set_two_step_enabled` | admin |
| `is_two_step_enabled()` | `AdminStorage::is_two_step_enabled` | — |
| `get_pending_admin()` | `AdminStorage::get_pending_admin` | — |
| `store_invoice(business, amount, currency, due_date, description, category, tags, origination_fee_bps, late_payment_penalty_bps, early_payment_discount_bps, nonce)` | `Invoice::new` + `InvoiceStorage::store_invoice` (see order of checks below) | business |
| `get_invoice(invoice_id)` | `InvoiceStorage::get` | — |
| `get_business_invoices(business)` | `InvoiceStorage::get_business_invoices` | — |
| `get_invoices_by_status(status)` | `InvoiceStorage::get_invoices_by_status` | — |
| `get_total_invoice_count()` | `InvoiceStorage::get_total_count` | — |
| `get_invoice_count_by_status(status)` | status index length | — |
| `get_invoices_by_customer(customer_name)` | `InvoiceStorage::get_by_customer` | — |
| `get_invoices_by_tax_id(tax_id)` | `InvoiceStorage::get_by_tax_id` | — |
| `get_invoices_by_status_batch(invoice_ids)` | status per ID, at most `MAX_INPUT_STATUS_BATCH_SIZE` IDs | — |
| `verify_invoice(admin, invoice_id)` | `Invoice::verify` | admin (`require_admin_auth`) |
| `update_invoice_metadata(business, invoice_id, metadata)` | `Invoice::update_metadata` | business (inside `Invoice`) |
| `clear_invoice_metadata(business, invoice_id)` | `Invoice::clear_metadata` | business (inside `Invoice`) |
| `add_invoice_tag(business, invoice_id, tag)` | `Invoice::add_tag` | business (wrapper, after ownership check) |
| `remove_invoice_tag(business, invoice_id, tag)` | `Invoice::remove_tag` | business (wrapper, after ownership check) |
| `update_invoice_category(business, invoice_id, category)` | `Invoice::update_category` | business (wrapper, after ownership check) |

`initialize` succeeds once. Repeating it with identical params returns `Ok(())`;
any other params return `OperationNotAllowed`. There is no standalone
`initialize_admin`: the admin is installed only by `initialize`. If a
deployment already has an admin from an older flow, only that admin can
complete `initialize`.

`store_invoice` checks, in order: the business signature; idempotency (the key
is `sha256("inv_create" || business || nonce)`, so a repeated
`(business, nonce)` returns the original ID and another business reusing the
nonce gets its own invoice); the per-address rate limit and input size bounds;
KYC, regulatory, currency whitelist and protocol limits (minimum amount, due
date horizon, invoices per business); `Invoice::new`; storage; the
`invoice_uploaded` event, the audit entry and the notification; and finally
the idempotency record. Invoice IDs come from `Invoice::new`, which mixes the
ledger timestamp, sequence and a monotonic counter, so invoices created in the
same ledger get distinct IDs.

### Core Functions

#### Invoice Management

##### `store_invoice`

Creates and stores a new invoice in the contract.

```rust
pub fn store_invoice(
    env: Env,
    business: Address,
    amount: i128,
    currency: Address,
    due_date: u64,
    description: String,
    category: InvoiceCategory,
    tags: Vec<String>,
) -> Result<BytesN<32>, QuickLendXError>
```

**Parameters:**

- `business`: Address of the business creating the invoice
- `amount`: Invoice amount in the token's smallest indivisible unit (e.g. 1 000 000 for 1.000000 USDC with 6 decimals, or 10 000 000 for 1 XLM with 7 decimals). The contract stores this value verbatim — **no decimal conversion is performed internally**. See [`docs/contracts/token-decimals.md`](token-decimals.md) for the full explanation and integration examples.
- `currency`: Token address for the invoice currency
- `due_date`: Unix timestamp for invoice due date
- `description`: Human-readable invoice description
- `category`: Invoice category (Services, Goods, etc.)
- `tags`: Array of tags for categorization

**Returns:** Invoice ID (32-byte hash)

**Example:**

```rust
let invoice_id = contract.store_invoice(
    &env,
    business_addr,
    10000, // $100.00
    usdc_addr,
    1735689600, // Jan 1, 2025
    String::from_str(&env, "Consulting services"),
    InvoiceCategory::Services,
    vec![&env, String::from_str(&env, "consulting")]
)?;
```

##### `get_invoice`

Retrieves invoice details by ID.

```rust
pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<Invoice, QuickLendXError>
```

##### `update_invoice_status`

Updates the status of an invoice.

```rust
pub fn update_invoice_status(
    env: Env,
    invoice_id: BytesN<32>,
    new_status: InvoiceStatus,
) -> Result<(), QuickLendXError>
```

#### Bidding System

##### `place_bid`

Places a bid on an available invoice.

```rust
pub fn place_bid(
    env: Env,
    investor: Address,
    invoice_id: BytesN<32>,
    bid_amount: i128,
    expected_return: i128,
) -> Result<BytesN<32>, QuickLendXError>
```

**Parameters:**

- `investor`: Address of the investor placing the bid
- `invoice_id`: ID of the invoice to bid on
- `bid_amount`: Amount the investor is willing to pay
- `expected_return`: Expected return amount

**Returns:** Bid ID

##### `accept_bid`

Accepts a bid on an invoice, creating an escrow.

```rust
pub fn accept_bid(
    env: Env,
    invoice_id: BytesN<32>,
    bid_id: BytesN<32>,
) -> Result<(), QuickLendXError>
```

#### Payment & Escrow

##### `release_escrow_funds`

Releases escrow funds to the investor upon invoice verification.

```rust
pub fn release_escrow_funds(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError>
```

##### `refund_escrow_funds`

Refunds escrow funds to the investor if conditions aren't met.

```rust
pub fn refund_escrow_funds(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError>
```

#### Verification & KYC

##### `submit_kyc_application`

Submits KYC application for business verification.

```rust
pub fn submit_kyc_application(
    env: Env,
    business: Address,
    kyc_data: String,
) -> Result<(), QuickLendXError>
```

##### `verify_business`

Verifies a business (admin only).

```rust
pub fn verify_business(
    env: Env,
    admin: Address,
    business: Address,
) -> Result<(), QuickLendXError>
```

#### Query Functions (Paginated)

##### `get_business_invoices_paged`

Retrieves invoices for a business with pagination and optional status filtering.

```rust
pub fn get_business_invoices_paged(
    env: Env,
    business: Address,
    status_filter: Option<InvoiceStatus>,
    offset: u32,
    limit: u32,
) -> Vec<BytesN<32>>
```

**Parameters:**

- `business`: Address of the business
- `status_filter`: Optional status filter (None returns all statuses)
- `offset`: Starting index for pagination (0-based)
- `limit`: Maximum number of results to return

**Returns:** Vector of invoice IDs

**Example:**

```rust
// Get first 10 verified invoices for a business
let invoices = contract.get_business_invoices_paged(
    &env,
    business_addr,
    Some(InvoiceStatus::Verified),
    0,  // offset
    10  // limit
);

// Get next 10 invoices
let more_invoices = contract.get_business_invoices_paged(
    &env,
    business_addr,
    Some(InvoiceStatus::Verified),
    10, // offset
    10  // limit
);
```

##### `get_investor_investments_paged`

Retrieves investments for an investor with pagination and optional status filtering.

```rust
pub fn get_investor_investments_paged(
    env: Env,
    investor: Address,
    status_filter: Option<InvestmentStatus>,
    offset: u32,
    limit: u32,
) -> Vec<BytesN<32>>
```

**Parameters:**

- `investor`: Address of the investor
- `status_filter`: Optional investment status filter
- `offset`: Starting index for pagination
- `limit`: Maximum number of results to return

**Returns:** Vector of investment IDs

**Example:**

```rust
// Get all active investments for an investor
let active_investments = contract.get_investor_investments_paged(
    &env,
    investor_addr,
    Some(InvestmentStatus::Active),
    0,
    50
);
```

##### `get_available_invoices_paged`

Retrieves available (verified) invoices with pagination and optional filters.

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

**Parameters:**

- `min_amount`: Optional minimum invoice amount filter
- `max_amount`: Optional maximum invoice amount filter
- `category_filter`: Optional category filter
- `offset`: Starting index for pagination
- `limit`: Maximum number of results to return

**Returns:** Vector of invoice IDs

**Example:**

```rust
// Get service invoices between $100 and $1000
let invoices = contract.get_available_invoices_paged(
    &env,
    Some(10000),  // $100.00 minimum
    Some(100000), // $1000.00 maximum
    Some(InvoiceCategory::Services),
    0,
    20
);
```

##### `get_bid_history_paged`

Retrieves bid history for an invoice with pagination and optional status filtering.

```rust
pub fn get_bid_history_paged(
    env: Env,
    invoice_id: BytesN<32>,
    status_filter: Option<BidStatus>,
    offset: u32,
    limit: u32,
) -> Vec<Bid>
```

**Parameters:**

- `invoice_id`: ID of the invoice
- `status_filter`: Optional bid status filter
- `offset`: Starting index for pagination
- `limit`: Maximum number of results to return

**Returns:** Vector of Bid objects

**Example:**

```rust
// Get all accepted bids for an invoice
let accepted_bids = contract.get_bid_history_paged(
    &env,
    invoice_id,
    Some(BidStatus::Accepted),
    0,
    10
);
```

##### `get_investor_bids_paged`

Retrieves bid history for an investor with pagination and optional status filtering.

```rust
pub fn get_investor_bids_paged(
    env: Env,
    investor: Address,
    status_filter: Option<BidStatus>,
    offset: u32,
    limit: u32,
) -> Vec<Bid>
```

**Parameters:**

- `investor`: Address of the investor
- `status_filter`: Optional bid status filter
- `offset`: Starting index for pagination
- `limit`: Maximum number of results to return

**Returns:** Vector of Bid objects

**Example:**

```rust
// Get investor's placed bids
let placed_bids = contract.get_investor_bids_paged(
    &env,
    investor_addr,
    Some(BidStatus::Placed),
    0,
    25
);
```

**Pagination Notes:**

- All paginated functions use overflow-safe arithmetic (`saturating_add`, `min`)
- Offset beyond data length returns empty results (no error)
- Limit of 0 returns empty results
- Filters are applied before pagination for accurate results

#### Audit & Backup

##### `get_audit_trail`

Retrieves audit trail for an invoice.

```rust
pub fn get_invoice_audit_trail(env: Env, invoice_id: BytesN<32>) -> Vec<BytesN<32>>
```

##### `create_backup`

Creates a backup of contract data.

```rust
pub fn create_backup(env: Env, description: String) -> Result<BytesN<32>, QuickLendXError>
```

### Data Structures

#### Invoice

```rust
pub struct Invoice {
    pub id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub due_date: u64,
    pub description: String,
    pub category: InvoiceCategory,
    pub tags: Vec<String>,
    pub status: InvoiceStatus,
    pub created_at: u64,
    pub updated_at: u64,
}
```

#### Bid

```rust
pub struct Bid {
    pub id: BytesN<32>,
    pub investor: Address,
    pub invoice_id: BytesN<32>,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub status: BidStatus,
    pub created_at: u64,
}
```

## 💻 Code Examples

### Complete Invoice Lifecycle

```rust
use soroban_sdk::{Address, String, Vec, vec, BytesN};

// 1. Business submits KYC
contract.submit_kyc_application(
    &env,
    business_addr,
    String::from_str(&env, "{\"name\":\"Acme Corp\",\"tax_id\":\"123456789\"}")
)?;

// 2. Admin verifies business
contract.verify_business(&env, admin_addr, business_addr)?;

// 3. Business creates invoice
let invoice_id = contract.store_invoice(
    &env,
    business_addr,
    50000, // $500.00
    usdc_addr,
    1735689600,
    String::from_str(&env, "Software development services"),
    InvoiceCategory::Services,
    vec![&env, String::from_str(&env, "software"), String::from_str(&env, "development")]
)?;

// 4. Investor places bid
let bid_id = contract.place_bid(
    &env,
    investor_addr,
    invoice_id,
    48000, // $480.00 bid
    52000  // $520.00 expected return
)?;

// 5. Business accepts bid
contract.accept_bid(&env, invoice_id, bid_id)?;

// 6. Invoice gets verified
contract.verify_invoice(&env, invoice_id)?;

// 7. Release escrow to investor
contract.release_escrow_funds(&env, invoice_id)?;
```

### Query Examples

```rust
// Get all invoices for a business
let business_invoices = contract.get_business_invoices(&env, business_addr);

// Get invoices by status
let pending_invoices = contract.get_invoices_by_status(&env, InvoiceStatus::Pending);

// Get invoices with rating above threshold
let high_rated_invoices = contract.get_invoices_with_rating_above(&env, 4);

// Get audit trail
let audit_trail = contract.get_invoice_audit_trail(&env, invoice_id);

// Query audit logs
let filter = AuditQueryFilter {
    operation: Some(AuditOperation::InvoiceCreated),
    actor: Some(business_addr),
    start_time: Some(1640995200), // Jan 1, 2022
    end_time: Some(1672531200),   // Jan 1, 2023
};
let audit_logs = contract.query_audit_logs(&env, filter, 100);

// Paginated queries for large datasets
// Get first page of business invoices (10 per page)
let page1 = contract.get_business_invoices_paged(
    &env,
    business_addr,
    None, // all statuses
    0,    // offset
    10    // limit
);

// Get second page
let page2 = contract.get_business_invoices_paged(
    &env,
    business_addr,
    None,
    10,   // offset
    10    // limit
);

// Get available invoices with filters
let filtered_invoices = contract.get_available_invoices_paged(
    &env,
    Some(5000),   // min $50.00
    Some(50000),  // max $500.00
    Some(InvoiceCategory::Services),
    0,
    20
);

// Get investor's active investments
let active_investments = contract.get_investor_investments_paged(
    &env,
    investor_addr,
    Some(InvestmentStatus::Active),
    0,
    50
);

// Get bid history for an invoice
let bids = contract.get_bid_history_paged(
    &env,
    invoice_id,
    Some(BidStatus::Placed),
    0,
    25
);
```

Query limit safety:

- Public query endpoints with pagination/limits enforce `MAX_QUERY_LIMIT = 100`.
- Effective limit is always `min(limit, 100)` for `query_audit_logs`, `query_analytics_data`,
  `get_business_invoices_paged`, `get_investor_investments_paged`,
  `get_available_invoices_paged`, `get_bid_history_paged`, and `get_investor_bids_paged`.

### Error Handling

```rust
use crate::errors::QuickLendXError;

match contract.store_invoice(&env, business, amount, currency, due_date, description, category, tags) {
    Ok(invoice_id) => {
        println!("Invoice created successfully: {:?}", invoice_id);
    }
    Err(QuickLendXError::InvalidAmount) => {
        println!("Error: Invalid invoice amount");
    }
    Err(QuickLendXError::InvoiceDueDateInvalid) => {
        println!("Error: Due date must be in the future");
    }
    Err(QuickLendXError::InvalidDescription) => {
        println!("Error: Description cannot be empty");
    }
    Err(e) => {
        println!("Unexpected error: {:?}", e);
    }
}
```
