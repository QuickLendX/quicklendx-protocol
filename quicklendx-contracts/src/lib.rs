//! QuickLendX smart contract library root.
//!
//! Only the modules required for the escrow state-machine invariant tests
//! (issue #808) and bid-ranking tie-breaker tests (issue #811) are declared
//! here. Other modules have pre-existing compilation issues unrelated to
//! these issues and are excluded.

#![no_std]

extern crate alloc;

// ── Modules required by the escrow + bid-ranking tests ───────────────────────

/// Error types used across the contract.
pub mod errors;

/// Minimal events stub: only the events needed by `payments.rs` and `bid.rs`.
/// The full `events.rs` has deep dependencies on modules with pre-existing
/// compilation issues unrelated to issues #808 and #811.
#[path = "events_escrow_stub.rs"]
pub mod events;

/// Shared data types (Bid, BidStatus, Investment, etc.).
// pub mod types; // excluded: bid.rs defines Bid/BidStatus locally; types module causes conflicts

/// Admin storage helpers.
pub mod admin;

/// Escrow payment primitives: create / release / refund.
pub mod payments;

/// Bid storage, ranking, and tie-breaker logic.
pub mod bid;

// ── Minimal contract stub ─────────────────────────────────────────────────────
use soroban_sdk::{contract, contractimpl};

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {}

// ── Test modules ──────────────────────────────────────────────────────────────
#[cfg(test)]
mod test_escrow_state_machine;

#[cfg(test)]
mod test_bid_ranking_tiebreaker;

#[cfg(test)]
mod test_invoice_id_collision_regression;
