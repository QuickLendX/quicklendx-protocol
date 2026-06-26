//! Structured diagnostics for QuickLendX smart contracts.
//!
//! This module provides the [`qlx_log!`] macro, a feature-gated wrapper around
//! [`soroban_sdk::log!`] that emits uniformly domain-tagged diagnostic messages.
//!
//! # Feature Gate
//!
//! The macro emits real log statements only when compiled under:
//! - **`cfg(test)`** — i.e., running `cargo test` (with or without `--features diagnostics`)
//! - **`feature = "diagnostics"`** — i.e., built with `--features diagnostics`
//!
//! In all other configurations (including production `release` and `release-with-logs`
//! without the `diagnostics` feature), the macro expands to an empty statement `{}`
//! with **zero runtime cost**: format strings, domain literals, and argument expressions
//! are fully eliminated by the compiler before WASM codegen.
//!
//! # Usage
//!
//! ```rust,ignore
//! // Plain message
//! qlx_log!(env, "escrow", "Escrow created successfully");
//!
//! // With formatting arguments (passed directly to soroban_sdk::log!)
//! qlx_log!(env, "bid", "Bid placed: amount={}", bid_amount);
//! qlx_log!(env, "settlement", "Payment recorded: applied={} total={}", applied, total);
//! ```
//!
//! # Domains
//!
//! Use consistent domain strings to group and filter log output:
//!
//! | Domain       | Scope                                                         |
//! |--------------|---------------------------------------------------------------|
//! | `"escrow"`   | Escrow creation, acceptance, refund, and release transitions  |
//! | `"bid"`      | Bid placement, withdrawal, cancellation, and expiry           |
//! | `"settlement"` | Partial payments, full settlement, and finalization           |
//! | `"payment"`  | Low-level token transfers and escrow fund movements           |
//!
//! # Output Format
//!
//! Every log line is prefixed with the domain tag:
//! ```text
//! [escrow] Escrow created successfully
//! [bid] Bid placed: amount=5000
//! [settlement] Payment recorded: applied=1000 total=3000
//! ```
//!
//! # Zero-Overhead Guarantee
//!
//! When the `diagnostics` feature is absent and we are not in a `#[cfg(test)]` context,
//! the macro body expands to `{}` — no string allocation, no format evaluation,
//! no host function call. The Rust compiler and LLVM/WASM optimizer will eliminate
//! the call sites entirely, maintaining strict WASM size and gas budgets.

/// Feature-gated structured diagnostics macro for QuickLendX contracts.
///
/// Emits a domain-tagged diagnostic log via [`soroban_sdk::log!`] only when compiled
/// under `#[cfg(test)]` or with the `diagnostics` Cargo feature enabled.
///
/// In release builds without the feature, **all invocations expand to `{}`** — no
/// runtime overhead whatsoever.
///
/// # Syntax
///
/// ```rust,ignore
/// // No format arguments
/// qlx_log!(env, "domain", "message");
///
/// // With format arguments (forwarded to soroban_sdk::log!)
/// qlx_log!(env, "domain", "key={}", value);
/// ```
///
/// # Examples
///
/// ```rust,ignore
/// use crate::qlx_log;
///
/// qlx_log!(env, "escrow", "Escrow created");
/// qlx_log!(env, "bid", "Bid placed: amount={}", bid.bid_amount);
/// qlx_log!(env, "settlement", "Settled: investor_return={} fee={}", investor_return, platform_fee);
/// ```
#[cfg(any(test, feature = "diagnostics"))]
#[macro_export]
macro_rules! qlx_log {
    // Plain message — no format args
    ($env:expr, $domain:literal, $msg:literal) => {
        soroban_sdk::log!($env, "[{}] {}", $domain, $msg)
    };
    // Message with format arguments
    ($env:expr, $domain:literal, $msg:literal, $($arg:tt)*) => {{
        let rendered = ::alloc::format!($msg, $($arg)*);
        soroban_sdk::log!($env, "[{}] {}", $domain, rendered)
    }};
}

/// No-op version of `qlx_log!` compiled when both `cfg(test)` and `feature = "diagnostics"`
/// are absent. Expands to `{}` — zero runtime cost, zero WASM size impact.
#[cfg(not(any(test, feature = "diagnostics")))]
#[macro_export]
macro_rules! qlx_log {
    ($env:expr, $domain:literal, $msg:literal) => {{}};
    ($env:expr, $domain:literal, $msg:literal, $($arg:tt)*) => {{}};
}

/// A rich diagnostic snapshot of internal protocol state.
///
/// Only available when compiled with `--features diagnostics`. Never present in
/// production WASM builds, keeping the on-chain surface minimal.
///
/// All fields are read-only aggregates; no state is mutated.
#[cfg(feature = "diagnostics")]
#[soroban_sdk::contracttype]
#[derive(Clone)]
pub struct ProtocolDiagnostics {
    /// Total invoices across all statuses.
    pub total_invoices: u64,
    /// Invoices in `Pending` status.
    pub pending_invoices: u32,
    /// Invoices in `Verified` status.
    pub verified_invoices: u32,
    /// Invoices in `Funded` status.
    pub funded_invoices: u32,
    /// Invoices in `Paid` status.
    pub paid_invoices: u32,
    /// Invoices in `Defaulted` status.
    pub defaulted_invoices: u32,
    /// Running bid counter (monotonically increasing).
    pub total_bids_ever: u64,
    /// Whether the protocol is currently paused.
    pub is_paused: bool,
    /// Whether maintenance mode is active.
    pub is_maintenance: bool,
    /// Whether backpressure load-shedding is active.
    pub backpressure_active: bool,
    /// Current fee in basis points.
    pub fee_bps: u32,
    /// Number of whitelisted currencies.
    pub currency_count: u32,
    /// Ledger sequence at snapshot time.
    pub ledger_sequence: u32,
    /// Ledger timestamp at snapshot time.
    pub ledger_timestamp: u64,
}

/// Build a `ProtocolDiagnostics` snapshot from current contract state.
///
/// Read-only; no authentication required; no state mutations. This function is
/// only compiled when the `diagnostics` Cargo feature is enabled — it is entirely
/// absent from production WASM builds.
#[cfg(feature = "diagnostics")]
pub fn get_protocol_diagnostics(env: &soroban_sdk::Env) -> ProtocolDiagnostics {
    use crate::backpressure::BackpressureControl;
    use crate::bid::BidStorage;
    use crate::currency::CurrencyWhitelist;
    use crate::init::ProtocolInitializer;
    use crate::maintenance::MaintenanceControl;
    use crate::pause::PauseControl;
    use crate::storage::InvoiceStorage;
    use crate::types::InvoiceStatus;

    let pending = InvoiceStorage::get_invoices_by_status(env, InvoiceStatus::Pending).len();
    let verified = InvoiceStorage::get_invoices_by_status(env, InvoiceStatus::Verified).len();
    let funded = InvoiceStorage::get_invoices_by_status(env, InvoiceStatus::Funded).len();
    let paid = InvoiceStorage::get_invoices_by_status(env, InvoiceStatus::Paid).len();
    let defaulted = InvoiceStorage::get_invoices_by_status(env, InvoiceStatus::Defaulted).len();

    ProtocolDiagnostics {
        total_invoices: InvoiceStorage::get_total_count(env),
        pending_invoices: pending,
        verified_invoices: verified,
        funded_invoices: funded,
        paid_invoices: paid,
        defaulted_invoices: defaulted,
        total_bids_ever: BidStorage::next_count(env),
        is_paused: PauseControl::is_paused(env),
        is_maintenance: MaintenanceControl::is_maintenance_mode(env),
        backpressure_active: BackpressureControl::is_active(env),
        fee_bps: ProtocolInitializer::get_fee_bps(env),
        currency_count: CurrencyWhitelist::currency_count(env),
        ledger_sequence: env.ledger().sequence(),
        ledger_timestamp: env.ledger().timestamp(),
    }
}
