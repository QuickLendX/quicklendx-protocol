//! Regression tests: bid acceptance must be rejected when the invoice has
//! passed its due date (escrow would be created for an already-expired invoice).
//!
//! # Threat
//! Without the temporal guard in `load_accept_bid_context`, a business could
//! accept a bid on an invoice whose `due_date` is already in the past.  This
//! would lock investor funds into an obligation that the business cannot
//! settle on time, effectively trapping the investment.  Since bids can be
//! placed before the due date (which is valid), the critical missing check
//! is at acceptance time: once the due date passes, the business must not
//! be able to create a new escrow.
//!
//! These tests are NOT feature-gated so they run on every CI matrix entry.

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::payments::EscrowStorage;
use crate::storage::InvoiceStorage;
use crate::types::InvoiceStatus;
use crate::QuickLendXContract;

// ============================================================================
// Helpers
// ============================================================================

fn setup_env() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Initialize admin
    env.as_contract(&contract_id, || {
        crate::admin::AdminStorage::initialize(&env, &admin).unwrap();
    });

    (env, contract_id, admin, business, investor)
}

fn setup_token(
    env: &Env,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);

    sac.mint(business, &10_000i128);
    sac.mint(investor, &10_000i128);
    let expiry = env.ledger().sequence() + 100_000;
    tok.approve(business, contract_id, &10_000i128, &expiry);
    tok.approve(investor, contract_id, &10_000i128, &expiry);

    currency
}

// ============================================================================
// Test: bid acceptance blocked when invoice due_date has passed
// ============================================================================

/// A bid placed before the due date must be rejected at acceptance time if
/// the invoice `due_date` has already passed.
#[test]
fn accept_bid_blocked_when_invoice_is_expired() {
    let (env, contract_id, _admin, business, investor) = setup_env();
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let due_date = env.ledger().timestamp() + 86_400; // 1 day from now

    let invoice_id = env.as_contract(&contract_id, || {
        // Create invoice with a future due_date
        let invoice = crate::types::Invoice::new(
            &env,
            business.clone(),
            1_000i128,
            currency.clone(),
            due_date,
            String::from_str(&env, "Test invoice"),
            InvoiceCategory::Services,
            Vec::new(&env),
            None,
        )
        .unwrap();
        let id = invoice.id.clone();
        InvoiceStorage::store_invoice(&env, &invoice);
        id
    });

    // Verify invoice
    env.as_contract(&contract_id, || {
        let mut inv = InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();
        inv.verify(&env, business.clone());
        InvoiceStorage::update_invoice(&env, &inv);
    });
    assert_eq!(
        env.as_contract(&contract_id, || {
            InvoiceStorage::get_invoice(&env, &invoice_id)
                .unwrap()
                .status
        }),
        InvoiceStatus::Verified
    );

    // Store a bid (bypass the public entrypoint to avoid the due_date check in validate_bid)
    let bid_id = env.as_contract(&contract_id, || {
        let bid = crate::types::Bid {
            bid_id: crate::bid::BidStorage::generate_unique_bid_id(&env),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: 1_000i128,
            expected_return: 1_200i128,
            timestamp: env.ledger().timestamp(),
            status: crate::types::BidStatus::Placed,
            expiration_timestamp: env.ledger().timestamp() + 604_800,
        };
        let id = bid.bid_id.clone();
        crate::bid::BidStorage::store_bid(&env, &bid);
        crate::bid::BidStorage::add_bid_to_invoice(&env, &invoice_id, &id);
        id
    });

    // Advance time past due_date
    env.ledger().set_timestamp(due_date + 1);

    // Accepting the bid must now fail — the invoice has expired
    env.as_contract(&contract_id, || {
        let err = crate::escrow::load_accept_bid_context(&env, &invoice_id, &bid_id)
            .expect_err("accepting bid on expired invoice must fail");
        assert_eq!(
            err,
            QuickLendXError::OperationNotAllowed,
            "must return OperationNotAllowed when invoice due_date has passed"
        );

        // Verify no escrow was created
        assert!(
            EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none(),
            "no escrow must exist after rejected acceptance"
        );

        // Verify invoice state is unchanged
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();
        assert_eq!(invoice.status, InvoiceStatus::Verified, "invoice must remain Verified");
        assert_eq!(invoice.funded_amount, 0, "invoice must not be funded");
    });
}

/// A bid accepted before the due_date must still succeed (happy path guard).
#[test]
fn accept_bid_succeeds_before_due_date() {
    let (env, contract_id, _admin, business, investor) = setup_env();
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let due_date = env.ledger().timestamp() + 86_400; // 1 day from now

    let invoice_id = env.as_contract(&contract_id, || {
        let invoice = crate::types::Invoice::new(
            &env,
            business.clone(),
            1_000i128,
            currency.clone(),
            due_date,
            String::from_str(&env, "Test invoice"),
            InvoiceCategory::Services,
            Vec::new(&env),
            None,
        )
        .unwrap();
        let id = invoice.id.clone();
        InvoiceStorage::store_invoice(&env, &invoice);
        id
    });

    // Verify invoice
    env.as_contract(&contract_id, || {
        let mut inv = InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();
        inv.verify(&env, business.clone());
        InvoiceStorage::update_invoice(&env, &inv);
    });

    // Store a bid
    let bid_id = env.as_contract(&contract_id, || {
        let bid = crate::types::Bid {
            bid_id: crate::bid::BidStorage::generate_unique_bid_id(&env),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: 1_000i128,
            expected_return: 1_200i128,
            timestamp: env.ledger().timestamp(),
            status: crate::types::BidStatus::Placed,
            expiration_timestamp: env.ledger().timestamp() + 604_800,
        };
        let id = bid.bid_id.clone();
        crate::bid::BidStorage::store_bid(&env, &bid);
        crate::bid::BidStorage::add_bid_to_invoice(&env, &invoice_id, &id);
        id
    });

    // Accept while still before due_date — must succeed
    env.as_contract(&contract_id, || {
        let ctx = crate::escrow::load_accept_bid_context(&env, &invoice_id, &bid_id)
            .expect("accepting bid before due_date must succeed");
        assert_eq!(ctx.invoice.id, invoice_id);
        assert_eq!(ctx.bid.bid_id, bid_id);
    });
}
