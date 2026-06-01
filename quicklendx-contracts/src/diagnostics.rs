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
        soroban_sdk::log!($env, concat!("[", $domain, "] ", $msg))
    };
    // Message with format arguments
    ($env:expr, $domain:literal, $msg:literal, $($arg:tt)*) => {
        soroban_sdk::log!($env, concat!("[", $domain, "] ", $msg), $($arg)*)
    };
}

/// No-op version of `qlx_log!` compiled when both `cfg(test)` and `feature = "diagnostics"`
/// are absent. Expands to `{}` — zero runtime cost, zero WASM size impact.
#[cfg(not(any(test, feature = "diagnostics")))]
#[macro_export]
macro_rules! qlx_log {
    ($env:expr, $domain:literal, $msg:literal) => {{}};
    ($env:expr, $domain:literal, $msg:literal, $($arg:tt)*) => {{}};
}
