//! Invariant: dispute resolution emits exactly one event — either
//! `DisputeResolved` or `DisputeRejected`, never both.
//!
//! Off-chain indexers reconstruct dispute state from these event topics.
//! This test pins the one-event-per-resolution invariant for both
//! `resolve_dispute` and `resolve_dispute_structured`.

use super::*;
use crate::errors::QuickLendXError;
use crate::events::{
    DisputeRejected, DisputeResolved, TOPIC_DISPUTE_CREATED, TOPIC_DISPUTE_REJECTED,
    TOPIC_DISPUTE_RESOLVED, TOPIC_DISPUTE_UNDER_REVIEW,
};
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, xdr, Address, BytesN, Env, String, Symbol, TryFromVal, Val, Vec,
};

const INV_AMOUNT: i128 = 1_500_000;
const EXP_RETURN: i128 = 1_650_000;

fn count_events_with_topic(env: &Env, topic_str: &str) -> usize {
    let topic_sym = Symbol::new(env, topic_str);
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym).expect("topic to ScVal");
    env.events()
        .all()
        .events()
        .iter()
        .filter(|e| match &e.body {
            xdr::ContractEventBody::V0(body) => body.topics.first() == Some(&topic_xdr),
        })
        .count()
}

fn latest_payload<T>(env: &Env, topic_str: &str) -> T
where
    T: TryFromVal<Env, Val>,
{
    let topic_sym = Symbol::new(env, topic_str);
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym).expect("topic to ScVal");
    let all = env.events().all();
    for e in all.events().iter().rev() {
        if let xdr::ContractEventBody::V0(body) = &e.body {
            if body.topics.first() == Some(&topic_xdr) {
                return T::try_from_val(
                    env,
                    &Val::try_from_val(env, &body.data).expect("data ScVal to Val"),
                )
                .expect("event payload decode");
            }
        }
    }
    panic!(
        "topic {:?} not found in {} events",
        topic_str,
        all.events().len()
    );
}

fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register(QuickLendXContract, ());
    env.ledger().set_timestamp(1);
    let client = QuickLendXContractClient::new(&env, &cid);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin, cid)
}

fn kyc_business(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    admin: &Address,
    biz: &Address,
) {
    client.submit_kyc_application(biz, &String::from_str(env, "KYC"));
    client.verify_business(admin, biz);
}

fn kyc_investor(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    investor: &Address,
    limit: i128,
) {
    client.submit_investor_kyc(investor, &String::from_str(env, "KYC"));
    client.verify_investor(investor, &limit);
}

fn mint_currency(
    env: &Env,
    contract_id: &Address,
    biz: &Address,
    investor: Option<&Address>,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let bal = 10_000_000i128;
    sac.mint(biz, &bal);
    sac.mint(contract_id, &1i128);
    if let Some(inv) = investor {
        sac.mint(inv, &bal);
        let exp = env.ledger().sequence() + 1_000;
        let tok = token::Client::new(env, &currency);
        tok.approve(biz, contract_id, &bal, &exp);
        tok.approve(inv, contract_id, &bal, &exp);
    }
    currency
}

fn fund_invoice(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    biz: &Address,
    inv: &Address,
    currency: &Address,
    desc: &str,
) -> BytesN<32> {
    let due = env.ledger().timestamp() + 86_400;
    let id = client.upload_invoice(
        biz,
        &INV_AMOUNT,
        currency,
        &due,
        &String::from_str(env, desc),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&id);
    let bid_id = client.place_bid(inv, &id, &INV_AMOUNT, &EXP_RETURN);
    client.accept_bid(&id, &bid_id);
    id
}

fn dispute_under_review(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    id: &BytesN<32>,
    biz: &Address,
    admin: &Address,
) {
    let reason = String::from_str(env, "Dispute reason");
    let evidence = String::from_str(env, "Evidence description");
    client.create_dispute(id, biz, &reason, &evidence);
    client.put_dispute_under_review(id, admin);
}

// ============================================================================
// Invariant tests
// ============================================================================

/// resolve_dispute emits exactly one DisputeResolved and zero DisputeRejected.
#[test]
fn test_resolve_dispute_emits_exactly_one_resolved_event() {
    let (env, client, admin, cid) = setup();
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_AMOUNT);
    let id = fund_invoice(&env, &client, &biz, &inv, &currency, "resolved invariant");
    dispute_under_review(&env, &client, &id, &biz, &admin);

    let resolution = String::from_str(&env, "Resolved in favor of investor");
    client.resolve_dispute(&id, &admin, &resolution);

    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_RESOLVED),
        1
    );
    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_REJECTED),
        0
    );

    // Verify payload is well-formed
    let p: DisputeResolved = latest_payload(&env, TOPIC_DISPUTE_RESOLVED);
    assert_eq!(p.invoice_id, id);
    assert_eq!(p.resolved_by, admin);
    assert_eq!(p.resolution, resolution);
}

/// resolve_dispute_structured with a non-dismissed outcome emits exactly one
/// DisputeResolved and zero DisputeRejected.
#[test]
fn test_structured_resolve_favor_investor_emits_resolved() {
    let (env, client, admin, cid) = setup();
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_AMOUNT);
    let id = fund_invoice(&env, &client, &biz, &inv, &currency, "structured favor investor");
    dispute_under_review(&env, &client, &id, &biz, &admin);

    let note = String::from_str(&env, "Investor claim upheld");
    client.resolve_dispute_structured(
        &id,
        &admin,
        &DisputeResolution::FavorInvestor,
        &note,
    );

    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_RESOLVED),
        1
    );
    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_REJECTED),
        0
    );

    let p: DisputeResolved = latest_payload(&env, TOPIC_DISPUTE_RESOLVED);
    assert_eq!(p.invoice_id, id);
    assert_eq!(p.resolved_by, admin);
    assert_eq!(p.resolution, note);
}

/// resolve_dispute_structured with FavorBusiness outcome emits DisputeResolved.
#[test]
fn test_structured_resolve_favor_business_emits_resolved() {
    let (env, client, admin, cid) = setup();
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_AMOUNT);
    let id = fund_invoice(&env, &client, &biz, &inv, &currency, "structured favor business");
    dispute_under_review(&env, &client, &id, &biz, &admin);

    let note = String::from_str(&env, "Business claim upheld");
    client.resolve_dispute_structured(
        &id,
        &admin,
        &DisputeResolution::FavorBusiness,
        &note,
    );

    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_RESOLVED),
        1
    );
    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_REJECTED),
        0
    );
}

/// resolve_dispute_structured with Split outcome emits DisputeResolved.
#[test]
fn test_structured_resolve_split_emits_resolved() {
    let (env, client, admin, cid) = setup();
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_AMOUNT);
    let id = fund_invoice(&env, &client, &biz, &inv, &currency, "structured split");
    dispute_under_review(&env, &client, &id, &biz, &admin);

    let note = String::from_str(&env, "Split resolution");
    client.resolve_dispute_structured(
        &id,
        &admin,
        &DisputeResolution::Split,
        &note,
    );

    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_RESOLVED),
        1
    );
    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_REJECTED),
        0
    );
}

/// resolve_dispute_structured with Dismissed outcome emits exactly one
/// DisputeRejected and zero DisputeResolved.
#[test]
fn test_structured_dismiss_emits_dispute_rejected() {
    let (env, client, admin, cid) = setup();
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_AMOUNT);
    let id = fund_invoice(&env, &client, &biz, &inv, &currency, "dismiss invariant");
    dispute_under_review(&env, &client, &id, &biz, &admin);

    let note = String::from_str(&env, "Claim lacks sufficient evidence");
    client.resolve_dispute_structured(
        &id,
        &admin,
        &DisputeResolution::Dismissed,
        &note,
    );

    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_RESOLVED),
        0
    );
    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_REJECTED),
        1
    );

    // Verify payload is well-formed
    let p: DisputeRejected = latest_payload(&env, TOPIC_DISPUTE_REJECTED);
    assert_eq!(p.invoice_id, id);
    assert_eq!(p.rejected_by, admin);
    assert_eq!(p.reason, note);
}

/// A second resolution attempt must fail and emit zero additional events.
#[test]
fn test_double_resolve_emits_no_extra_events() {
    let (env, client, admin, cid) = setup();
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_AMOUNT);
    let id = fund_invoice(&env, &client, &biz, &inv, &currency, "double resolve");
    dispute_under_review(&env, &client, &id, &biz, &admin);

    let note = String::from_str(&env, "Final resolution");
    client.resolve_dispute(&id, &admin, &note);

    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_RESOLVED),
        1
    );

    // Second resolve attempt must fail
    let result = client.try_resolve_dispute(&id, &admin, &note);
    assert_eq!(result, Err(QuickLendXError::DisputeNotUnderReview));

    // Event count must remain unchanged
    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_RESOLVED),
        1
    );
    assert_eq!(
        count_events_with_topic(&env, TOPIC_DISPUTE_REJECTED),
        0
    );
}
