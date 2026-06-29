//! Dust transfer tests: token transfers below MIN_TRANSFER are rejected with typed errors.
//!
//! Issue #1569: Add dust handling tests for token transfers — amounts below MIN_TRANSFER
//! rejected with typed error.
//!
//! This module documents and verifies the boundary behavior of token transfer amounts
//! that fall below the protocol's minimum transfer threshold. All tests use deterministic
//! amounts and no randomization, ensuring stability and reproducibility.

use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env};

use crate::errors::QuickLendXError;
use crate::payments::transfer_funds;
use crate::QuickLendXContract;

// ============================================================================
// Constants: Matching the protocol's test-mode MIN_TRANSFER boundary
// ============================================================================

/// The protocol's minimum transfer amount in test mode (defined in protocol_limits.rs).
/// In production, this is 1_000_000 (1 token with 6 decimals).
/// In test mode, it is 10 to keep tests efficient.
const MIN_TRANSFER: i128 = 10;

// ============================================================================
// Test Helpers
// ============================================================================

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

/// Register a SAC token, mint to addresses, and optionally approve the contract.
fn setup_token(
    env: &Env,
    contract_id: &Address,
    mint_to: &[(Address, i128)],
    approve: &[(Address, i128)],
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let token_client = token::Client::new(env, &currency);

    for (addr, amount) in mint_to {
        sac_client.mint(addr, amount);
    }

    let expiration = env.ledger().sequence() + 10_000;
    for (addr, amount) in approve {
        token_client.approve(addr, contract_id, amount, &expiration);
    }

    currency
}

// ============================================================================
// Test: transfer_below_min_transfer_returns_typed_error
// ============================================================================

/// Transfer of amount = MIN_TRANSFER - 1 is rejected with the typed dust error.
///
/// This test verifies that amounts just below the minimum are properly caught
/// by the dust check and return the expected typed error variant.
#[test]
fn transfer_below_min_transfer_returns_typed_error() {
    let (env, contract_id) = setup();
    let from = Address::generate(&env);
    let to = Address::generate(&env);

    // Set up sender with sufficient balance and allowance
    let currency = setup_token(
        &env,
        &contract_id,
        &[(from.clone(), 1_000_000)], // plenty of balance
        &[(from.clone(), 1_000_000)], // plenty of allowance
    );
    let token_client = token::Client::new(&env, &currency);

    let from_balance_before = token_client.balance(&from);
    let to_balance_before = token_client.balance(&to);

    // Attempt transfer with amount = MIN_TRANSFER - 1
    let amount_below_min = MIN_TRANSFER - 1;
    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &from, &to, amount_below_min)
    });

    // Verify typed error returned
    assert_eq!(result, Err(QuickLendXError::InvalidAmount));

    // Verify no state change occurred
    assert_eq!(token_client.balance(&from), from_balance_before);
    assert_eq!(token_client.balance(&to), to_balance_before);
}

// ============================================================================
// Test: transfer_at_min_transfer_succeeds
// ============================================================================

/// Transfer of exactly MIN_TRANSFER is accepted (boundary happy path).
///
/// This test verifies that the minimum transfer amount is accepted
/// and the funds are correctly transferred.
#[test]
fn transfer_at_min_transfer_succeeds() {
    let (env, contract_id) = setup();
    let from = Address::generate(&env);
    let to = Address::generate(&env);

    let currency = setup_token(
        &env,
        &contract_id,
        &[(from.clone(), 1_000_000)],
        &[(from.clone(), 1_000_000)],
    );
    let token_client = token::Client::new(&env, &currency);

    let from_balance_before = token_client.balance(&from);
    let to_balance_before = token_client.balance(&to);

    // Transfer exactly MIN_TRANSFER
    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &from, &to, MIN_TRANSFER)
    });

    // Verify success
    assert_eq!(result, Ok(()));

    // Verify correct balances after transfer
    assert_eq!(
        token_client.balance(&from),
        from_balance_before - MIN_TRANSFER
    );
    assert_eq!(token_client.balance(&to), to_balance_before + MIN_TRANSFER);
}

// ============================================================================
// Test: transfer_of_zero_returns_typed_error
// ============================================================================

/// Zero-amount transfer is rejected with the typed dust error.
///
/// This test ensures that transfers with amount = 0 are caught early
/// as invalid amounts before any token operations.
#[test]
fn transfer_of_zero_returns_typed_error() {
    let (env, contract_id) = setup();
    let from = Address::generate(&env);
    let to = Address::generate(&env);

    let currency = setup_token(
        &env,
        &contract_id,
        &[(from.clone(), 1_000_000)],
        &[(from.clone(), 1_000_000)],
    );
    let token_client = token::Client::new(&env, &currency);

    let from_balance_before = token_client.balance(&from);
    let to_balance_before = token_client.balance(&to);

    // Attempt transfer with amount = 0
    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &from, &to, 0)
    });

    // Verify typed error returned
    assert_eq!(result, Err(QuickLendXError::InvalidAmount));

    // Verify no state change occurred
    assert_eq!(token_client.balance(&from), from_balance_before);
    assert_eq!(token_client.balance(&to), to_balance_before);
}

// ============================================================================
// Test: transfer_of_one_returns_typed_error
// ============================================================================

/// Amount = 1 is below MIN_TRANSFER and is rejected with typed error.
///
/// This test documents the boundary case where the smallest positive
/// amount is below the minimum and should be rejected.
/// This test is meaningful when MIN_TRANSFER > 1.
#[test]
fn transfer_of_one_returns_typed_error() {
    let (env, contract_id) = setup();
    let from = Address::generate(&env);
    let to = Address::generate(&env);

    let currency = setup_token(
        &env,
        &contract_id,
        &[(from.clone(), 1_000_000)],
        &[(from.clone(), 1_000_000)],
    );
    let token_client = token::Client::new(&env, &currency);

    let from_balance_before = token_client.balance(&from);
    let to_balance_before = token_client.balance(&to);

    // Attempt transfer with amount = 1 (which is < MIN_TRANSFER = 10 in test mode)
    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &from, &to, 1)
    });

    // Verify typed error returned
    assert_eq!(result, Err(QuickLendXError::InvalidAmount));

    // Verify no state change occurred
    assert_eq!(token_client.balance(&from), from_balance_before);
    assert_eq!(token_client.balance(&to), to_balance_before);
}

// ============================================================================
// Test: transfer_above_min_transfer_succeeds
// ============================================================================

/// Transfer well above MIN_TRANSFER succeeds (regression guard).
///
/// This test ensures that transfers above the minimum threshold work correctly
/// and guards against regressions in the minimum amount boundary enforcement.
#[test]
fn transfer_above_min_transfer_succeeds() {
    let (env, contract_id) = setup();
    let from = Address::generate(&env);
    let to = Address::generate(&env);

    let currency = setup_token(
        &env,
        &contract_id,
        &[(from.clone(), 1_000_000)],
        &[(from.clone(), 1_000_000)],
    );
    let token_client = token::Client::new(&env, &currency);

    let from_balance_before = token_client.balance(&from);
    let to_balance_before = token_client.balance(&to);

    // Transfer well above MIN_TRANSFER: MIN_TRANSFER * 10 = 100
    let amount_above_min = MIN_TRANSFER * 10;
    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &from, &to, amount_above_min)
    });

    // Verify success
    assert_eq!(result, Ok(()));

    // Verify correct balances after transfer
    assert_eq!(
        token_client.balance(&from),
        from_balance_before - amount_above_min
    );
    assert_eq!(
        token_client.balance(&to),
        to_balance_before + amount_above_min
    );
}

// ============================================================================
// Property-based test: any amount below MIN_TRANSFER is rejected
// ============================================================================

#[cfg(feature = "fuzz-tests")]
mod props {
    use super::*;
    use proptest::prelude::*;

    /// Property test: any amount in [0, MIN_TRANSFER) is rejected with InvalidAmount error.
    ///
    /// This property test verifies that the dust boundary is enforced uniformly
    /// across the entire range of values below MIN_TRANSFER. No seed failures
    /// are expected; if one occurs, it will be committed as a regression seed
    /// to proptest-regressions/.
    proptest! {
        #[test]
        fn any_amount_below_min_transfer_is_rejected(
            amount in 0i128..MIN_TRANSFER
        ) {
            let (env, contract_id) = setup();
            let from = Address::generate(&env);
            let to = Address::generate(&env);

            let currency = setup_token(
                &env,
                &contract_id,
                &[(from.clone(), 1_000_000_000)],
                &[(from.clone(), 1_000_000_000)],
            );

            let from_balance_before = token::Client::new(&env, &currency).balance(&from);

            let result = env.as_contract(&contract_id, || {
                transfer_funds(&env, &currency, &from, &to, amount)
            });

            // All amounts below MIN_TRANSFER must be rejected with InvalidAmount
            prop_assert_eq!(result, Err(QuickLendXError::InvalidAmount));

            // Verify no state change
            let from_balance_after = token::Client::new(&env, &currency).balance(&from);
            prop_assert_eq!(from_balance_after, from_balance_before);
        }
    }

    /// Property test: any amount >= MIN_TRANSFER (up to a reasonable limit) succeeds.
    ///
    /// This property test verifies that amounts at or above the minimum threshold
    /// are accepted and processed correctly.
    proptest! {
        #[test]
        fn any_amount_at_or_above_min_transfer_succeeds(
            amount in MIN_TRANSFER..=(MIN_TRANSFER * 1_000)
        ) {
            let (env, contract_id) = setup();
            let from = Address::generate(&env);
            let to = Address::generate(&env);

            let currency = setup_token(
                &env,
                &contract_id,
                &[(from.clone(), 1_000_000_000)],
                &[(from.clone(), 1_000_000_000)],
            );
            let token_client = token::Client::new(&env, &currency);

            let from_balance_before = token_client.balance(&from);
            let to_balance_before = token_client.balance(&to);

            let result = env.as_contract(&contract_id, || {
                transfer_funds(&env, &currency, &from, &to, amount)
            });

            // All amounts >= MIN_TRANSFER must succeed
            prop_assert_eq!(result, Ok(()));

            // Verify correct balances
            let from_balance_after = token_client.balance(&from);
            let to_balance_after = token_client.balance(&to);
            prop_assert_eq!(from_balance_after, from_balance_before - amount);
            prop_assert_eq!(to_balance_after, to_balance_before + amount);
        }
    }
}
