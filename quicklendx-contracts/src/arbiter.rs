//! Dispute arbiter registry.
//!
//! # Why a separate arbiter registry?
//!
//! Admin authority controls protocol configuration (fee schedule, listings,
//! treasury, pause, upgrade). It is intentionally **privileged but broad** —
//! losing an admin key is bad for the protocol, but losing an arbiter key is
//! catastrophic for the users it can hurt: every dispute resolution moves
//! escrowed funds. Conflating the two roles lets a single compromised admin
//! key drain every disputed investment.
//!
//! The arbiter registry splits those concerns: only addresses that have been
//! **explicitly registered** as arbiters may drive the dispute lifecycle.
//! Removing admin authority no longer removes dispute authority (and vice
//! versa), so a plane-key rotation on either side does not silently transfer
//! the other.
//!
//! # Storage layout
//!
//! * `arbiters` — `Vec<Address>` of registered arbiters, instance storage.
//! * Each registered arbiter is also stored as a `bool` flag under
//!   `(ARBITER_FLAG_KEY, address)` so membership checks are O(1).
//!
//! Both are kept in sync by `register_arbiter` / `unregister_arbiter`.

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, Symbol, Vec};

const ARBITERS_KEY: Symbol = symbol_short!("arbiters");
const ARBITER_FLAG_KEY: Symbol = symbol_short!("arb_flag");

/// Storage layer for the dispute-arbiter registry.
///
/// All write operations require admin authorization (`AdminStorage::require_admin_auth`);
/// membership checks (`is_arbiter`, `require_dispute_arbiter`) only require
/// the address itself and are therefore callable from any dispute
/// entrypoint.
pub struct ArbiterStorage;

impl ArbiterStorage {
    /// Return every currently registered arbiter address.
    pub fn list_arbiters(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&ARBITERS_KEY)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Membership test: O(1) flag lookup, no allocation.
    pub fn is_arbiter(env: &Env, address: &Address) -> bool {
        env.storage()
            .instance()
            .get(&(ARBITER_FLAG_KEY, address.clone()))
            .unwrap_or(false)
    }

    /// Register a new dispute arbiter (admin only).
    ///
    /// Idempotent — re-registering an existing arbiter is a no-op and does
    /// **not** emit a duplicate-registration error. Audit trails come from
    /// the `arbiter_registered` event emitted below.
    pub fn register_arbiter(
        env: &Env,
        admin: &Address,
        arbiter: &Address,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin_auth(env, admin)?;

        if Self::is_arbiter(env, arbiter) {
            return Ok(());
        }

        // Update the O(1) flag first so a concurrent listing read sees a
        // consistent view: either the address is fully registered (flag + list)
        // or not at all.
        env.storage()
            .instance()
            .set(&(ARBITER_FLAG_KEY, arbiter.clone()), &true);

        let mut arbiters = Self::list_arbiters(env);
        arbiters.push_back(arbiter.clone());
        env.storage().instance().set(&ARBITERS_KEY, &arbiters);

        crate::events::arbiter_registered(env, admin, arbiter);
        Ok(())
    }

    /// Revoke a registered dispute arbiter (admin only).
    ///
    /// Returns `OperationNotAllowed` if the address was never registered; this
    /// surfaces accidental no-op revocations to monitoring rather than letting
    /// them slip by as silent successes.
    pub fn unregister_arbiter(
        env: &Env,
        admin: &Address,
        arbiter: &Address,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin_auth(env, admin)?;

        if !Self::is_arbiter(env, arbiter) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        env.storage()
            .instance()
            .remove(&(ARBITER_FLAG_KEY, arbiter.clone()));

        let current = Self::list_arbiters(env);
        let mut next: Vec<Address> = Vec::new(env);
        for entry in current.iter() {
            if entry != *arbiter {
                next.push_back(entry);
            }
        }
        env.storage().instance().set(&ARBITERS_KEY, &next);

        crate::events::arbiter_revoked(env, admin, arbiter);
        Ok(())
    }

    /// Guard: `address` must be a registered dispute arbiter.
    ///
    /// Distinct from [`AdminStorage::require_admin`]: admin status controls
    /// protocol configuration; arbiter status controls dispute resolution.
    /// They are deliberately not the same authority.
    pub fn require_dispute_arbiter(env: &Env, address: &Address) -> Result<(), QuickLendXError> {
        if !Self::is_arbiter(env, address) {
            return Err(QuickLendXError::NotArbiter);
        }
        Ok(())
    }
}
