//! # Escrow State-Machine Invariant Tests
//!
//! Codifies the escrow lifecycle invariants required by issue #808:
//!
//! - `release_escrow` and `refund_escrow` only transition from `Held`.
//! - Neither operation can be double-executed (terminal states are final).
//! - `release_escrow` and `refund_escrow` are mutually exclusive.
//! - A non-existent escrow is rejected with `StorageKeyNotFound`.
//! - `create_escrow` rejects duplicate creation for the same invoice.
//!
//! ## Security assumptions
//! - `Released` and `Refunded` are terminal: once set, no further fund
//!   movement is possible for that escrow record.
//! - No double-spend: funds are transferred exactly once per escrow.
//! - Storage is updated **after** the token transfer succeeds, so a failed
//!   transfer leaves the escrow in `Held` and the operation is safely retryable.
//!
//! ## Coverage
//! Every public state-transition path in `payments.rs` is exercised:
//! `create_escrow`, `release_escrow`, `refund_escrow`.
//! All error variants reachable from those paths are asserted.

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env};

use crate::errors::QuickLendXError;
use crate::payments::{create_escrow, refund_escrow, release_escrow, EscrowStatus, EscrowStorage};
use crate::QuickLendXContract;

// ============================================================================
// Test helpers
// ============================================================================

/// Register the contract and return its address.
/// All escrow functions require `env.current_contract_address()` which is only
/// set inside `env.as_contract`.
fn register_contract(env: &Env) -> Address {
    env.register(QuickLendXContract, ())
}

/// Register a real Stellar Asset Contract, mint `balance` to `investor` and
/// `business`, and approve the contract to spend on their behalf.
fn setup_token(
    env: &Env,
    investor: &Address,
    business: &Address,
    contract_id: &Address,
    balance: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);

    sac.mint(investor, &balance);
    sac.mint(business, &balance);

    let expiry = env.ledger().sequence() + 10_000;
    tok.approve(investor, contract_id, &balance, &expiry);
    tok.approve(business, contract_id, &balance, &expiry);

    currency
}

/// Build a 32-byte invoice ID from a seed byte.
fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

// ============================================================================
// Invariant 1 – release_escrow only transitions from Held → Released
// ============================================================================

/// `release_escrow` succeeds when escrow is `Held` and transitions to `Released`.
#[test]
fn invariant_release_from_held_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 1_000i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let inv_id = invoice_id(&env, 0x01);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");

        release_escrow(&env, &inv_id).expect("release_escrow must succeed from Held");

        let escrow = EscrowStorage::get_escrow_by_invoice(&env, &inv_id)
            .expect("escrow record must exist");
        assert_eq!(
            escrow.status,
            EscrowStatus::Released,
            "escrow must be Released after release_escrow"
        );
    });
}

/// `refund_escrow` succeeds when escrow is `Held` and transitions to `Refunded`.
#[test]
fn invariant_refund_from_held_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 1_000i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let inv_id = invoice_id(&env, 0x02);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");

        refund_escrow(&env, &inv_id).expect("refund_escrow must succeed from Held");

        let escrow = EscrowStorage::get_escrow_by_invoice(&env, &inv_id)
            .expect("escrow record must exist");
        assert_eq!(
            escrow.status,
            EscrowStatus::Refunded,
            "escrow must be Refunded after refund_escrow"
        );
    });
}

// ============================================================================
// Invariant 2 – terminal states are final (no double-execution)
// ============================================================================

/// Double-release is rejected with `InvalidStatus`.
#[test]
fn invariant_double_release_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 1_000i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let inv_id = invoice_id(&env, 0x03);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");
        release_escrow(&env, &inv_id).expect("first release must succeed");

        let err = release_escrow(&env, &inv_id)
            .expect_err("double-release must be rejected");
        assert_eq!(
            err,
            QuickLendXError::InvalidStatus,
            "double-release must return InvalidStatus"
        );
    });
}

/// Double-refund is rejected with `InvalidStatus`.
#[test]
fn invariant_double_refund_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 1_000i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let inv_id = invoice_id(&env, 0x04);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");
        refund_escrow(&env, &inv_id).expect("first refund must succeed");

        let err = refund_escrow(&env, &inv_id)
            .expect_err("double-refund must be rejected");
        assert_eq!(
            err,
            QuickLendXError::InvalidStatus,
            "double-refund must return InvalidStatus"
        );
    });
}

// ============================================================================
// Invariant 3 – release and refund are mutually exclusive
// ============================================================================

/// After `release_escrow`, calling `refund_escrow` is rejected with `InvalidStatus`.
#[test]
fn invariant_refund_after_release_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 1_000i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let inv_id = invoice_id(&env, 0x05);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");
        release_escrow(&env, &inv_id).expect("release must succeed");

        let err = refund_escrow(&env, &inv_id)
            .expect_err("refund after release must be rejected");
        assert_eq!(
            err,
            QuickLendXError::InvalidStatus,
            "refund after release must return InvalidStatus"
        );
    });
}

/// After `refund_escrow`, calling `release_escrow` is rejected with `InvalidStatus`.
#[test]
fn invariant_release_after_refund_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 1_000i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let inv_id = invoice_id(&env, 0x06);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");
        refund_escrow(&env, &inv_id).expect("refund must succeed");

        let err = release_escrow(&env, &inv_id)
            .expect_err("release after refund must be rejected");
        assert_eq!(
            err,
            QuickLendXError::InvalidStatus,
            "release after refund must return InvalidStatus"
        );
    });
}

// ============================================================================
// Invariant 4 – operations on non-existent escrow are rejected
// ============================================================================

/// `release_escrow` on a non-existent invoice returns `StorageKeyNotFound`.
#[test]
fn invariant_release_nonexistent_escrow_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let inv_id = invoice_id(&env, 0x07);

    env.as_contract(&contract_id, || {
        let err = release_escrow(&env, &inv_id)
            .expect_err("release on missing escrow must fail");
        assert_eq!(
            err,
            QuickLendXError::StorageKeyNotFound,
            "missing escrow must return StorageKeyNotFound"
        );
    });
}

/// `refund_escrow` on a non-existent invoice returns `StorageKeyNotFound`.
#[test]
fn invariant_refund_nonexistent_escrow_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let inv_id = invoice_id(&env, 0x08);

    env.as_contract(&contract_id, || {
        let err = refund_escrow(&env, &inv_id)
            .expect_err("refund on missing escrow must fail");
        assert_eq!(
            err,
            QuickLendXError::StorageKeyNotFound,
            "missing escrow must return StorageKeyNotFound"
        );
    });
}

// ============================================================================
// Invariant 5 – create_escrow rejects duplicate creation (no double-funding)
// ============================================================================

/// A second `create_escrow` for the same invoice is rejected with
/// `InvoiceAlreadyFunded`, preventing double-funding.
#[test]
fn invariant_duplicate_create_escrow_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 1_000i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let inv_id = invoice_id(&env, 0x09);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("first create_escrow must succeed");

        let err = create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect_err("duplicate create_escrow must be rejected");
        assert_eq!(
            err,
            QuickLendXError::InvoiceAlreadyFunded,
            "duplicate escrow must return InvoiceAlreadyFunded"
        );
    });
}

// ============================================================================
// Invariant 6 – create_escrow rejects zero/negative amounts
// ============================================================================

/// `create_escrow` with amount = 0 returns `InvalidAmount`.
#[test]
fn invariant_create_escrow_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    // No real token needed — amount is rejected before any transfer attempt.
    let currency = Address::generate(&env);
    let inv_id = invoice_id(&env, 0x0A);

    env.as_contract(&contract_id, || {
        let err = create_escrow(&env, &inv_id, &investor, &business, 0, &currency)
            .expect_err("zero-amount escrow must be rejected");
        assert_eq!(
            err,
            QuickLendXError::InvalidAmount,
            "zero amount must return InvalidAmount"
        );
    });
}

/// `create_escrow` with a negative amount returns `InvalidAmount`.
#[test]
fn invariant_create_escrow_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let inv_id = invoice_id(&env, 0x0B);

    env.as_contract(&contract_id, || {
        let err = create_escrow(&env, &inv_id, &investor, &business, -500, &currency)
            .expect_err("negative-amount escrow must be rejected");
        assert_eq!(
            err,
            QuickLendXError::InvalidAmount,
            "negative amount must return InvalidAmount"
        );
    });
}

// ============================================================================
// Invariant 7 – token balances reflect state transitions (no double-spend)
// ============================================================================

/// After `release_escrow`, the business receives the escrowed funds and the
/// contract balance returns to zero.
#[test]
fn invariant_release_transfers_funds_to_business() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 2_000i128;
    let initial = amount * 5;
    let currency = setup_token(&env, &investor, &business, &contract_id, initial);
    let tok = token::Client::new(&env, &currency);
    let inv_id = invoice_id(&env, 0x0C);

    let investor_before = tok.balance(&investor);
    let business_before = tok.balance(&business);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");

        // Contract holds the funds after creation
        assert_eq!(
            tok.balance(&contract_id),
            amount,
            "contract must hold escrowed amount after create"
        );
        assert_eq!(
            tok.balance(&investor),
            investor_before - amount,
            "investor balance must decrease by escrow amount"
        );

        release_escrow(&env, &inv_id).expect("release must succeed");

        // After release: contract balance back to 0, business received funds
        assert_eq!(
            tok.balance(&contract_id),
            0,
            "contract balance must be zero after release"
        );
        assert_eq!(
            tok.balance(&business),
            business_before + amount,
            "business must receive escrowed amount after release"
        );
        assert_eq!(
            tok.balance(&investor),
            investor_before - amount,
            "investor balance must not change after release"
        );
    });
}

/// After `refund_escrow`, the investor gets their funds back and the contract
/// balance returns to zero.
#[test]
fn invariant_refund_returns_funds_to_investor() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 3_000i128;
    let initial = amount * 5;
    let currency = setup_token(&env, &investor, &business, &contract_id, initial);
    let tok = token::Client::new(&env, &currency);
    let inv_id = invoice_id(&env, 0x0D);

    let investor_before = tok.balance(&investor);
    let business_before = tok.balance(&business);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");

        assert_eq!(tok.balance(&contract_id), amount);
        assert_eq!(tok.balance(&investor), investor_before - amount);

        refund_escrow(&env, &inv_id).expect("refund must succeed");

        assert_eq!(
            tok.balance(&contract_id),
            0,
            "contract balance must be zero after refund"
        );
        assert_eq!(
            tok.balance(&investor),
            investor_before,
            "investor must be fully refunded"
        );
        assert_eq!(
            tok.balance(&business),
            business_before,
            "business balance must not change after refund"
        );
    });
}

// ============================================================================
// Invariant 8 – escrow record fields are correct after creation
// ============================================================================

/// The stored `Escrow` record has the correct fields and starts in `Held`.
#[test]
fn invariant_escrow_record_fields_correct_after_creation() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 500i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let inv_id = invoice_id(&env, 0x0E);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
            .expect("create_escrow must succeed");

        let escrow = EscrowStorage::get_escrow_by_invoice(&env, &inv_id)
            .expect("escrow must be stored");

        assert_eq!(escrow.invoice_id, inv_id, "invoice_id must match");
        assert_eq!(escrow.investor, investor, "investor must match");
        assert_eq!(escrow.business, business, "business must match");
        assert_eq!(escrow.amount, amount, "amount must match");
        assert_eq!(escrow.currency, currency, "currency must match");
        assert_eq!(
            escrow.status,
            EscrowStatus::Held,
            "new escrow must start in Held"
        );
    });
}

// ============================================================================
// Invariant 9 – independent escrows do not interfere
// ============================================================================

/// Two escrows for different invoices are fully independent: releasing one
/// does not affect the other.
#[test]
fn invariant_independent_escrows_do_not_interfere() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 1_000i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 20);
    let inv_a = invoice_id(&env, 0x0F);
    let inv_b = invoice_id(&env, 0x10);

    env.as_contract(&contract_id, || {
        create_escrow(&env, &inv_a, &investor, &business, amount, &currency)
            .expect("create escrow A");
        create_escrow(&env, &inv_b, &investor, &business, amount, &currency)
            .expect("create escrow B");

        // Release A
        release_escrow(&env, &inv_a).expect("release A must succeed");

        // B must still be Held
        let escrow_b = EscrowStorage::get_escrow_by_invoice(&env, &inv_b)
            .expect("escrow B must exist");
        assert_eq!(
            escrow_b.status,
            EscrowStatus::Held,
            "escrow B must remain Held after releasing A"
        );

        // Refund B must still work
        refund_escrow(&env, &inv_b).expect("refund B must succeed");

        let escrow_b_after = EscrowStorage::get_escrow_by_invoice(&env, &inv_b)
            .expect("escrow B must still exist");
        assert_eq!(escrow_b_after.status, EscrowStatus::Refunded);
    });
}

// ============================================================================
// Invariant 10 – escrow ID lookup is consistent
// ============================================================================

/// `get_escrow_by_invoice` and `get_escrow` (by ID) return the same record.
#[test]
fn invariant_escrow_lookup_by_id_and_invoice_consistent() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = register_contract(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let amount = 750i128;
    let currency = setup_token(&env, &investor, &business, &contract_id, amount * 10);
    let inv_id = invoice_id(&env, 0x11);

    env.as_contract(&contract_id, || {
        let escrow_id =
            create_escrow(&env, &inv_id, &investor, &business, amount, &currency)
                .expect("create_escrow must succeed");

        let by_invoice = EscrowStorage::get_escrow_by_invoice(&env, &inv_id)
            .expect("lookup by invoice must succeed");
        let by_id = EscrowStorage::get_escrow(&env, &escrow_id)
            .expect("lookup by escrow_id must succeed");

        assert_eq!(
            by_invoice.escrow_id, by_id.escrow_id,
            "both lookups must return the same escrow"
        );
        assert_eq!(by_invoice.status, EscrowStatus::Held);
        assert_eq!(by_id.status, EscrowStatus::Held);
    });
}
