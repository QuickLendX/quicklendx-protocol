//! Regression tests for the escrow terminal-state race between settlement and refund.
//!
//! Soroban serializes contract calls, so a same-ledger race is modeled here as
//! two ordered calls against the same funded invoice. The first call performs
//! the winning terminal transition; the second call must observe that persisted
//! terminal state and fail with `QuickLendXError::InvalidStatus`.
//!
//! The token contract is deliberately pre-funded with unrelated same-currency
//! balance at the QuickLendX contract address. That makes the double-spend check
//! strict: a buggy second terminal transfer would have enough token balance to
//! execute, and would therefore be caught by the exact contract-balance delta and
//! total-balance conservation assertions below.

use super::*;
use crate::bid::BidStatus;
use crate::errors::QuickLendXError;
use crate::investment::InvestmentStatus;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::EscrowStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

const INITIAL_BALANCE: i128 = 500_000;
const INVOICE_AMOUNT: i128 = 100_000;
const UNRELATED_CONTRACT_BALANCE: i128 = 250_000;
const LEDGER_TIMESTAMP: u64 = 1_000_000;

macro_rules! assert_invalid_status {
    ($result:expr) => {{
        let result = $result;
        assert!(
            matches!(&result, Err(Ok(QuickLendXError::InvalidStatus))),
            "losing terminal operation must fail with InvalidStatus; got: {result:?}"
        );
    }};
}

#[derive(Clone, Copy, Debug)]
enum RefundCaller {
    Admin,
    Business,
}

struct FundedFixture {
    env: Env,
    client: QuickLendXContractClient<'static>,
    contract_id: Address,
    admin: Address,
    business: Address,
    investor: Address,
    currency: Address,
    invoice_id: BytesN<32>,
    bid_id: BytesN<32>,
    escrow_id: BytesN<32>,
}

#[derive(Debug)]
struct RaceSnapshot {
    contract_balance: i128,
    business_balance: i128,
    investor_balance: i128,
    invoice_status: InvoiceStatus,
    invoice_funded_amount: i128,
    invoice_total_paid: i128,
    invoice_has_investor: bool,
    invoice_payment_count: u32,
    bid_status: BidStatus,
    investment_status: InvestmentStatus,
    investment_amount: i128,
    escrow_status: EscrowStatus,
    escrow_amount: i128,
}

struct TerminalExpectation {
    invoice_status: InvoiceStatus,
    bid_status: BidStatus,
    investment_status: InvestmentStatus,
    escrow_status: EscrowStatus,
    funded_amount: i128,
    total_paid: i128,
    invoice_has_investor: bool,
    payment_count: u32,
}

fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(LEDGER_TIMESTAMP);

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);

    (env, client, contract_id, admin)
}

fn setup_token(env: &Env, addresses: &[&Address], contract_id: &Address) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let allowance_expiration = env.ledger().sequence() + 100_000;

    for address in addresses {
        sac_client.mint(address, &INITIAL_BALANCE);
        token_client.approve(
            address,
            contract_id,
            &INITIAL_BALANCE,
            &allowance_expiration,
        );
    }

    // Unrelated balance is not escrow. It proves a second payout cannot hide
    // behind an insufficient-balance failure.
    sac_client.mint(contract_id, &UNRELATED_CONTRACT_BALANCE);

    currency
}

fn verified_business(env: &Env, client: &QuickLendXContractClient<'_>, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC Data"));
    client.verify_business(admin, &business);
    business
}

fn verified_investor(
    env: &Env,
    client: &QuickLendXContractClient<'_>,
    investment_limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC Data"));
    client.verify_investor(&investor, &investment_limit);
    investor
}

fn upload_and_verify_invoice(
    env: &Env,
    client: &QuickLendXContractClient<'_>,
    business: &Address,
    currency: &Address,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        business,
        &INVOICE_AMOUNT,
        currency,
        &due_date,
        &String::from_str(env, "Settlement/refund race regression invoice"),
        &InvoiceCategory::Technology,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

fn place_bid(
    client: &QuickLendXContractClient<'_>,
    investor: &Address,
    invoice_id: &BytesN<32>,
) -> BytesN<32> {
    client.place_bid(
        investor,
        invoice_id,
        &INVOICE_AMOUNT,
        &(INVOICE_AMOUNT + 100),
    )
}

fn build_funded_fixture() -> FundedFixture {
    let (env, client, contract_id, admin) = setup();

    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, INITIAL_BALANCE);
    let currency = setup_token(&env, &[&investor, &business], &contract_id);
    let invoice_id = upload_and_verify_invoice(&env, &client, &business, &currency);
    let bid_id = place_bid(&client, &investor, &invoice_id);
    let escrow_id = client.accept_bid_and_fund(&invoice_id, &bid_id);

    let token_client = token::Client::new(&env, &currency);
    let escrow = client.get_escrow_details(&invoice_id);

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );
    assert_eq!(
        client.get_bid(&bid_id).expect("bid must exist").status,
        BidStatus::Accepted
    );
    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        InvestmentStatus::Active
    );
    assert_eq!(escrow.escrow_id, escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Held);
    assert_eq!(
        token_client.balance(&contract_id),
        UNRELATED_CONTRACT_BALANCE + INVOICE_AMOUNT,
        "funding should lock one escrow amount while preserving unrelated contract balance"
    );
    assert_eq!(
        token_client.balance(&investor),
        INITIAL_BALANCE - INVOICE_AMOUNT
    );
    assert_eq!(token_client.balance(&business), INITIAL_BALANCE);

    FundedFixture {
        env,
        client,
        contract_id,
        admin,
        business,
        investor,
        currency,
        invoice_id,
        bid_id,
        escrow_id,
    }
}

fn refund_caller(fixture: &FundedFixture, caller: RefundCaller) -> &Address {
    match caller {
        RefundCaller::Admin => &fixture.admin,
        RefundCaller::Business => &fixture.business,
    }
}

fn race_snapshot(fixture: &FundedFixture) -> RaceSnapshot {
    let token_client = token::Client::new(&fixture.env, &fixture.currency);
    let invoice = fixture.client.get_invoice(&fixture.invoice_id);
    let bid = fixture
        .client
        .get_bid(&fixture.bid_id)
        .expect("bid must exist");
    let investment = fixture.client.get_invoice_investment(&fixture.invoice_id);
    let escrow = fixture.client.get_escrow_details(&fixture.invoice_id);

    RaceSnapshot {
        contract_balance: token_client.balance(&fixture.contract_id),
        business_balance: token_client.balance(&fixture.business),
        investor_balance: token_client.balance(&fixture.investor),
        invoice_status: invoice.status,
        invoice_funded_amount: invoice.funded_amount,
        invoice_total_paid: invoice.total_paid,
        invoice_has_investor: invoice.investor.is_some(),
        invoice_payment_count: invoice.payment_history.len(),
        bid_status: bid.status,
        investment_status: investment.status,
        investment_amount: investment.amount,
        escrow_status: escrow.status.clone(),
        escrow_amount: escrow.amount,
    }
}

fn assert_unchanged_after_loser(after_winner: &RaceSnapshot, after_loser: &RaceSnapshot) {
    assert_eq!(after_loser.contract_balance, after_winner.contract_balance);
    assert_eq!(after_loser.business_balance, after_winner.business_balance);
    assert_eq!(after_loser.investor_balance, after_winner.investor_balance);
    assert_eq!(after_loser.invoice_status, after_winner.invoice_status);
    assert_eq!(
        after_loser.invoice_funded_amount,
        after_winner.invoice_funded_amount
    );
    assert_eq!(
        after_loser.invoice_total_paid,
        after_winner.invoice_total_paid
    );
    assert_eq!(
        after_loser.invoice_has_investor,
        after_winner.invoice_has_investor
    );
    assert_eq!(
        after_loser.invoice_payment_count,
        after_winner.invoice_payment_count
    );
    assert_eq!(after_loser.bid_status, after_winner.bid_status);
    assert_eq!(
        after_loser.investment_status,
        after_winner.investment_status
    );
    assert_eq!(
        after_loser.investment_amount,
        after_winner.investment_amount
    );
    assert_eq!(after_loser.escrow_status, after_winner.escrow_status);
    assert_eq!(after_loser.escrow_amount, after_winner.escrow_amount);
}

fn total_observed_balance(snapshot: &RaceSnapshot) -> i128 {
    snapshot
        .contract_balance
        .checked_add(snapshot.business_balance)
        .and_then(|sum| sum.checked_add(snapshot.investor_balance))
        .expect("test balances must not overflow")
}

fn assert_exactly_one_escrow_disbursement(
    before_terminal: &RaceSnapshot,
    after_winner: &RaceSnapshot,
    after_loser: &RaceSnapshot,
) {
    let expected_total = INITIAL_BALANCE * 2 + UNRELATED_CONTRACT_BALANCE;

    assert_eq!(total_observed_balance(before_terminal), expected_total);
    assert_eq!(total_observed_balance(after_winner), expected_total);
    assert_eq!(total_observed_balance(after_loser), expected_total);
    assert_eq!(
        before_terminal.contract_balance,
        UNRELATED_CONTRACT_BALANCE + INVOICE_AMOUNT,
        "before terminal action, contract must hold unrelated funds plus one escrow amount"
    );
    assert_eq!(
        after_winner.contract_balance, UNRELATED_CONTRACT_BALANCE,
        "the winning terminal operation must release/refund exactly one escrow amount"
    );
    assert_eq!(
        after_loser.contract_balance, UNRELATED_CONTRACT_BALANCE,
        "the losing terminal operation must not drain unrelated contract funds"
    );
    assert_eq!(
        before_terminal.contract_balance - after_loser.contract_balance,
        INVOICE_AMOUNT,
        "contract balance delta must equal exactly one escrow disbursement"
    );

    let recipient_delta = after_loser.business_balance + after_loser.investor_balance
        - before_terminal.business_balance
        - before_terminal.investor_balance;
    assert_eq!(
        recipient_delta, INVOICE_AMOUNT,
        "business+investor balances must increase by exactly one escrow amount"
    );
    assert_eq!(after_loser.business_balance, INITIAL_BALANCE);
    assert_eq!(after_loser.investor_balance, INITIAL_BALANCE);
}

fn assert_terminal_consistency(fixture: &FundedFixture, expected: TerminalExpectation) {
    let invoice = fixture.client.get_invoice(&fixture.invoice_id);
    let bid = fixture
        .client
        .get_bid(&fixture.bid_id)
        .expect("bid must exist");
    let investment = fixture.client.get_invoice_investment(&fixture.invoice_id);
    let escrow = fixture.client.get_escrow_details(&fixture.invoice_id);

    assert_eq!(invoice.id, fixture.invoice_id);
    assert_eq!(invoice.business, fixture.business);
    assert_eq!(invoice.currency, fixture.currency);
    assert_eq!(invoice.amount, INVOICE_AMOUNT);
    assert_eq!(invoice.status, expected.invoice_status);
    assert_eq!(invoice.funded_amount, expected.funded_amount);
    assert_eq!(invoice.total_paid, expected.total_paid);
    assert_eq!(invoice.investor.is_some(), expected.invoice_has_investor);
    assert_eq!(invoice.payment_history.len(), expected.payment_count);

    if expected.payment_count == 1 {
        let payment = invoice
            .payment_history
            .get(0)
            .expect("paid invoice must contain exactly one payment record");
        assert_eq!(payment.payer, fixture.business);
        assert_eq!(payment.amount, INVOICE_AMOUNT);
    } else {
        assert_eq!(expected.payment_count, 0);
    }

    if expected.invoice_has_investor {
        assert_eq!(invoice.investor.as_ref(), Some(&fixture.investor));
        assert!(invoice.funded_at.is_some());
    } else {
        assert!(invoice.investor.is_none());
        assert!(invoice.funded_at.is_none());
    }

    assert_eq!(
        invoice.settled_at.is_some(),
        expected.invoice_status == InvoiceStatus::Paid
    );

    assert_eq!(bid.invoice_id, fixture.invoice_id);
    assert_eq!(bid.investor, fixture.investor);
    assert_eq!(bid.bid_amount, INVOICE_AMOUNT);
    assert_eq!(bid.status, expected.bid_status);

    assert_eq!(investment.invoice_id, fixture.invoice_id);
    assert_eq!(investment.investor, fixture.investor);
    assert_eq!(investment.amount, INVOICE_AMOUNT);
    assert_eq!(investment.status, expected.investment_status);

    assert_eq!(escrow.escrow_id, fixture.escrow_id);
    assert_eq!(escrow.invoice_id, fixture.invoice_id);
    assert_eq!(escrow.investor, fixture.investor);
    assert_eq!(escrow.business, fixture.business);
    assert_eq!(escrow.amount, INVOICE_AMOUNT);
    assert_eq!(escrow.currency, fixture.currency);
    assert_eq!(escrow.status, expected.escrow_status);

    let released = escrow.status == EscrowStatus::Released;
    let refunded = escrow.status == EscrowStatus::Refunded;
    assert!(
        released ^ refunded,
        "escrow must end in exactly one terminal state: Released xor Refunded"
    );

    assert_eq!(fixture.client.get_active_investment_ids().len(), 0);
    assert!(
        fixture.client.validate_no_orphan_investments(),
        "terminal invoice must not leave an active orphan investment"
    );
}

fn paid_expectation() -> TerminalExpectation {
    TerminalExpectation {
        invoice_status: InvoiceStatus::Paid,
        bid_status: BidStatus::Accepted,
        investment_status: InvestmentStatus::Completed,
        escrow_status: EscrowStatus::Released,
        funded_amount: INVOICE_AMOUNT,
        total_paid: INVOICE_AMOUNT,
        invoice_has_investor: true,
        payment_count: 1,
    }
}

fn refunded_expectation() -> TerminalExpectation {
    TerminalExpectation {
        invoice_status: InvoiceStatus::Refunded,
        bid_status: BidStatus::Cancelled,
        investment_status: InvestmentStatus::Refunded,
        escrow_status: EscrowStatus::Refunded,
        funded_amount: 0,
        total_paid: 0,
        invoice_has_investor: false,
        payment_count: 0,
    }
}

fn settle_then_refund_is_rejected(refund_loser: RefundCaller) {
    let fixture = build_funded_fixture();
    let before_terminal = race_snapshot(&fixture);

    fixture
        .client
        .settle_invoice(&fixture.invoice_id, &INVOICE_AMOUNT);
    let after_winner = race_snapshot(&fixture);

    let losing_refund = fixture
        .client
        .try_refund_escrow_funds(&fixture.invoice_id, refund_caller(&fixture, refund_loser));
    assert_invalid_status!(losing_refund);
    let after_loser = race_snapshot(&fixture);

    assert_unchanged_after_loser(&after_winner, &after_loser);
    assert_exactly_one_escrow_disbursement(&before_terminal, &after_winner, &after_loser);
    assert_terminal_consistency(&fixture, paid_expectation());
}

fn refund_then_settle_is_rejected(refund_winner: RefundCaller) {
    let fixture = build_funded_fixture();
    let before_terminal = race_snapshot(&fixture);

    fixture
        .client
        .refund_escrow_funds(&fixture.invoice_id, refund_caller(&fixture, refund_winner));
    let after_winner = race_snapshot(&fixture);

    let losing_settle = fixture
        .client
        .try_settle_invoice(&fixture.invoice_id, &INVOICE_AMOUNT);
    assert_invalid_status!(losing_settle);
    let after_loser = race_snapshot(&fixture);

    assert_unchanged_after_loser(&after_winner, &after_loser);
    assert_exactly_one_escrow_disbursement(&before_terminal, &after_winner, &after_loser);
    assert_terminal_consistency(&fixture, refunded_expectation());
}

fn final_partial_payment_then_refund_is_rejected(refund_loser: RefundCaller) {
    let fixture = build_funded_fixture();
    let before_terminal = race_snapshot(&fixture);

    fixture.client.process_partial_payment(
        &fixture.invoice_id,
        &INVOICE_AMOUNT,
        &String::from_str(&fixture.env, "race-final-partial-payment"),
    );
    let after_winner = race_snapshot(&fixture);

    let losing_refund = fixture
        .client
        .try_refund_escrow_funds(&fixture.invoice_id, refund_caller(&fixture, refund_loser));
    assert_invalid_status!(losing_refund);
    let after_loser = race_snapshot(&fixture);

    assert_unchanged_after_loser(&after_winner, &after_loser);
    assert_exactly_one_escrow_disbursement(&before_terminal, &after_winner, &after_loser);
    assert_terminal_consistency(&fixture, paid_expectation());
}

/// Explicit settlement wins first; later admin refund must fail without a second payout.
#[test]
fn settle_then_admin_refund_is_rejected_without_double_spend() {
    settle_then_refund_is_rejected(RefundCaller::Admin);
}

/// Explicit settlement wins first; later business-owner refund must fail the same way.
#[test]
fn settle_then_business_refund_is_rejected_without_double_spend() {
    settle_then_refund_is_rejected(RefundCaller::Business);
}

/// Admin refund wins first; later explicit settlement must fail without a second payout.
#[test]
fn admin_refund_then_settle_is_rejected_without_double_spend() {
    refund_then_settle_is_rejected(RefundCaller::Admin);
}

/// Business-owner refund wins first; later explicit settlement must fail without a second payout.
#[test]
fn business_refund_then_settle_is_rejected_without_double_spend() {
    refund_then_settle_is_rejected(RefundCaller::Business);
}

/// Final partial-payment settlement wins first; later admin refund must fail.
#[test]
fn final_partial_payment_then_admin_refund_is_rejected_without_double_spend() {
    final_partial_payment_then_refund_is_rejected(RefundCaller::Admin);
}

/// Final partial-payment settlement wins first; later business-owner refund must fail.
#[test]
fn final_partial_payment_then_business_refund_is_rejected_without_double_spend() {
    final_partial_payment_then_refund_is_rejected(RefundCaller::Business);
}
