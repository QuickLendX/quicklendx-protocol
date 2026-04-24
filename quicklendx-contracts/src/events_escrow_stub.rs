//! Minimal events stub for escrow state-machine and bid-ranking tests.
//!
//! The full `events.rs` has deep dependencies on modules with pre-existing
//! compilation issues. This stub satisfies the imports needed by `payments.rs`,
//! `bid.rs`, and `admin.rs` without pulling in the full module tree.

use crate::payments::Escrow;
use crate::bid::Bid;
use soroban_sdk::{symbol_short, Address, Env};

/// Emit an escrow-created event.
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

/// Emit a bid-expired event.
#[inline]
pub fn emit_bid_expired(env: &Env, bid: &Bid) {
    env.events().publish(
        (symbol_short!("bid_exp"),),
        (
            bid.bid_id.clone(),
            bid.invoice_id.clone(),
            bid.investor.clone(),
            bid.bid_amount,
        ),
    );
}

/// Emit a bid-TTL-updated event.
#[inline]
pub fn emit_bid_ttl_updated(env: &Env, old_days: u64, new_days: u64, admin: &Address) {
    env.events().publish(
        (symbol_short!("bid_ttl"),),
        (old_days, new_days, admin.clone()),
    );
}

/// Emit an admin-set event.
#[inline]
pub fn emit_admin_set(env: &Env, admin: &Address) {
    env.events().publish(
        (symbol_short!("adm_set"),),
        (admin.clone(),),
    );
}

/// Emit an admin-transferred event.
#[inline]
pub fn emit_admin_transferred(env: &Env, old_admin: &Address, new_admin: &Address) {
    env.events().publish(
        (symbol_short!("adm_xfr"),),
        (old_admin.clone(), new_admin.clone()),
    );
}

/// Emit an admin-initialized event.
#[inline]
pub fn emit_admin_initialized(env: &Env, admin: &Address) {
    env.events().publish(
        (symbol_short!("adm_ini"),),
        (admin.clone(),),
    );
}
