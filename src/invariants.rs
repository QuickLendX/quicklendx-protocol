use soroban_sdk::{Env};

use crate::settlement::{compute_settlement, verify_conservation, MAX_FACE_VALUE};

/// Solvency invariant:
///
/// Ensures that total investor payouts derived from settlement logic
/// never exceed the total available funds.
///
/// This is enforced using `compute_settlement` and `verify_conservation`.
///
/// # Security
/// Any violation is classified as **P0 (Critical)**:
/// - Indicates broken accounting
/// - Could enable over-withdrawal or fund leakage
/// 🚨 P0 if violated
pub fn validate_solvency_invariant(
    face: u128,
    funded: u128,
    fee_bps: u128,
    penalty_bps: u128,
) {
    if let Some(result) = compute_settlement(face, funded, fee_bps, penalty_bps) {
        assert!(
            verify_conservation(&result),
            "P0: Solvency invariant violated"
        );
    }
}