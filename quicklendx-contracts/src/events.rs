use crate::bid::Bid;
use crate::fees::{FeeType, FeeStructure};
use crate::invoice::{Invoice, InvoiceMetadata};
use crate::payments::Escrow;
use crate::profits::PlatformFeeConfig;
use crate::verification::InvestorVerification;
use soroban_sdk::{symbol_short, Address, BytesN, Env, String, Symbol};

// ============================================================================
// Canonical Event Topics
//
// These compile-time constants define the **immutable** set of event topics
// emitted by the QuickLendX protocol.  Off-chain indexers and analytics tools
// MUST use these exact values when filtering Soroban event streams.
//
// Stability guarantee:
//   Once the contract is deployed to a network, no existing topic string will
//   be changed or removed.  New events may be added in future upgrades.
//   Payload fields follow append-only ordering: existing field positions are
//   frozen; new fields are appended at the end.
//
// All topics are at most 9 bytes to satisfy the `symbol_short!` constraint.
// ============================================================================

/// Emitted when a business uploads a new invoice.
/// Payload: (invoice_id, business, amount, currency, due_date, timestamp)
pub const TOPIC_INVOICE_UPLOADED: Symbol = symbol_short!("inv_up");

/// Emitted when an admin verifies an invoice.
/// Payload: (invoice_id, business, timestamp)
pub const TOPIC_INVOICE_VERIFIED: Symbol = symbol_short!("inv_ver");

/// Emitted when a business cancels a Pending or Verified invoice.
/// Payload: (invoice_id, business, timestamp)
pub const TOPIC_INVOICE_CANCELLED: Symbol = symbol_short!("inv_canc");

/// Emitted when an invoice is fully settled via `settle_invoice`.
/// Payload: (invoice_id, business, investor, investor_return, platform_fee, timestamp)
pub const TOPIC_INVOICE_SETTLED: Symbol = symbol_short!("inv_set");

/// Emitted when `handle_default` marks an invoice as Defaulted.
/// Payload: (invoice_id, business, investor, timestamp)
pub const TOPIC_INVOICE_DEFAULTED: Symbol = symbol_short!("inv_def");

/// Emitted when an invoice's bidding window expires without funding.
/// Payload: (invoice_id, business, due_date)
pub const TOPIC_INVOICE_EXPIRED: Symbol = symbol_short!("inv_exp");

/// Emitted for every partial payment recorded against a Funded invoice.
/// Payload: (invoice_id, business, payment_amount, total_paid, progress_bps, transaction_id)
pub const TOPIC_PARTIAL_PAYMENT: Symbol = symbol_short!("inv_pp");

/// Emitted when a single atomic payment record is stored.
/// Payload: (invoice_id, payer, amount, transaction_id, timestamp)
pub const TOPIC_PAYMENT_RECORDED: Symbol = symbol_short!("pay_rec");

/// Emitted after all payments are recorded and the invoice transitions to Settled.
/// Payload: (invoice_id, business, investor, total_paid, timestamp)
pub const TOPIC_INVOICE_SETTLED_FINAL: Symbol = symbol_short!("inv_stlf");

/// Emitted when an investor places a bid on a Verified invoice.
/// Payload: (bid_id, invoice_id, investor, bid_amount, expected_return, timestamp, expiration_timestamp)
pub const TOPIC_BID_PLACED: Symbol = symbol_short!("bid_plc");

/// Emitted when a business accepts a bid, moving the invoice to Funded.
/// Payload: (bid_id, invoice_id, investor, business, bid_amount, expected_return, timestamp)
pub const TOPIC_BID_ACCEPTED: Symbol = symbol_short!("bid_acc");

/// Emitted when an investor withdraws their active bid.
/// Payload: (bid_id, invoice_id, investor, bid_amount, timestamp)
pub const TOPIC_BID_WITHDRAWN: Symbol = symbol_short!("bid_wdr");

/// Emitted when `clean_expired_bids` removes a bid past its TTL.
/// Payload: (bid_id, invoice_id, investor, bid_amount, expiration_timestamp)
pub const TOPIC_BID_EXPIRED: Symbol = symbol_short!("bid_exp");

/// Emitted when an escrow account is created upon bid acceptance.
/// Payload: (escrow_id, invoice_id, investor, business, amount)
pub const TOPIC_ESCROW_CREATED: Symbol = symbol_short!("esc_cr");

/// Emitted when escrow funds are released to the business after settlement.
/// Payload: (escrow_id, invoice_id, business, amount)
pub const TOPIC_ESCROW_RELEASED: Symbol = symbol_short!("esc_rel");

/// Emitted when escrow funds are returned to the investor (cancellation / default).
/// Payload: (escrow_id, invoice_id, investor, amount)
pub const TOPIC_ESCROW_REFUNDED: Symbol = symbol_short!("esc_ref");

// ============================================================================
// Invoice Event Emitters
// ============================================================================

/// Emit an `inv_up` event when a business uploads a new invoice.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id`  | `BytesN<32>` | SHA-256 derived unique identifier |
/// | 1 | `business`    | `Address`    | Uploading business account        |
/// | 2 | `amount`      | `i128`       | Invoice face value (in stroops)   |
/// | 3 | `currency`    | `Address`    | Token contract address            |
/// | 4 | `due_date`    | `u64`        | Unix timestamp of payment due date|
/// | 5 | `timestamp`   | `u64`        | Ledger time of upload             |
///
/// # Security
/// `business` is the caller authenticated via `require_auth()` in `upload_invoice`.
pub fn emit_invoice_uploaded(env: &Env, invoice: &Invoice) {
    env.events().publish(
        (TOPIC_INVOICE_UPLOADED,),
        (
            invoice.id.clone(),
            invoice.business.clone(),
            invoice.amount,
            invoice.currency.clone(),
            invoice.due_date,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit an `inv_ver` event when an admin verifies an invoice.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id` | `BytesN<32>` | Unique invoice identifier |
/// | 1 | `business`   | `Address`    | Invoice owner             |
/// | 2 | `timestamp`  | `u64`        | Ledger time of verification |
///
/// # Security
/// Only the stored admin can call `verify_invoice`; auth checked upstream.
pub fn emit_invoice_verified(env: &Env, invoice: &Invoice) {
    env.events().publish(
        (TOPIC_INVOICE_VERIFIED,),
        (
            invoice.id.clone(),
            invoice.business.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit an `inv_canc` event when a business cancels an invoice.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id` | `BytesN<32>` | Unique invoice identifier |
/// | 1 | `business`   | `Address`    | Invoice owner             |
/// | 2 | `timestamp`  | `u64`        | Ledger time of cancellation |
///
/// # Security
/// Only the invoice's `business` can cancel (auth checked in `cancel_invoice`).
pub fn emit_invoice_cancelled(env: &Env, invoice: &Invoice) {
    env.events().publish(
        (TOPIC_INVOICE_CANCELLED,),
        (
            invoice.id.clone(),
            invoice.business.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit an `inv_meta` event when invoice metadata is updated.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id`      | `BytesN<32>` | Unique invoice identifier      |
/// | 1 | `customer_name`   | `String`     | Client name from metadata      |
/// | 2 | `tax_id`          | `String`     | Tax identifier from metadata   |
/// | 3 | `line_items_count`| `u32`        | Number of line items           |
/// | 4 | `total`           | `i128`       | Sum of line item amounts       |
pub fn emit_invoice_metadata_updated(env: &Env, invoice: &Invoice, metadata: &InvoiceMetadata) {
    let mut total = 0i128;
    for record in metadata.line_items.iter() {
        total = total.saturating_add(record.3);
    }

    env.events().publish(
        (symbol_short!("inv_meta"),),
        (
            invoice.id.clone(),
            metadata.customer_name.clone(),
            metadata.tax_id.clone(),
            metadata.line_items.len() as u32,
            total,
        ),
    );
}

/// Emit an `inv_mclr` event when invoice metadata is cleared.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id` | `BytesN<32>` | Unique invoice identifier |
/// | 1 | `business`   | `Address`    | Invoice owner             |
pub fn emit_invoice_metadata_cleared(env: &Env, invoice: &Invoice) {
    env.events().publish(
        (symbol_short!("inv_mclr"),),
        (invoice.id.clone(), invoice.business.clone()),
    );
}

/// Emit an `inv_veri` event when an investor is KYC-verified.
///
/// # Payload (positional, frozen)
/// | 0 | `investor`         | `Address` | Verified investor account      |
/// | 1 | `investment_limit` | `i128`    | Maximum allowed investment     |
/// | 2 | `verified_at`      | `u64`     | Ledger timestamp of approval   |
///
/// # Security
/// Only the stored admin can verify investors; auth checked upstream.
pub fn emit_investor_verified(env: &Env, verification: &InvestorVerification) {
    env.events().publish(
        (symbol_short!("inv_veri"),),
        (
            verification.investor.clone(),
            verification.investment_limit,
            verification.verified_at,
        ),
    );
}

/// Emit an `inv_set` event when an invoice is fully settled.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id`     | `BytesN<32>` | Unique invoice identifier            |
/// | 1 | `business`       | `Address`    | Invoice owner                        |
/// | 2 | `investor`       | `Address`    | Funding investor (zero addr if none) |
/// | 3 | `investor_return`| `i128`       | Net amount returned to investor      |
/// | 4 | `platform_fee`   | `i128`       | Fee collected by the platform        |
/// | 5 | `timestamp`      | `u64`        | Ledger time of settlement            |
///
/// # Security
/// `investor_return + platform_fee <= payment_amount` enforced in profits module.
pub fn emit_invoice_settled(
    env: &Env,
    invoice: &crate::invoice::Invoice,
    investor_return: i128,
    platform_fee: i128,
) {
    env.events().publish(
        (TOPIC_INVOICE_SETTLED,),
        (
            invoice.id.clone(),
            invoice.business.clone(),
            invoice.investor.clone().unwrap_or(Address::from_str(
                env,
                "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            )),
            investor_return,
            platform_fee,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit an `inv_pp` event for each partial payment on a Funded invoice.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id`     | `BytesN<32>` | Unique invoice identifier       |
/// | 1 | `business`       | `Address`    | Invoice owner                   |
/// | 2 | `payment_amount` | `i128`       | Amount paid in this instalment  |
/// | 3 | `total_paid`     | `i128`       | Cumulative paid so far          |
/// | 4 | `progress`       | `u32`        | Progress in basis points (0-10000) |
/// | 5 | `transaction_id` | `String`     | Off-chain transaction reference |
///
/// # Security
/// `payment_amount > 0` enforced in `make_payment`; `total_paid` is monotonic.
pub fn emit_partial_payment(
    env: &Env,
    invoice: &Invoice,
    payment_amount: i128,
    total_paid: i128,
    progress: u32,
    transaction_id: String,
) {
    env.events().publish(
        (TOPIC_PARTIAL_PAYMENT,),
        (
            invoice.id.clone(),
            invoice.business.clone(),
            payment_amount,
            total_paid,
            progress,
            transaction_id,
        ),
    );
}

/// Emit a `pay_rec` event when a single payment record is persisted.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id`    | `BytesN<32>` | Unique invoice identifier       |
/// | 1 | `payer`         | `Address`    | Account that sent the payment   |
/// | 2 | `amount`        | `i128`       | Payment amount                  |
/// | 3 | `transaction_id`| `String`     | Off-chain transaction reference |
/// | 4 | `timestamp`     | `u64`        | Ledger time of recording        |
pub fn emit_payment_recorded(
    env: &Env,
    invoice_id: &BytesN<32>,
    payer: &Address,
    amount: i128,
    transaction_id: String,
) {
    env.events().publish(
        (TOPIC_PAYMENT_RECORDED,),
        (
            invoice_id.clone(),
            payer.clone(),
            amount,
            transaction_id,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit an `inv_stlf` event after all payments complete and the invoice is Settled.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id` | `BytesN<32>` | Unique invoice identifier       |
/// | 1 | `business`   | `Address`    | Invoice owner                   |
/// | 2 | `investor`   | `Address`    | Funding investor                |
/// | 3 | `total_paid` | `i128`       | Total amount paid               |
/// | 4 | `timestamp`  | `u64`        | Ledger time of final settlement |
///
/// # Security
/// Emitted only once per invoice when status transitions to Settled.
pub fn emit_invoice_settled_final(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    investor: &Address,
    total_paid: i128,
) {
    env.events().publish(
        (TOPIC_INVOICE_SETTLED_FINAL,),
        (
            invoice_id.clone(),
            business.clone(),
            investor.clone(),
            total_paid,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit an `inv_exp` event when an invoice's bidding window expires.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id` | `BytesN<32>` | Unique invoice identifier |
/// | 1 | `business`   | `Address`    | Invoice owner             |
/// | 2 | `due_date`   | `u64`        | Original due date         |
pub fn emit_invoice_expired(env: &Env, invoice: &crate::invoice::Invoice) {
    env.events().publish(
        (TOPIC_INVOICE_EXPIRED,),
        (
            invoice.id.clone(),
            invoice.business.clone(),
            invoice.due_date,
        ),
    );
}

/// Emit an `inv_def` event when an invoice is marked as Defaulted.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id` | `BytesN<32>` | Unique invoice identifier            |
/// | 1 | `business`   | `Address`    | Invoice owner                        |
/// | 2 | `investor`   | `Address`    | Funding investor (zero addr if none) |
/// | 3 | `timestamp`  | `u64`        | Ledger time of default               |
///
/// # Security
/// Only callable after `due_date` has passed (grace period enforced upstream).
pub fn emit_invoice_defaulted(env: &Env, invoice: &crate::invoice::Invoice) {
    env.events().publish(
        (TOPIC_INVOICE_DEFAULTED,),
        (
            invoice.id.clone(),
            invoice.business.clone(),
            invoice.investor.clone().unwrap_or(Address::from_str(
                env,
                "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            )),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit an `inv_fnd` event when an invoice transitions to Funded.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id` | `BytesN<32>` | Unique invoice identifier |
/// | 1 | `investor`   | `Address`    | Funding investor          |
/// | 2 | `amount`     | `i128`       | Funded amount             |
/// | 3 | `timestamp`  | `u64`        | Ledger time of funding    |
pub fn emit_invoice_funded(env: &Env, invoice_id: &BytesN<32>, investor: &Address, amount: i128) {
    env.events().publish(
        (symbol_short!("inv_fnd"),),
        (
            invoice_id.clone(),
            investor.clone(),
            amount,
            env.ledger().timestamp(),
        ),
    );
}

// ============================================================================
// Insurance Event Emitters
// ============================================================================

/// Emit an `ins_add` event when investment insurance is attached.
///
/// # Payload (positional, frozen)
/// | 0 | `investment_id`      | `BytesN<32>` | Investment identifier    |
/// | 1 | `invoice_id`         | `BytesN<32>` | Invoice identifier       |
/// | 2 | `investor`           | `Address`    | Insured investor         |
/// | 3 | `provider`           | `Address`    | Insurance provider       |
/// | 4 | `coverage_percentage`| `u32`        | Coverage % (0-100)       |
/// | 5 | `coverage_amount`    | `i128`       | Max payout               |
/// | 6 | `premium_amount`     | `i128`       | Premium paid             |
pub fn emit_insurance_added(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    investor: &Address,
    provider: &Address,
    coverage_percentage: u32,
    coverage_amount: i128,
    premium_amount: i128,
) {
    env.events().publish(
        (symbol_short!("ins_add"),),
        (
            investment_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            provider.clone(),
            coverage_percentage,
            coverage_amount,
            premium_amount,
        ),
    );
}

/// Emit an `ins_prm` event when an insurance premium is collected.
///
/// # Payload (positional, frozen)
/// | 0 | `investment_id` | `BytesN<32>` | Investment identifier |
/// | 1 | `provider`      | `Address`    | Insurance provider    |
/// | 2 | `premium_amount`| `i128`       | Premium collected     |
pub fn emit_insurance_premium_collected(
    env: &Env,
    investment_id: &BytesN<32>,
    provider: &Address,
    premium_amount: i128,
) {
    env.events().publish(
        (symbol_short!("ins_prm"),),
        (investment_id.clone(), provider.clone(), premium_amount),
    );
}

/// Emit an `ins_clm` event when an insurance claim is paid out.
///
/// # Payload (positional, frozen)
/// | 0 | `investment_id` | `BytesN<32>` | Investment identifier |
/// | 1 | `invoice_id`    | `BytesN<32>` | Invoice identifier    |
/// | 2 | `provider`      | `Address`    | Insurance provider    |
/// | 3 | `coverage_amount`| `i128`      | Amount paid out       |
pub fn emit_insurance_claimed(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    provider: &Address,
    coverage_amount: i128,
) {
    env.events().publish(
        (symbol_short!("ins_clm"),),
        (
            investment_id.clone(),
            invoice_id.clone(),
            provider.clone(),
            coverage_amount,
        ),
    );
}

// ============================================================================
// Platform Fee Event Emitters
// ============================================================================

/// Emit a `fee_upd` event when the platform fee configuration changes.
///
/// # Payload (positional, frozen)
/// | 0 | `fee_bps`    | `i128`   | New fee in basis points  |
/// | 1 | `updated_at` | `u64`    | Ledger time of update    |
/// | 2 | `updated_by` | `Address`| Admin who made the change|
///
/// # Security
/// Only the stored admin can update the fee; auth checked upstream.
pub fn emit_platform_fee_updated(env: &Env, config: &PlatformFeeConfig) {
    env.events().publish(
        (symbol_short!("fee_upd"),),
        (
            old_bps,
            new_bps,
            updated_by.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit a `fee_rout` event when platform fees are routed to the treasury.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id` | `BytesN<32>` | Source invoice              |
/// | 1 | `recipient`  | `Address`    | Treasury address            |
/// | 2 | `fee_amount` | `i128`       | Fee amount routed           |
/// | 3 | `timestamp`  | `u64`        | Ledger time of routing      |
pub fn emit_platform_fee_routed(
    env: &Env,
    invoice_id: &BytesN<32>,
    recipient: &Address,
    fee_amount: i128,
) {
    env.events().publish(
        (symbol_short!("fee_rout"),),
        (
            invoice_id.clone(),
            recipient.clone(),
            fee_amount,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit a `fee_cfg` event when the platform fee bps is reconfigured.
///
/// # Payload (positional, frozen)
/// | 0 | `old_fee_bps` | `u32`    | Previous fee in bps      |
/// | 1 | `new_fee_bps` | `u32`    | New fee in bps           |
/// | 2 | `updated_by`  | `Address`| Admin who changed it     |
/// | 3 | `timestamp`   | `u64`    | Ledger time of update    |
pub fn emit_platform_fee_config_updated(
    env: &Env,
    old_fee_bps: u32,
    new_fee_bps: u32,
    updated_by: &Address,
) {
    env.events().publish(
        (symbol_short!("fee_cfg"),),
        (
            old_fee_bps,
            new_fee_bps,
            updated_by.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit a `trs_cfg` event when the treasury address is configured.
///
/// # Payload (positional, frozen)
/// | 0 | `treasury_address` | `Address` | New treasury account    |
/// | 1 | `configured_by`    | `Address` | Admin who set it        |
/// | 2 | `timestamp`        | `u64`     | Ledger time of config   |
pub fn emit_treasury_configured(env: &Env, treasury_address: &Address, configured_by: &Address) {
    env.events().publish(
        (symbol_short!("trs_cfg"),),
        (
            treasury_address.clone(),
            configured_by.clone(),
            env.ledger().timestamp(),
        ),
    );
}

// ============================================================================
// Escrow Event Emitters
// ============================================================================

/// Emit an `esc_cr` event when an escrow account is created for a bid.
///
/// # Payload (positional, frozen)
/// | 0 | `escrow_id`  | `BytesN<32>` | Unique escrow identifier |
/// | 1 | `invoice_id` | `BytesN<32>` | Associated invoice       |
/// | 2 | `investor`   | `Address`    | Funding investor         |
/// | 3 | `business`   | `Address`    | Invoice owner            |
/// | 4 | `amount`     | `i128`       | Escrowed amount          |
///
/// # Security
/// Funds are locked in the contract; only released via `release_escrow_funds`
/// or refunded via `refund_escrow`.
pub fn emit_escrow_created(env: &Env, escrow: &Escrow) {
    env.events().publish(
        (symbol_short!("esc_cr"),),
        (
            escrow.escrow_id.clone(),
            escrow.invoice_id.clone(),
            escrow.investor.clone(),
            escrow.business.clone(),
            escrow.amount,
        ),
    );
}

/// Emit an `esc_rel` event when escrow funds are released to the business.
///
/// # Payload (positional, frozen)
/// | 0 | `escrow_id`  | `BytesN<32>` | Unique escrow identifier |
/// | 1 | `invoice_id` | `BytesN<32>` | Associated invoice       |
/// | 2 | `business`   | `Address`    | Recipient                |
/// | 3 | `amount`     | `i128`       | Released amount          |
///
/// # Security
/// Callable only after invoice is in Settled status; admin or authorized caller.
pub fn emit_escrow_released(
    env: &Env,
    escrow_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    business: &Address,
    amount: i128,
) {
    env.events().publish(
        (symbol_short!("esc_rel"),),
        (
            escrow_id.clone(),
            invoice_id.clone(),
            business.clone(),
            amount,
        ),
    );
}

/// Emit an `esc_ref` event when escrow funds are refunded to the investor.
///
/// # Payload (positional, frozen)
/// | 0 | `escrow_id`  | `BytesN<32>` | Unique escrow identifier |
/// | 1 | `invoice_id` | `BytesN<32>` | Associated invoice       |
/// | 2 | `investor`   | `Address`    | Recipient                |
/// | 3 | `amount`     | `i128`       | Refunded amount          |
///
/// # Security
/// Triggered on invoice cancellation or confirmed default path.
pub fn emit_escrow_refunded(
    env: &Env,
    escrow_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    investor: &Address,
    amount: i128,
) {
    env.events().publish(
        (symbol_short!("esc_ref"),),
        (
            escrow_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            amount,
        ),
    );
}

// ============================================================================
// Bid Event Emitters
// ============================================================================

/// Emit a `bid_plc` event when an investor places a bid.
///
/// # Payload (positional, frozen)
/// | 0 | `bid_id`               | `BytesN<32>` | Unique bid identifier          |
/// | 1 | `invoice_id`           | `BytesN<32>` | Target invoice                 |
/// | 2 | `investor`             | `Address`    | Bidding investor               |
/// | 3 | `bid_amount`           | `i128`       | Offered principal              |
/// | 4 | `expected_return`      | `i128`       | Expected repayment amount      |
/// | 5 | `timestamp`            | `u64`        | Ledger time of bid             |
/// | 6 | `expiration_timestamp` | `u64`        | When the bid lapses            |
///
/// # Security
/// `investor` is authenticated via `require_auth()` in `place_bid`.
pub fn emit_bid_placed(env: &Env, bid: &Bid) {
    env.events().publish(
        (symbol_short!("bid_plc"),),
        (
            bid.bid_id.clone(),
            bid.invoice_id.clone(),
            bid.investor.clone(),
            bid.bid_amount,
            bid.expected_return,
            bid.timestamp,
            bid.expiration_timestamp,
        ),
    );
}

/// Emit a `bid_wdr` event when an investor withdraws their bid.
///
/// # Payload (positional, frozen)
/// | 0 | `bid_id`     | `BytesN<32>` | Unique bid identifier |
/// | 1 | `invoice_id` | `BytesN<32>` | Target invoice        |
/// | 2 | `investor`   | `Address`    | Withdrawing investor  |
/// | 3 | `bid_amount` | `i128`       | Returned principal    |
/// | 4 | `timestamp`  | `u64`        | Ledger time of withdrawal |
///
/// # Security
/// Only the bid's `investor` can withdraw; auth checked via `require_auth()`.
pub fn emit_bid_withdrawn(env: &Env, bid: &Bid) {
    env.events().publish(
        (symbol_short!("bid_wdr"),),
        (
            bid.bid_id.clone(),
            bid.invoice_id.clone(),
            bid.investor.clone(),
            bid.bid_amount,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit a `bid_acc` event when a business accepts a bid.
///
/// # Payload (positional, frozen)
/// | 0 | `bid_id`          | `BytesN<32>` | Unique bid identifier     |
/// | 1 | `invoice_id`      | `BytesN<32>` | Target invoice            |
/// | 2 | `investor`        | `Address`    | Funding investor          |
/// | 3 | `business`        | `Address`    | Invoice owner             |
/// | 4 | `bid_amount`      | `i128`       | Funded principal          |
/// | 5 | `expected_return` | `i128`       | Agreed repayment amount   |
/// | 6 | `timestamp`       | `u64`        | Ledger time of acceptance |
///
/// # Security
/// Only the invoice's `business` can accept bids; auth checked upstream.
pub fn emit_bid_accepted(env: &Env, bid: &Bid, invoice_id: &BytesN<32>, business: &Address) {
    env.events().publish(
        (symbol_short!("bid_acc"),),
        (
            bid.bid_id.clone(),
            invoice_id.clone(),
            bid.investor.clone(),
            business.clone(),
            bid.bid_amount,
            bid.expected_return,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit a `bid_exp` event when an expired bid is cleaned up.
///
/// # Payload (positional, frozen)
/// | 0 | `bid_id`               | `BytesN<32>` | Unique bid identifier  |
/// | 1 | `invoice_id`           | `BytesN<32>` | Target invoice         |
/// | 2 | `investor`             | `Address`    | Investor who placed it |
/// | 3 | `bid_amount`           | `i128`       | Original bid amount    |
/// | 4 | `expiration_timestamp` | `u64`        | TTL timestamp          |
pub fn emit_bid_expired(env: &Env, bid: &Bid) {
    env.events().publish(
        (symbol_short!("bid_exp"),),
        (
            bid.bid_id.clone(),
            bid.invoice_id.clone(),
            bid.investor.clone(),
            bid.bid_amount,
            bid.expiration_timestamp,
        ),
    );
}

// ============================================================================
// Backup Event Emitters
// ============================================================================

/// Emit a `bkup_crt` event when a manual backup is created.
///
/// # Payload (positional, frozen)
/// | 0 | `backup_id`     | `BytesN<32>` | Unique backup identifier |
/// | 1 | `invoice_count` | `u32`        | Number of invoices saved |
/// | 2 | `timestamp`     | `u64`        | Ledger time of backup    |
pub fn emit_backup_created(env: &Env, backup_id: &BytesN<32>, invoice_count: u32) {
    env.events().publish(
        (symbol_short!("bkup_crt"),),
        (backup_id.clone(), invoice_count, env.ledger().timestamp()),
    );
}

/// Emit a `bkup_rstr` event when a backup is restored.
///
/// # Payload (positional, frozen)
/// | 0 | `backup_id`     | `BytesN<32>` | Restored backup identifier |
/// | 1 | `invoice_count` | `u32`        | Number of invoices restored|
/// | 2 | `timestamp`     | `u64`        | Ledger time of restore     |
pub fn emit_backup_restored(env: &Env, backup_id: &BytesN<32>, invoice_count: u32) {
    env.events().publish(
        (symbol_short!("bkup_rstr"),),
        (backup_id.clone(), invoice_count, env.ledger().timestamp()),
    );
}

/// Emit a `bkup_vd` event when a backup is validated.
///
/// # Payload (positional, frozen)
/// | 0 | `backup_id` | `BytesN<32>` | Validated backup identifier |
/// | 1 | `success`   | `bool`       | Whether validation passed   |
/// | 2 | `timestamp` | `u64`        | Ledger time of validation   |
pub fn emit_backup_validated(env: &Env, backup_id: &BytesN<32>, success: bool) {
    env.events().publish(
        (symbol_short!("bkup_vd"),),
        (backup_id.clone(), success, env.ledger().timestamp()),
    );
}

/// Emit a `bkup_ar` event when a backup is archived.
///
/// # Payload (positional, frozen)
/// | 0 | `backup_id` | `BytesN<32>` | Archived backup identifier |
/// | 1 | `timestamp` | `u64`        | Ledger time of archival    |
pub fn emit_backup_archived(env: &Env, backup_id: &BytesN<32>) {
    env.events().publish(
        (symbol_short!("bkup_ar"),),
        (backup_id.clone(), env.ledger().timestamp()),
    );
}

/// Emit a `ret_pol` event when the backup retention policy is updated.
///
/// # Payload (positional, frozen)
/// | 0 | `max_backups`        | `u32`  | Maximum retained backups    |
/// | 1 | `max_age_seconds`    | `u64`  | Maximum backup age          |
/// | 2 | `auto_cleanup_enabled`| `bool`| Whether auto-cleanup is on |
/// | 3 | `timestamp`          | `u64`  | Ledger time of update       |
pub fn emit_retention_policy_updated(
    env: &Env,
    max_backups: u32,
    max_age_seconds: u64,
    auto_cleanup_enabled: bool,
) {
    env.events().publish(
        (symbol_short!("ret_pol"),),
        (
            max_backups,
            max_age_seconds,
            auto_cleanup_enabled,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit a `bkup_cln` event when expired backups are cleaned up.
///
/// # Payload (positional, frozen)
/// | 0 | `removed_count` | `u32` | Number of backups removed |
/// | 1 | `timestamp`     | `u64` | Ledger time of cleanup    |
pub fn emit_backups_cleaned(env: &Env, removed_count: u32) {
    env.events().publish(
        (symbol_short!("bkup_cln"),),
        (removed_count, env.ledger().timestamp()),
    );
}

// ============================================================================
// Audit Event Emitters
// ============================================================================

/// Emit an `aud_val` event when invoice audit integrity is validated.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id` | `BytesN<32>` | Validated invoice identifier |
/// | 1 | `is_valid`   | `bool`       | Whether the audit passed     |
/// | 2 | `timestamp`  | `u64`        | Ledger time of validation    |
pub fn emit_audit_validation(env: &Env, invoice_id: &BytesN<32>, is_valid: bool) {
    env.events().publish(
        (symbol_short!("aud_val"),),
        (invoice_id.clone(), is_valid, env.ledger().timestamp()),
    );
}

/// Emit an `aud_qry` event when audit logs are queried.
///
/// # Payload (positional, frozen)
/// | 0 | `query_type`   | `String` | Type identifier for the query |
/// | 1 | `result_count` | `u32`    | Number of records returned    |
pub fn emit_audit_query(env: &Env, query_type: String, result_count: u32) {
    env.events()
        .publish((symbol_short!("aud_qry"),), (query_type, result_count));
}

// ============================================================================
// Invoice Category / Tag Event Emitters
// ============================================================================

/// Emit a `cat_upd` event when an invoice's category is changed.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id`   | `BytesN<32>`    | Unique invoice identifier |
/// | 1 | `business`     | `Address`       | Invoice owner             |
/// | 2 | `old_category` | `InvoiceCategory`| Previous category        |
/// | 3 | `new_category` | `InvoiceCategory`| Updated category         |
pub fn emit_invoice_category_updated(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    old_category: &crate::invoice::InvoiceCategory,
    new_category: &crate::invoice::InvoiceCategory,
) {
    env.events().publish(
        (symbol_short!("cat_upd"),),
        (
            invoice_id.clone(),
            business.clone(),
            old_category.clone(),
            new_category.clone(),
        ),
    );
}

/// Emit a `tag_add` event when a tag is added to an invoice.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id` | `BytesN<32>` | Unique invoice identifier |
/// | 1 | `business`   | `Address`    | Invoice owner             |
/// | 2 | `tag`        | `String`     | Tag value                 |
pub fn emit_invoice_tag_added(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    tag: &String,
) {
    env.events().publish(
        (symbol_short!("tag_add"),),
        (invoice_id.clone(), business.clone(), tag.clone()),
    );
}

/// Emit a `tag_rm` event when a tag is removed from an invoice.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id` | `BytesN<32>` | Unique invoice identifier |
/// | 1 | `business`   | `Address`    | Invoice owner             |
/// | 2 | `tag`        | `String`     | Tag value removed         |
pub fn emit_invoice_tag_removed(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    tag: &String,
) {
    env.events().publish(
        (symbol_short!("tag_rm"),),
        (invoice_id.clone(), business.clone(), tag.clone()),
    );
}

// ============================================================================
// Dispute Event Emitters
// ============================================================================

/// Emit a `dsp_cr` event when a dispute is opened.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id`  | `BytesN<32>` | Disputed invoice         |
/// | 1 | `created_by`  | `Address`    | Dispute initiator        |
/// | 2 | `reason`      | `String`     | Short reason string      |
/// | 3 | `timestamp`   | `u64`        | Ledger time of creation  |
///
/// # Security
/// Only the invoice `business` or the funding `investor` may open a dispute.
pub fn emit_dispute_created(
    env: &Env,
    invoice_id: &BytesN<32>,
    created_by: &Address,
    reason: &String,
) {
    env.events().publish(
        (symbol_short!("dsp_cr"),),
        (
            invoice_id.clone(),
            created_by.clone(),
            reason.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit a `dsp_ur` event when a dispute is escalated to UnderReview.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id`  | `BytesN<32>` | Disputed invoice         |
/// | 1 | `reviewed_by` | `Address`    | Admin handling review    |
/// | 2 | `timestamp`   | `u64`        | Ledger time of escalation|
pub fn emit_dispute_under_review(env: &Env, invoice_id: &BytesN<32>, reviewed_by: &Address) {
    env.events().publish(
        (symbol_short!("dsp_ur"),),
        (
            invoice_id.clone(),
            reviewed_by.clone(),
            env.ledger().timestamp(),
        ),
    );
}

/// Emit a `dsp_rs` event when a dispute is resolved.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id`  | `BytesN<32>` | Disputed invoice         |
/// | 1 | `resolved_by` | `Address`    | Admin who resolved it    |
/// | 2 | `resolution`  | `String`     | Resolution summary       |
/// | 3 | `timestamp`   | `u64`        | Ledger time of resolution|
///
/// # Security
/// Only the stored admin can resolve disputes.
pub fn emit_dispute_resolved(
    env: &Env,
    invoice_id: &BytesN<32>,
    resolved_by: &Address,
    resolution: &String,
) {
    env.events().publish(
        (symbol_short!("dsp_rs"),),
        (
            invoice_id.clone(),
            resolved_by.clone(),
            resolution.clone(),
            env.ledger().timestamp(),
        ),
    );
}

// ============================================================================
// Profit / Fee Breakdown Event Emitter
// ============================================================================

/// Emit a `pf_brk` event with full settlement calculation details.
///
/// # Payload (positional, frozen)
/// | 0 | `invoice_id`       | `BytesN<32>` | Source invoice                |
/// | 1 | `investment_amount`| `i128`       | Original principal invested   |
/// | 2 | `payment_amount`   | `i128`       | Total payment received        |
/// | 3 | `gross_profit`     | `i128`       | Profit before fees            |
/// | 4 | `platform_fee`     | `i128`       | Fee charged by platform       |
/// | 5 | `investor_return`  | `i128`       | Net returned to investor      |
/// | 6 | `fee_bps_applied`  | `i128`       | Fee rate used (basis points)  |
/// | 7 | `timestamp`        | `u64`        | Ledger time of settlement     |
///
/// # Security
/// `investor_return = payment_amount - platform_fee`; verified in profits module.
#[allow(dead_code)]
pub fn emit_profit_fee_breakdown(
    env: &Env,
    invoice_id: &BytesN<32>,
    investment_amount: i128,
    payment_amount: i128,
    gross_profit: i128,
    platform_fee: i128,
    investor_return: i128,
    fee_bps_applied: i128,
) {
    env.events().publish(
        (symbol_short!("pf_brk"),),
        (
            invoice_id.clone(),
            investment_amount,
            payment_amount,
            gross_profit,
            platform_fee,
            investor_return,
            fee_bps_applied,
            env.ledger().timestamp(),
        ),
    );
}

/// Emit event when the admin updates the bid TTL configuration.
///
/// ### Fields
/// - `old_days`: previous TTL value in days (0 = was using compile-time default)
/// - `new_days`: newly configured TTL value in days
/// - `admin`: address of the admin who made the change
/// - `timestamp`: ledger timestamp of the change
pub fn emit_bid_ttl_updated(env: &Env, old_days: u64, new_days: u64, admin: &Address) {
    env.events().publish(
        (symbol_short!("ttl_upd"),),
        (old_days, new_days, admin.clone(), env.ledger().timestamp()),
    );
}
