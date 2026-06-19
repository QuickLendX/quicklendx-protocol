//! Cancel-vs-accept interleaving regression for a single bid.
//!
//! Soroban executes transactions serially, but an investor cancelling a bid and
//! a business accepting that same bid can be submitted in the same logical
//! window. These tests model both possible ledger orderings and assert that the
//! second transition is rejected without leaving split bid, invoice, escrow, or
//! investment state.

use super::*;
use crate::errors::QuickLendXError;
use crate::investment::InvestmentStatus;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::EscrowStatus;
use crate::types::BidStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

struct CancelAcceptFixture {
    env: Env,
    client: QuickLendXContractClient<'static>,
    contract_id: Address,
    invoice_id: BytesN<32>,
    bid_id: BytesN<32>,
    investor: Address,
}

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn setup_token(
    env: &Env,
    contract_id: &Address,
    business: &Address,
    investor: &Address,
    business_balance: i128,
    investor_balance: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);

    sac.mint(business, &business_balance);
    sac.mint(investor, &investor_balance);
    sac.mint(contract_id, &1i128);

    let exp = env.ledger().sequence() + 100_000;
    tok.approve(business, contract_id, &(business_balance * 4), &exp);
    tok.approve(investor, contract_id, &(investor_balance * 4), &exp);

    currency
}

fn verified_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    _admin: &Address,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

fn build_cancel_accept_fixture() -> CancelAcceptFixture {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, &admin, 50_000);
    let currency = setup_token(&env, &contract_id, &business, &investor, 20_000, 20_000);
    client.add_currency(&admin, &currency);

    let invoice_amount = 10_000i128;
    let bid_amount = 9_000i128;
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        &business,
        &invoice_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Cancel accept race invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &invoice_amount);

    CancelAcceptFixture {
        env,
        client,
        contract_id,
        invoice_id,
        bid_id,
        investor,
    }
}

fn assert_no_funding_state(client: &QuickLendXContractClient, invoice_id: &BytesN<32>) {
    let invoice = client.get_invoice(invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Verified,
        "cancelled bid must leave invoice available but unfunded"
    );
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.funded_at.is_none());
    assert!(invoice.investor.is_none());
    assert!(
        client.try_get_escrow_details(invoice_id).is_err(),
        "cancelled bid must not create escrow"
    );
    assert!(
        client.try_get_invoice_investment(invoice_id).is_err(),
        "cancelled bid must not create investment"
    );
}

fn assert_funded_state(
    client: &QuickLendXContractClient,
    invoice_id: &BytesN<32>,
    bid_id: &BytesN<32>,
    investor: &Address,
) {
    let invoice = client.get_invoice(invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, 9_000);
    assert_eq!(invoice.investor, Some(investor.clone()));
    assert!(invoice.funded_at.is_some());

    let bid = client.get_bid(bid_id).expect("bid must exist");
    assert_eq!(bid.status, BidStatus::Accepted);

    let escrow = client.get_escrow_details(invoice_id);
    assert_eq!(escrow.status, EscrowStatus::Held);
    assert_eq!(escrow.amount, 9_000);
    assert_eq!(escrow.investor, *investor);

    let investment = client.get_invoice_investment(invoice_id);
    assert_eq!(investment.status, InvestmentStatus::Active);
    assert_eq!(investment.invoice_id, *invoice_id);
    assert_eq!(investment.investor, *investor);
    assert_eq!(investment.amount, 9_000);
}

fn assert_invoice_count_invariant(client: &QuickLendXContractClient) {
    let total = client.get_total_invoice_count();
    let sum = client.get_invoice_count_by_status(&InvoiceStatus::Pending)
        + client.get_invoice_count_by_status(&InvoiceStatus::Verified)
        + client.get_invoice_count_by_status(&InvoiceStatus::Funded)
        + client.get_invoice_count_by_status(&InvoiceStatus::Paid)
        + client.get_invoice_count_by_status(&InvoiceStatus::Defaulted)
        + client.get_invoice_count_by_status(&InvoiceStatus::Cancelled)
        + client.get_invoice_count_by_status(&InvoiceStatus::Refunded);
    assert_eq!(total, sum, "invoice status indexes must remain balanced");
}

/// Race ordering: the investor cancellation is ordered before the business
/// acceptance. The cancelled bid must not be selected by ranking or funded by a
/// later accept attempt.
#[test]
fn test_cancel_then_accept_same_bid_rejects_accept_and_leaves_no_partial_state() {
    let fixture = build_cancel_accept_fixture();

    assert_eq!(
        fixture
            .client
            .get_bid(&fixture.bid_id)
            .expect("bid must exist")
            .status,
        BidStatus::Placed
    );

    assert!(
        fixture.client.cancel_bid(&fixture.bid_id),
        "first cancel must transition Placed -> Cancelled"
    );

    let bid = fixture
        .client
        .get_bid(&fixture.bid_id)
        .expect("bid must remain stored");
    assert_eq!(bid.status, BidStatus::Cancelled);
    assert!(
        fixture.client.get_best_bid(&fixture.invoice_id).is_none(),
        "cancelled bid must not be returned by get_best_bid"
    );

    let accept_after_cancel = fixture
        .client
        .try_accept_bid_and_fund(&fixture.invoice_id, &fixture.bid_id);
    let err = accept_after_cancel
        .expect_err("accepting a cancelled bid must fail")
        .expect("contract error must decode");
    assert_eq!(err, QuickLendXError::InvalidStatus);

    let bid_after = fixture
        .client
        .get_bid(&fixture.bid_id)
        .expect("bid must remain stored");
    assert_eq!(
        bid_after.status,
        BidStatus::Cancelled,
        "failed accept must not resurrect or fund a cancelled bid"
    );
    assert_no_funding_state(&fixture.client, &fixture.invoice_id);
    assert_invoice_count_invariant(&fixture.client);

    let token_client = token::Client::new(
        &fixture.env,
        &fixture.client.get_invoice(&fixture.invoice_id).currency,
    );
    assert_eq!(
        token_client.balance(&fixture.contract_id),
        1,
        "failed accept must not transfer investor funds into escrow"
    );
    assert_eq!(token_client.balance(&fixture.investor), 20_000);
}

/// Race ordering: the business acceptance is ordered before the investor
/// cancellation. Once the bid is Accepted, cancellation must be a false no-op
/// and the funded invoice, escrow, and investment must remain mutually
/// consistent.
#[test]
fn test_accept_then_cancel_same_bid_rejects_cancel_and_preserves_funded_state() {
    let fixture = build_cancel_accept_fixture();

    let accept = fixture
        .client
        .try_accept_bid_and_fund(&fixture.invoice_id, &fixture.bid_id);
    assert!(
        accept.is_ok(),
        "first accept must succeed before any cancellation; got {accept:?}"
    );

    assert!(
        !fixture.client.cancel_bid(&fixture.bid_id),
        "cancel_bid must return false once the bid is Accepted"
    );

    assert_funded_state(
        &fixture.client,
        &fixture.invoice_id,
        &fixture.bid_id,
        &fixture.investor,
    );
    assert!(
        fixture.client.get_best_bid(&fixture.invoice_id).is_none(),
        "accepted bid must not remain selectable as a Placed best bid"
    );
    assert_invoice_count_invariant(&fixture.client);

    let token_client = token::Client::new(
        &fixture.env,
        &fixture.client.get_invoice(&fixture.invoice_id).currency,
    );
    assert_eq!(
        token_client.balance(&fixture.contract_id),
        9_001,
        "contract balance must contain only the seeded token plus accepted bid amount"
    );
    assert_eq!(token_client.balance(&fixture.investor), 11_000);
}
