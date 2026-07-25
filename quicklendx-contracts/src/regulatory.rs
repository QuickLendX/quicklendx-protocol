//! Regulatory compliance hook — reserved seam for a future compliance layer.
//!
//! # Purpose
//!
//! [`require_regulatory_ok`] is a **deliberately empty** gate that the protocol
//! calls at every state-changing entry point before committing work. Today it is
//! a pure no-op that always returns `Ok(())`, which means it has zero runtime
//! cost and imposes no new restrictions on existing callers.
//!
//! The seam exists so that a future compliance layer (e.g. on-chain allowlist,
//! off-chain oracle attestation, jurisdiction-based block list) can be dropped
//! in **without touching any other contract module**: only this file changes when
//! the policy is upgraded.
//!
//! # Contract
//!
//! * **Current behaviour (no-op):** always returns `Ok(())`.
//! * **Future behaviour:** may return a compliance-specific error when the actor
//!   is not cleared for participation.
//! * The function signature is intentionally stable and must **not** be changed
//!   without a protocol-level migration.
//!
//! # Call sites
//!
//! | Entry point    | Actor passed  |
//! |----------------|---------------|
//! | `store_invoice` | `business`   |
//! | `place_bid`     | `investor`   |
//!
//! # no_std discipline
//!
//! This module only uses `soroban_sdk` primitives — no `std::` imports.

use crate::errors::QuickLendXError;
use soroban_sdk::{Address, Env};

/// Regulatory compliance gate called before every state-changing operation.
///
/// **Current implementation:** no-op — always returns `Ok(())`.
///
/// Operators wishing to add a compliance layer should replace the body of this
/// function. The signature **must** remain identical so that all call sites
/// continue to compile without modification.
///
/// # Arguments
///
/// * `_env`   – Soroban execution environment (available for oracle calls in a
///              future implementation; unused today).
/// * `_actor` – The address initiating the action (business for `store_invoice`,
///              investor for `place_bid`).
///
/// # Errors
///
/// Always `Ok(())` in the current no-op implementation. A future implementation
/// may return a compliance-specific error variant.
#[inline]
pub fn require_regulatory_ok(
    _env: &Env,
    _actor: &Address,
) -> Result<(), QuickLendXError> {
    Ok(())
}
