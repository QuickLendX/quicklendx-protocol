//! Tests for `payments::require_matching_currency_precision` — the currency-precision
//! helper used as a defence-in-depth guard around every currency-amount entrypoint.
//!
//! Issue #2092 — these tests lock in three behaviour buckets:
//!
//! 1. **Matching** — token contract exposes a `decimals()` entry-point that
//!    returns a value in the allowed range `[0, 18]`. The helper must return
//!    `Ok(())`. Cover: lower bound (`0`), upper bound (`18`), typical SAC
//!    default (`7`).
//!
//! 2. **Over-precision** — `decimals()` returns a value strictly greater than
//!    `18`. The helper must reject the call with `QuickLendXError::InvalidCurrency`
//!    so the contract never silently accepts tokens whose internal scaling
//!    could overflow internal math. Cover: `19`, `20`, `u32::MAX`.
//!
//! 3. **Malformed** — the supplied `currency` address is not a well-formed
//!    SAC-style token. The helper must reject the call with `InvalidCurrency`
//!    (either because the contract is not registered or does not expose
//!    `decimals()`). Cover: a fresh random address (no contract).
//!
//! These tests use **plain `#[cfg(test)]`** with no feature gate so they run
//! on every CI matrix entry, satisfying the issue's acceptance criteria.
//!
//! The mock token contracts below are intentionally minimal: each one
//! defines only the surface required to drive a specific code-path of
//! `require_matching_currency_precision`. They expose **no** `transfer` /
//! `balance` / `allowance` entry-points, which is fine because the helper
//! under test never calls those.

use crate::errors::QuickLendXError;
use crate::payments::require_matching_currency_precision;
use crate::QuickLendXContract;
use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, Address, Env, Symbol,
};

// ============================================================================
// Mock token contracts
// ============================================================================

/// Token that reports `decimals()` = 0 (atomic units).
/// Must be accepted — boundary inclusive at the low end.
#[contract]
pub struct PrecisionZeroToken;

#[contractimpl]
impl PrecisionZeroToken {
    pub fn decimals(_env: Env) -> u32 {
        0
    }
}

/// Token that reports `decimals()` = 18 (the upper bound the protocol allows).
/// Must be accepted — boundary inclusive at the high end.
#[contract]
pub struct PrecisionEighteenToken;

#[contractimpl]
impl PrecisionEighteenToken {
    pub fn decimals(_env: Env) -> u32 {
        18
    }
}

/// Token that reports `decimals()` = 19 (one above the allowed maximum).
/// Must be rejected with `InvalidCurrency`.
#[contract]
pub struct PrecisionNineteenToken;

#[contractimpl]
impl PrecisionNineteenToken {
    pub fn decimals(_env: Env) -> u32 {
        19
    }
}

/// Token that reports `decimals()` = 20 (well above the allowed maximum).
/// Must be rejected with `InvalidCurrency`.
#[contract]
pub struct PrecisionTwentyToken;

#[contractimpl]
impl PrecisionTwentyToken {
    pub fn decimals(_env: Env) -> u32 {
        20
    }
}

/// Token that reports `decimals()` = `u32::MAX`. Ensures the guard rejects
/// even the most extreme over-precision value rather than wrapping or
/// truncating it.
#[contract]
pub struct PrecisionMaxToken;

#[contractimpl]
impl PrecisionMaxToken {
    pub fn decimals(_env: Env) -> u32 {
        u32::MAX
    }
}

/// Token whose `decimals()` entry-point returns a non-`u32` value (a `Symbol`
/// in this case). Kept available for the wrong-return-type malformed test;
/// see `test_precision_rejects_wrong_return_type` for the `#[ignore]` rationale
/// on Soroban 25.x.
#[contract]
pub struct MalformedTypeToken;

#[contractimpl]
impl MalformedTypeToken {
    /// Returns a `Symbol` instead of the expected `u32`. This is the canonical
    /// "wrong return type" malformed case — the helper should still reject the
    /// call rather than silently treating the symbol bytes as a number.
    pub fn decimals(_env: Env) -> Symbol {
        symbol_short!("bad")
    }
}

/// Build a fresh test environment, register the QuickLendX contract, and
/// return the env + contract id so individual tests can run as the contract.
fn precision_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

// ============================================================================
// Matching cases — must succeed
// ============================================================================

/// A contract that reports `decimals() == 0` is valid: amounts are already in
/// whole-token units.
#[test]
fn test_precision_matches_when_decimals_is_zero() {
    let (env, contract_id) = precision_env();
    let token_addr = env.register_contract(None, PrecisionZeroToken);

    let result = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &token_addr, 1_000_000)
    });
    assert_eq!(result, Ok(()));
}

/// A contract that reports `decimals() == 18` is valid: it is the highest
/// precision the protocol accepts, so the boundary must be inclusive.
#[test]
fn test_precision_matches_when_decimals_is_eighteen() {
    let (env, contract_id) = precision_env();
    let token_addr = env.register_contract(None, PrecisionEighteenToken);

    let result = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &token_addr, 1)
    });
    assert_eq!(result, Ok(()));
}

/// The Stellar Asset Contract returns `decimals() == 7` by default — the
/// "happy path" that real production flows take. Sanity-check that the
/// default-precision case is still accepted by the new tests.
#[test]
fn test_precision_matches_sac_default_decimals() {
    let (env, contract_id) = precision_env();
    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let result = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &currency, 1_000_000)
    });
    assert_eq!(result, Ok(()));
}

// ============================================================================
// Over-precision cases — must be rejected
// ============================================================================

/// A contract that reports `decimals() == 19` exceeds the supported maximum
/// and must be rejected so the protocol never silently truncates the
/// trailing digits of large amounts.
#[test]
fn test_precision_rejects_decimals_just_above_max() {
    let (env, contract_id) = precision_env();
    let token_addr = env.register_contract(None, PrecisionNineteenToken);

    let result = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &token_addr, 1_000_000)
    });
    assert_eq!(
        result,
        Err(QuickLendXError::InvalidCurrency),
        "decimals=19 must be rejected as over-precision"
    );
}

/// A contract that reports `decimals() == 20` is also rejected; documents
/// that the guard rejects arbitrary over-precision values, not just `19`.
#[test]
fn test_precision_rejects_decimals_twenty() {
    let (env, contract_id) = precision_env();
    let token_addr = env.register_contract(None, PrecisionTwentyToken);

    let result = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &token_addr, 1_000_000)
    });
    assert_eq!(
        result,
        Err(QuickLendXError::InvalidCurrency),
        "decimals=20 must be rejected as over-precision"
    );
}

/// A contract that reports `decimals() == u32::MAX` must be rejected — this
/// is the saturation case where any arithmetic using `decimals` would
/// overflow immediately.
#[test]
fn test_precision_rejects_decimals_max_u32() {
    let (env, contract_id) = precision_env();
    let token_addr = env.register_contract(None, PrecisionMaxToken);

    let result = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &token_addr, 1_000_000)
    });
    assert_eq!(
        result,
        Err(QuickLendXError::InvalidCurrency),
        "decimals=u32::MAX must be rejected as over-precision"
    );
}

// ============================================================================
// Malformed cases — must be rejected
// ============================================================================

/// A bare address that hosts no contract at all has no `decimals()`
/// entry-point and must be rejected with `InvalidCurrency`. This is the
/// "unregistered token address" malformed case.
#[test]
fn test_precision_rejects_unregistered_address() {
    let (env, contract_id) = precision_env();
    let bogus = Address::generate(&env);

    let result = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &bogus, 1_000_000)
    });
    assert_eq!(
        result,
        Err(QuickLendXError::InvalidCurrency),
        "unregistered address must be rejected"
    );
}

/// A contract that exposes a `decimals` symbol but returns a non-`u32` value
/// (a `Symbol` here) must be rejected. The host surfaces a type-conversion
/// error from `try_invoke_contract::<u32, _>`, which the helper maps to
/// `InvalidCurrency` rather than panicking.
///
/// Marked `#[ignore]` to mirror the existing
/// `test_create_escrow_unregistered_token_address_does_not_succeed` test:
/// Soroban 25.x aborts the cross-contract call at the host level on type
/// mismatches instead of returning a transport-level `Err`. The test is
/// preserved so it documents the intended behaviour and can be enabled once
/// the SDK regression is fixed (or a build-time feature allows it).
// TODO(#2092): re-enable once soroban-sdk stops aborting on cross-contract
// type mismatches (tracked upstream; current pin is 25.1.1 in Cargo.toml).
#[test]
#[ignore = "Soroban 25.x aborts on cross-contract type mismatch; enable when SDK behaviour is fixed"]
fn test_precision_rejects_wrong_return_type() {
    let (env, contract_id) = precision_env();
    let token_addr = env.register_contract(None, MalformedTypeToken);

    let result = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &token_addr, 1_000_000)
    });
    assert_eq!(
        result,
        Err(QuickLendXError::InvalidCurrency),
        "decimals returning wrong type must be rejected"
    );
}

// ============================================================================
// Cross-cutting boundaries
// ============================================================================

/// Over-precision rejection must happen **before** the call ever reaches a
/// token-transfer or amount-arithmetic path: the helper must return
/// `InvalidCurrency` for `decimals > 18` regardless of `amount`, including
/// the smallest representable positive value (`1`). Locks in the
/// order-of-operations: `decimals <= 18` is checked at the end of the
/// helper, after the amount guard.
#[test]
fn test_precision_overprecision_rejection_is_amount_independent() {
    let (env, contract_id) = precision_env();
    let token_addr = env.register_contract(None, PrecisionNineteenToken);

    // Smallest positive amount.
    let r1 = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &token_addr, 1)
    });
    assert_eq!(
        r1,
        Err(QuickLendXError::InvalidCurrency),
        "over-precision rejection must be independent of amount"
    );

    // Largest representable amount.
    let r2 = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &token_addr, i128::MAX)
    });
    assert_eq!(
        r2,
        Err(QuickLendXError::InvalidCurrency),
        "over-precision rejection must be independent of amount"
    );
}

/// Both error buckets — `InvalidAmount` (amount <= 0) and `InvalidCurrency`
/// (token contract issues) — must be returned for **distinct** inputs and
/// must never be confused with each other.
#[test]
fn test_precision_amount_and_currency_errors_are_distinct() {
    let (env, contract_id) = precision_env();
    let eighteen_addr = env.register_contract(None, PrecisionEighteenToken);

    // Negative amount against a valid token -> InvalidAmount, not InvalidCurrency.
    let amount_err = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &eighteen_addr, -1)
    });
    assert_eq!(amount_err, Err(QuickLendXError::InvalidAmount));

    // Zero amount against a valid token -> InvalidAmount, not InvalidCurrency.
    let zero_err = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &eighteen_addr, 0)
    });
    assert_eq!(zero_err, Err(QuickLendXError::InvalidAmount));

    // Valid amount against an over-precision token -> InvalidCurrency, not InvalidAmount.
    let nineteen_addr = env.register_contract(None, PrecisionNineteenToken);
    let precision_err = env.as_contract(&contract_id, || {
        require_matching_currency_precision(&env, &nineteen_addr, 1_000_000)
    });
    assert_eq!(precision_err, Err(QuickLendXError::InvalidCurrency));
}
