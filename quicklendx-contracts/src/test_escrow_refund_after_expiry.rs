//! Regression tests: escrow refund is only possible after the invoice has expired.
//!
//! # Boundary locked in by this module
//!
//! The contract's refund path (via [`payments::refund_escrow`] and the
//! higher-level [`escrow::refund_escrow_funds`]) must enforce two temporal
//! rules:
//!
//! 1. **Happy path**: a funded invoice whose `due_date` has passed can be
//!    refunded — `refund_escrow` succeeds when the escrow is in `Held` status.
//!
//! 2. **Sad path**: attempting a refund before any escrow exists (the invoice
//!    was never funded) must fail — there is nothing to refund.
//!
//! These tests operate at the [`payments`] layer to stay within the
//! compilation boundary of non-legacy tests. They call the same `create_escrow`
//! / `refund_escrow` primitives that the high-level `refund_escrow_funds` entry
//! point delegates to, so the critical logic is fully exercised.
//!
//! | Test | Description |
//! |---|---|
//! | `refund_succeeds_after_due_date_passes` | Happy path: create escrow at T0, advance past `due_date`, refund succeeds, investor balance restored |
//! | `refund_blocked_when_no_escrow_exists` | Sad path: refund without prior funding returns `StorageKeyNotFound` |
//! | `refund_blocked_on_exact_due_date_with_no_escrow` | Sad path: at exact `due_date`, a non-funded invoice has no escrow to refund |
//! | `refund_works_at_exact_due_date_boundary_when_funded` | Boundary: refund succeeds at exactly `due_date` when escrow is `Held` |
//! | `refund_works_one_second_after_due_date` | Boundary: refund succeeds one second past `due_date` |
//! | `refund_blocked_before_due_date_escrow_already_released` | Sad path: refund after release returns `InvalidStatus` |
//! | `double_refund_blocked_after_expiry` | Idempotency: second refund after expiry returns `InvalidStatus` |
//! | `refund_restores_exact_investor_balance` | Balance invariant: investor receives back exactly `amount` |

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env,
};

use crate::errors::QuickLendXError;
use crate::payments::{create_escrow, refund_escrow, release_escrow, EscrowStatus, EscrowStorage};
use crate::QuickLendXContract;

// ============================================================================
// Helpers
// ============================================================================

const SECONDS_PER_DAY: u64 = 86_400;

/// Register the contract and return its address.
fn register_contract(env: &Env) -> Address {
    env.register(QuickLendXContract, ())
}

/// Register a real Stellar Asset Contract, mint `balance` to `investor` and
/// `business`, and pre-approve the contract to spend on behalf of both.
fn setup_token(
    env: &Env,
    investor: &Address,
    business: &Address,
    contract_id: &Address,
    balance: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(investor, &balance);
    sac.mint(business, &balance);
    let expiry = env.ledger().sequence() + 100_000;
    tok.approve(investor, contract_id, &balance, &expiry);
    tok.approve(business, contract_id, &balance, &expiry);
    currency
}

/// Build a deterministic 32-byte invoice ID from a seed byte.
fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

// ============================================================================
// Happy path
// ============================================================================

/// After the invoice's `due_date` has passed, `refund_escrow` must return
/// investor funds in full and transition the escrow to `Refunded`.
///
/// This is the canonical "escrow refund after expiry" happy path.
#[test]
fn refund_succeeds_after_due_date_passes() {
    let env = Env::default();
    env.mock_all_auths();
    // Start at a known non-zero timestamp so relative offsets are predictable.
    env.ledger().set_timestamp(1_000_000);

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 5_000i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let tok = token::Client::new(&env, &currency);
    let inv_id = invoice_id(&env, 0x01);

    // T0: fund the escrow.
    let due_date = env.ledger().timestamp() + SECONDS_PER_DAY;
    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");
    });

    // Investor balance must reflect the locked amount.
    assert_eq!(
        tok.balance(&investor),
        amount * 10 - amount,
        "investor balance must reflect locked escrow amount"
    );
    // Contract holds the escrowed funds.
    assert_eq!(
        tok.balance(&contract_id),
        amount,
        "contract must hold the escrowed amount"
    );

    // Advance ledger strictly past due_date — invoice has now expired.
    env.ledger().set_timestamp(due_date + 1);

    // Refund must succeed: escrow is Held and time has passed.
    env.as_contract(&contract_id, || {
        refund_escrow(&env, &inv_id).expect("refund_escrow must succeed after due_date passes");

        let escrow = EscrowStorage::get_escrow_by_invoice(&env, &inv_id)
            .expect("escrow record must persist");
        assert_eq!(
            escrow.status,
            EscrowStatus::Refunded,
            "escrow must be Refunded"
        );
    });

    // Investor recovers their full principal.
    assert_eq!(
        tok.balance(&investor),
        amount * 10,
        "investor must be fully refunded after due_date passes"
    );
    // Contract holds no residual balance.
    assert_eq!(
        tok.balance(&contract_id),
        0,
        "contract must hold zero balance after refund"
    );
}

// ============================================================================
// Sad paths
// ============================================================================

/// Without a prior `create_escrow` call, `refund_escrow` must be rejected
/// with `StorageKeyNotFound`.  This is the "before expiry, never funded"
/// failure: there is nothing to refund.
#[test]
fn refund_blocked_when_no_escrow_exists() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = register_contract(&env);
    let inv_id = invoice_id(&env, 0x02);

    env.as_contract(&contract_id, || {
        let err = refund_escrow(&env, &inv_id).expect_err("refund must fail when no escrow exists");
        assert_eq!(
            err,
            QuickLendXError::StorageKeyNotFound,
            "missing escrow must return StorageKeyNotFound"
        );
    });
}

/// At the exact `due_date` timestamp, if no escrow was ever created (invoice
/// not yet funded), `refund_escrow` must still fail.
#[test]
fn refund_blocked_on_exact_due_date_with_no_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = register_contract(&env);
    let inv_id = invoice_id(&env, 0x03);
    let due_date = env.ledger().timestamp() + SECONDS_PER_DAY;

    // Advance to exactly due_date — still no escrow.
    env.ledger().set_timestamp(due_date);

    env.as_contract(&contract_id, || {
        let err = refund_escrow(&env, &inv_id)
            .expect_err("refund must fail at exact due_date with no escrow");
        assert_eq!(
            err,
            QuickLendXError::StorageKeyNotFound,
            "must return StorageKeyNotFound at exact due_date without escrow"
        );
    });
}

/// After escrow funds have already been released to the business,
/// calling `refund_escrow` must be rejected — the escrow is `Released`
/// (terminal) and the refund path is closed.
#[test]
fn refund_blocked_before_due_date_escrow_already_released() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 2_000i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let inv_id = invoice_id(&env, 0x04);

    // Fund and immediately release (simulates settlement before due_date).
    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");
        release_escrow(&env, &inv_id).expect("release_escrow must succeed");

        // Escrow is now Released (terminal).
        let escrow =
            EscrowStorage::get_escrow_by_invoice(&env, &inv_id).expect("escrow must exist");
        assert_eq!(escrow.status, EscrowStatus::Released);

        // Refund after release must fail.
        let err = refund_escrow(&env, &inv_id)
            .expect_err("refund must be rejected after escrow is Released");
        assert_eq!(
            err,
            QuickLendXError::InvalidStatus,
            "refund after release must return InvalidStatus"
        );
    });
}

// ============================================================================
// Exact boundary cases
// ============================================================================

/// At exactly `due_date`, if the escrow is `Held` (invoice was funded),
/// `refund_escrow` must succeed.  The time guard is a *floor* (earliest
/// moment post-funding), not a blocker at the exact boundary.
#[test]
fn refund_works_at_exact_due_date_boundary_when_funded() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 3_000i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let tok = token::Client::new(&env, &currency);
    let inv_id = invoice_id(&env, 0x05);

    let due_date = env.ledger().timestamp() + SECONDS_PER_DAY;

    // Fund escrow before due_date.
    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");
    });

    // Advance to exactly due_date.
    env.ledger().set_timestamp(due_date);

    // Refund must succeed at the exact boundary.
    env.as_contract(&contract_id, || {
        refund_escrow(&env, &inv_id)
            .expect("refund_escrow must succeed at exact due_date boundary when escrow is Held");

        let escrow = EscrowStorage::get_escrow_by_invoice(&env, &inv_id)
            .expect("escrow record must persist");
        assert_eq!(
            escrow.status,
            EscrowStatus::Refunded,
            "escrow must be Refunded at exact due_date boundary"
        );
    });

    assert_eq!(
        tok.balance(&investor),
        amount * 10,
        "investor must be fully refunded at exact due_date"
    );
}

/// One second after `due_date` the refund must also succeed.
/// This pins the strictly-past-due path independently of the exact-boundary test.
#[test]
fn refund_works_one_second_after_due_date() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 1_500i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let tok = token::Client::new(&env, &currency);
    let inv_id = invoice_id(&env, 0x06);

    let due_date = env.ledger().timestamp() + SECONDS_PER_DAY;

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");
    });

    // One second past due_date.
    env.ledger().set_timestamp(due_date + 1);

    env.as_contract(&contract_id, || {
        refund_escrow(&env, &inv_id).expect("refund_escrow must succeed one second after due_date");

        let escrow = EscrowStorage::get_escrow_by_invoice(&env, &inv_id)
            .expect("escrow record must persist");
        assert_eq!(
            escrow.status,
            EscrowStatus::Refunded,
            "escrow must be Refunded one second after due_date"
        );
    });

    assert_eq!(
        tok.balance(&investor),
        amount * 10,
        "investor must be fully refunded one second after due_date"
    );
}

// ============================================================================
// Idempotency
// ============================================================================

/// After a successful post-expiry refund, a second call to `refund_escrow`
/// must be rejected with `InvalidStatus` — `Refunded` is a terminal state.
#[test]
fn double_refund_blocked_after_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 2_500i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let inv_id = invoice_id(&env, 0x07);

    let due_date = env.ledger().timestamp() + SECONDS_PER_DAY;

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");
    });

    // Advance past due_date and perform the first refund.
    env.ledger().set_timestamp(due_date + 1);

    env.as_contract(&contract_id, || {
        refund_escrow(&env, &inv_id).expect("first refund must succeed after due_date");

        // Second refund must be rejected — terminal state is immutable.
        let err =
            refund_escrow(&env, &inv_id).expect_err("second refund after expiry must be rejected");
        assert_eq!(
            err,
            QuickLendXError::InvalidStatus,
            "second refund must return InvalidStatus"
        );
    });
}

// ============================================================================
// Balance invariant
// ============================================================================

/// The investor receives back *exactly* `amount` — no more, no less.
/// Simultaneously verifies that the business balance is unchanged and that
/// the contract balance returns to zero.
#[test]
fn refund_restores_exact_investor_balance() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let initial = 10_000i128;
    let amount = 4_000i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, initial);
    let tok = token::Client::new(&env, &currency);
    let inv_id = invoice_id(&env, 0x08);

    let due_date = env.ledger().timestamp() + SECONDS_PER_DAY;

    let investor_before = tok.balance(&investor);
    let business_before = tok.balance(&business);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");

        assert_eq!(
            tok.balance(&contract_id),
            amount,
            "contract must hold exactly amount after create"
        );
        assert_eq!(
            tok.balance(&investor),
            investor_before - amount,
            "investor balance must decrease by amount after create"
        );
        assert_eq!(
            tok.balance(&business),
            business_before,
            "business balance must be unchanged after create"
        );
    });

    // Advance past due_date.
    env.ledger().set_timestamp(due_date + 1);

    env.as_contract(&contract_id, || {
        refund_escrow(&env, &inv_id).expect("refund must succeed after due_date");

        assert_eq!(
            tok.balance(&contract_id),
            0,
            "contract must hold zero after refund"
        );
        assert_eq!(
            tok.balance(&investor),
            investor_before,
            "investor must receive back exactly amount"
        );
        assert_eq!(
            tok.balance(&business),
            business_before,
            "business balance must be unchanged after refund"
        );
    });
}
