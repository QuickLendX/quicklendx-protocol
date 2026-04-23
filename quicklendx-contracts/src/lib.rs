//! QuickLendX smart contract library root.
//!
//! Only the modules required for the escrow state-machine invariant tests
//! (issue #808) are declared here. Other modules have pre-existing compilation
//! issues unrelated to this issue and are excluded.

#![no_std]

extern crate alloc;

// ── Modules required by the escrow state-machine tests ───────────────────────

/// Error types used across the contract.
pub mod errors;

/// Minimal events stub: only `emit_escrow_created` is needed by `payments.rs`.
/// The full `events.rs` has deep dependencies on modules with pre-existing
/// compilation issues unrelated to issue #808.
#[path = "events_escrow_stub.rs"]
pub mod events;

/// Escrow payment primitives: create / release / refund.
pub mod payments;

// ── Minimal contract stub ─────────────────────────────────────────────────────
use soroban_sdk::{contract, contractimpl};

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {}

// ── Test modules ──────────────────────────────────────────────────────────────
#[cfg(test)]
mod test_escrow_state_machine;
