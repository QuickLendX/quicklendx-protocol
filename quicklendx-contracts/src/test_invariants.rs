#![cfg(test)]

//! Invariant tests for protocol state consistency after a full lifecycle.
//!
//! This module provides an integration test that runs the complete flow
//! (KYC, upload, verify, bid, accept, release/settle, rate) and then asserts
//! global invariants: total_invoice_count, status counts, audit trail length,
//! escrow released, investment completed, and no orphaned storage.

use crate::investment::{InvestmentStatus, InvestmentStorage};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::EscrowStatus;
use crate::QuickLendXContract;
use crate::QuickLendXContractClient;
use soroban_sdk::{testutils::Address as _, token, Address, Env, String, Vec};

/// Invariant test scaffold for protocol state consistency.
/// Intentionally minimal and non-invasive.
#[test]
fn invariant_env_creation_is_safe() {
    let env = Env::default();
    let _ = env.ledger().timestamp();
}

/// Full lifecycle integration test: KYC → upload → verify → bid → accept →
/// release escrow → settle (partial payment to full) → rate.
/// Asserts: total_invoice_count, status counts, audit trail length,
/// escrow gone (Released), investment completed, no orphaned storage.
#[test]
fn test_invariants_after_full_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Token setup
    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    let initial_balance = 20_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, &contract_id, &initial_balance, &expiration);

    // 1. KYC: business and investor
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
    client.verify_investor(&investor, &15_000);

    // 2. Upload and verify invoice
    let amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Full lifecycle invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    // 3. Bid and accept (creates escrow)
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 500));
    client.accept_bid(&invoice_id, &bid_id);

    // 4. Release escrow (funds to business)
    client.release_escrow_funds(&invoice_id);

    // 5. Settle: business pays full amount (triggers settlement and investment completed)
    client.process_partial_payment(
        &invoice_id,
        &amount,
        &String::from_str(&env, "lifecycle-tx-1"),
    );

    // 6. Rate (allowed for Funded or Paid)
    client.add_invoice_rating(
        &invoice_id,
        &5,
        &String::from_str(&env, "Smooth process"),
        &investor,
    );

    // --- Invariant assertions ---

    // total_invoice_count: at least one invoice; sum of status counts must match (no orphaned storage)
    let total_invoice_count = client.get_total_invoice_count();
    assert!(
        total_invoice_count >= 1,
        "total_invoice_count must be at least 1"
    );

    // status counts: our invoice is Paid
    let paid_count = client.get_invoice_count_by_status(&InvoiceStatus::Paid);
    let pending_count = client.get_invoice_count_by_status(&InvoiceStatus::Pending);
    let verified_count = client.get_invoice_count_by_status(&InvoiceStatus::Verified);
    let funded_count = client.get_invoice_count_by_status(&InvoiceStatus::Funded);
    let defaulted_count = client.get_invoice_count_by_status(&InvoiceStatus::Defaulted);
    let cancelled_count = client.get_invoice_count_by_status(&InvoiceStatus::Cancelled);

    assert_eq!(
        paid_count, 1,
        "exactly one invoice must be Paid after full lifecycle"
    );

    // status counts sum to total (global invariant: no orphaned storage)
    let sum_status = pending_count
        + verified_count
        + funded_count
        + paid_count
        + defaulted_count
        + cancelled_count;
    assert_eq!(
        sum_status, total_invoice_count,
        "sum of status counts must equal total_invoice_count (no orphaned status buckets)"
    );

    // audit trail length: at least create, verify, funding, payment, settlement, rating
    let audit_trail = client.get_invoice_audit_trail(&invoice_id);
    assert!(
        audit_trail.len() >= 4,
        "audit trail must have multiple entries (create, verify, payment, etc.)"
    );

    // escrow gone: escrow exists but status is Released (funds no longer held)
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(
        escrow.status,
        EscrowStatus::Released,
        "escrow must be Released after release_escrow_funds (no funds held)"
    );

    // investment completed
    let investment = env.as_contract(&contract_id, || {
        InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
    });
    let investment = investment.expect("investment must exist for settled invoice");
    assert_eq!(
        investment.status,
        InvestmentStatus::Completed,
        "investment must be Completed after settlement"
    );

    // no orphaned storage: the one invoice we have is the one we created and is Paid
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.id, invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
    let paid_invoices = client.get_invoices_by_status(&InvoiceStatus::Paid);
    assert_eq!(paid_invoices.len(), 1);
    assert_eq!(paid_invoices.get(0).unwrap(), invoice_id);
}
