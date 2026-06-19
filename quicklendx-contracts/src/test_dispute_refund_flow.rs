//! End-to-end dispute-resolution-to-refund regression.
//!
//! The investor remedy is only final if the dispute, invoice, escrow, bid,
//! investment, status indexes, and token balances all agree after the refund.
//! This test drives the full funded invoice -> dispute -> review -> resolution
//! -> refund path and asserts no second refund or settlement can follow.

use super::*;
use crate::errors::QuickLendXError;
use crate::payments::EscrowStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

struct FundedDisputeFixture {
    env: Env,
    client: QuickLendXContractClient<'static>,
    admin: Address,
    business: Address,
    investor: Address,
    currency: Address,
    invoice_id: BytesN<32>,
    bid_id: BytesN<32>,
    bid_amount: i128,
}

fn setup_funded_invoice_for_dispute() -> FundedDisputeFixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000_000);

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    client.set_admin(&admin);
    let _ = client.try_initialize_protocol_limits(&admin, &1i128, &365u64, &86_400u64);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    let initial_balance = 10_000i128;

    sac.mint(&business, &initial_balance);
    sac.mint(&investor, &initial_balance);
    sac.mint(&contract_id, &1i128);

    let approval_expiration = env.ledger().sequence() + 10_000;
    tok.approve(
        &business,
        &contract_id,
        &initial_balance,
        &approval_expiration,
    );
    tok.approve(
        &investor,
        &contract_id,
        &initial_balance,
        &approval_expiration,
    );

    client.add_currency(&admin, &currency);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
    client.verify_investor(&investor, &initial_balance);

    let invoice_amount = 1_000i128;
    let bid_amount = 900i128;
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        &business,
        &invoice_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Disputed goods delivery"),
        &InvoiceCategory::Goods,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &invoice_amount);
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    FundedDisputeFixture {
        env,
        client,
        admin,
        business,
        investor,
        currency,
        invoice_id,
        bid_id,
        bid_amount,
    }
}

#[test]
fn dispute_resolved_against_business_refund_aligns_terminal_statuses() {
    let fx = setup_funded_invoice_for_dispute();
    let tok = token::Client::new(&fx.env, &fx.currency);
    let investor_balance_before_refund = tok.balance(&fx.investor);

    assert_eq!(
        fx.client.get_invoice(&fx.invoice_id).status,
        InvoiceStatus::Funded
    );
    assert_eq!(
        fx.client.get_escrow_status(&fx.invoice_id),
        EscrowStatus::Held
    );
    assert_eq!(
        fx.client.get_bid(&fx.bid_id).unwrap().status,
        BidStatus::Accepted
    );
    assert_eq!(
        fx.client.get_invoice_investment(&fx.invoice_id).status,
        InvestmentStatus::Active
    );

    fx.client.create_dispute(
        &fx.invoice_id,
        &fx.investor,
        &String::from_str(&fx.env, "Delivered goods were rejected by buyer"),
        &String::from_str(&fx.env, "Inspection record and delivery photos"),
    );
    fx.client
        .put_dispute_under_review(&fx.invoice_id, &fx.admin);
    fx.client.resolve_dispute(
        &fx.invoice_id,
        &fx.admin,
        &String::from_str(&fx.env, "Resolved against business; refund investor escrow"),
    );

    assert_eq!(
        fx.client.get_invoice(&fx.invoice_id).dispute_status,
        DisputeStatus::Resolved
    );
    let dispute = fx
        .client
        .get_dispute_details(&fx.invoice_id)
        .expect("resolved dispute should remain queryable");
    assert_eq!(dispute.resolved_by, fx.admin);
    assert!(dispute.resolved_at > 0);

    fx.client.refund_escrow_funds(&fx.invoice_id, &fx.business);

    assert_eq!(
        fx.client.get_escrow_status(&fx.invoice_id),
        EscrowStatus::Refunded
    );

    let invoice = fx.client.get_invoice(&fx.invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Refunded);
    assert_eq!(invoice.dispute_status, DisputeStatus::Resolved);
    assert_eq!(invoice.investor, None);
    assert_eq!(invoice.funded_amount, 0);
    assert_eq!(invoice.funded_at, None);

    assert_eq!(
        fx.client.get_bid(&fx.bid_id).unwrap().status,
        BidStatus::Cancelled
    );
    assert_eq!(
        fx.client.get_invoice_investment(&fx.invoice_id).status,
        InvestmentStatus::Refunded
    );

    let investor_balance_after_refund = tok.balance(&fx.investor);
    assert_eq!(
        investor_balance_after_refund - investor_balance_before_refund,
        fx.bid_amount
    );
    assert!(
        !fx.client
            .get_invoices_by_status(&InvoiceStatus::Funded)
            .contains(&fx.invoice_id),
        "refunded disputed invoice must leave the Funded index"
    );
    assert!(
        fx.client
            .get_invoices_by_status(&InvoiceStatus::Refunded)
            .contains(&fx.invoice_id),
        "refunded disputed invoice must enter the Refunded index"
    );

    let retry = fx
        .client
        .try_refund_escrow_funds(&fx.invoice_id, &fx.business);
    assert!(matches!(retry, Err(Ok(QuickLendXError::InvalidStatus))));
    assert_eq!(
        tok.balance(&fx.investor),
        investor_balance_after_refund,
        "second refund attempt must not move funds"
    );

    let settle_after_refund = fx.client.try_settle_invoice(&fx.invoice_id, &fx.bid_amount);
    assert!(matches!(
        settle_after_refund,
        Err(Ok(QuickLendXError::InvalidStatus))
    ));
    assert_eq!(
        tok.balance(&fx.investor),
        investor_balance_after_refund,
        "settlement attempt after refund must not move funds"
    );
}
