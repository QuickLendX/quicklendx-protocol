//! Minimal events stub for escrow state-machine tests (issue #808).
//!
//! The full `events.rs` has deep dependencies on modules with pre-existing
//! compilation issues. This stub satisfies `payments.rs`'s import of
//! `emit_escrow_created` without pulling in the full module tree.

use crate::payments::Escrow;
use soroban_sdk::{symbol_short, Env};

/// Emit an escrow-created event.
///
/// In production this is the full implementation in `events.rs`.
/// In the test build this is a minimal stub that emits the same event
/// topic so tests can verify event emission if needed.
#[inline]
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
