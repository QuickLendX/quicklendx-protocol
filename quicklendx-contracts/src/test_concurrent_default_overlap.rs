/// Tests for concurrent / overlapping default attempts on the same invoice.
///
/// Boundary under test: two callers attempt to default the same invoice at the
/// same ledger timestamp. The transition guard ensures exactly one succeeds and
/// the second receives `DuplicateDefaultTransition` — state is updated exactly
/// once (status → Defaulted, investment → Defaulted, insurance claimed once).
use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

const GRACE: u64 = 7 * 24 * 60 * 60;

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
}

fn fund_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    amount: i128,
) -> BytesN<32> {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);

    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "kyc"));
    client.verify_business(admin, &business);

    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "kyc"));
    client.verify_investor(&investor, &(amount * 2));

    client.add_currency(admin, &currency);
    sac.mint(&investor, &amount);
    let expiry = env.ledger().sequence() + 10_000;
    tok.approve(&investor, &client.address, &amount, &expiry);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "inv"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 100));
    client.accept_bid(&invoice_id, &bid_id);

    invoice_id
}

/// Happy path: the first default call wins and transitions the invoice to Defaulted.
#[test]
fn first_default_call_wins() {
    let (env, client, admin) = setup();
    let invoice_id = fund_invoice(&env, &client, &admin, 1_000);

    let due = client.get_invoice(&invoice_id).due_date;
    env.ledger().set_timestamp(due + GRACE + 1);

    client.mark_invoice_defaulted(&invoice_id, &Some(GRACE));

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

/// Sad path: a second default attempt on the same invoice is rejected with
/// `DuplicateDefaultTransition` — the guard fires before status re-check.
#[test]
fn second_default_call_returns_duplicate_transition_error() {
    let (env, client, admin) = setup();
    let invoice_id = fund_invoice(&env, &client, &admin, 1_000);

    let due = client.get_invoice(&invoice_id).due_date;
    env.ledger().set_timestamp(due + GRACE + 1);

    // First call succeeds.
    client.mark_invoice_defaulted(&invoice_id, &Some(GRACE));

    // Second call — simulates a concurrent caller that arrived just after.
    let err = client
        .try_mark_invoice_defaulted(&invoice_id, &Some(GRACE))
        .unwrap_err()
        .expect("expected contract error");

    assert_eq!(err, QuickLendXError::DuplicateDefaultTransition);
}

/// Two concurrent defaults (same timestamp) — only one state write occurs.
/// Invoice must appear in the Defaulted list exactly once after both calls.
#[test]
fn concurrent_defaults_same_timestamp_only_one_state_write() {
    let (env, client, admin) = setup();
    let invoice_id = fund_invoice(&env, &client, &admin, 1_000);

    let due = client.get_invoice(&invoice_id).due_date;
    env.ledger().set_timestamp(due + GRACE + 1);

    // First caller wins.
    let first = client.try_mark_invoice_defaulted(&invoice_id, &Some(GRACE));
    // Second caller loses.
    let second = client.try_mark_invoice_defaulted(&invoice_id, &Some(GRACE));

    assert!(first.is_ok(), "first default call must succeed");
    assert!(second.is_err(), "second default call must fail");

    // Invoice appears in Defaulted list exactly once.
    let defaulted = client.get_invoices_by_status(&InvoiceStatus::Defaulted);
    let count = defaulted.iter().filter(|id| *id == invoice_id).count();
    assert_eq!(count, 1, "invoice must appear in Defaulted list exactly once");

    // Invoice must no longer be in Funded list.
    let funded = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(
        !funded.iter().any(|id| id == invoice_id),
        "invoice must be removed from Funded list after default"
    );
}

/// Investment status transitions to Defaulted exactly once regardless of how
/// many default calls are attempted.
#[test]
fn investment_status_updated_exactly_once_on_concurrent_defaults() {
    let (env, client, admin) = setup();
    let invoice_id = fund_invoice(&env, &client, &admin, 1_000);

    let due = client.get_invoice(&invoice_id).due_date;
    env.ledger().set_timestamp(due + GRACE + 1);

    client.mark_invoice_defaulted(&invoice_id, &Some(GRACE));
    let _ = client.try_mark_invoice_defaulted(&invoice_id, &Some(GRACE));

    assert_eq!(
        client.get_invoice_investment(&invoice_id).status,
        crate::investment::InvestmentStatus::Defaulted
    );
}

/// `check_invoice_expiration` followed by `mark_invoice_defaulted` — the
/// second path must lose cleanly.
#[test]
fn check_expiration_then_mark_defaulted_second_returns_duplicate_error() {
    let (env, client, admin) = setup();
    let invoice_id = fund_invoice(&env, &client, &admin, 1_000);

    let due = client.get_invoice(&invoice_id).due_date;
    env.ledger().set_timestamp(due + GRACE + 1);

    // Path A: automated scan defaults the invoice.
    let did_default = client.check_invoice_expiration(&invoice_id, &Some(GRACE));
    assert!(did_default, "check_invoice_expiration must default the invoice");

    // Path B: concurrent manual call arrives after path A.
    let err = client
        .try_mark_invoice_defaulted(&invoice_id, &Some(GRACE))
        .unwrap_err()
        .expect("expected contract error");

    assert_eq!(err, QuickLendXError::DuplicateDefaultTransition);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}
