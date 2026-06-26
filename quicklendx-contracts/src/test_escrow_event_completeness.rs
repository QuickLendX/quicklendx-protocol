//! Tests that escrow lifecycle events are emitted exactly once per transition
//! and never emitted on rejected transitions.
//!
//! Off-chain indexers reconstruct escrow state from these event topics.
//! This test pins the one-event-per-transition invariant for escrow creation,
//! release, and refund.

use super::*;
use crate::events::{
    EscrowCreated, EscrowRefunded, EscrowReleased, TOPIC_ESCROW_CREATED, TOPIC_ESCROW_REFUNDED,
    TOPIC_ESCROW_RELEASED,
};
use crate::invoice::InvoiceCategory;
use crate::payments::EscrowStatus;
use soroban_sdk::{
    testutils::Address as _, token, xdr, Address, BytesN, Env, String, Symbol, TryFromVal, Val, Vec,
};

const EVENT_AMOUNT: i128 = 10_000;
const EVENT_RETURN: i128 = 11_000;

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

fn setup_event_test(env: &Env) -> (QuickLendXContractClient<'static>, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    env.ledger().set_timestamp(1);
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.set_admin(&admin);
    (client, admin, contract_id)
}

fn prepare_verified_business_and_investor(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    admin: &Address,
) -> (Address, Address) {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);

    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &EVENT_AMOUNT);

    (business, investor)
}

fn mint_test_currency(
    env: &Env,
    contract_id: &Address,
    business: &Address,
    investor: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    let balance = 100_000i128;
    sac.mint(business, &balance);
    sac.mint(investor, &balance);
    sac.mint(contract_id, &1i128);
    let exp = env.ledger().sequence() + 1_000;
    tok.approve(business, contract_id, &balance, &exp);
    tok.approve(investor, contract_id, &balance, &exp);
    currency
}

fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    business: &Address,
    currency: &Address,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        business,
        &EVENT_AMOUNT,
        currency,
        &due_date,
        &String::from_str(env, "Escrow event completeness invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

#[test]
fn test_escrow_event_completeness() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup_event_test(&env);
    let (business, investor) = prepare_verified_business_and_investor(&env, &client, &admin);
    let currency = mint_test_currency(&env, &cid, &business, &investor);
    let invoice_id = create_verified_invoice(&env, &client, &business, &currency);

    let bid_id = client.place_bid(&investor, &invoice_id, &EVENT_AMOUNT, &EVENT_RETURN);

    // Escrow created path
    let events_before = env.events().all().events().len();
    client.accept_bid(&invoice_id, &bid_id);
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
    assert_eq!(
        count_events_with_topic(&env, TOPIC_ESCROW_CREATED),
        1,
        "expected exactly one EscrowCreated event"
    );
    let created_event: EscrowCreated = latest_payload(&env, TOPIC_ESCROW_CREATED);
    assert_eq!(created_event.escrow_id, escrow.escrow_id);
    assert_eq!(created_event.invoice_id, invoice_id);
    assert_eq!(created_event.investor, investor);
    assert_eq!(created_event.business, business);
    assert_eq!(created_event.amount, escrow.amount);

    // Release path
    client.release_escrow_funds(&invoice_id);
    assert_eq!(
        client.get_escrow_status(&invoice_id),
        EscrowStatus::Released
    );
    assert_eq!(
        count_events_with_topic(&env, TOPIC_ESCROW_RELEASED),
        1,
        "expected exactly one EscrowReleased event"
    );
    let released_event: EscrowReleased = latest_payload(&env, TOPIC_ESCROW_RELEASED);
    assert_eq!(released_event.escrow_id, escrow.escrow_id);
    assert_eq!(released_event.invoice_id, invoice_id);
    assert_eq!(released_event.business, business);
    assert_eq!(released_event.amount, escrow.amount);

    // Refund should be rejected after release and must not emit escrow refund
    let refund_result = client.try_refund_escrow_funds(&invoice_id, &admin);
    assert!(
        refund_result.is_err(),
        "refund after release must be rejected"
    );
    assert_eq!(
        count_events_with_topic(&env, TOPIC_ESCROW_REFUNDED),
        0,
        "no EscrowRefunded event should be emitted on rejected refund"
    );

    // Ensure no duplicate create/release events after failed refund attempt
    assert_eq!(
        count_events_with_topic(&env, TOPIC_ESCROW_CREATED),
        1,
        "EscrowCreated must remain exactly once"
    );
    assert_eq!(
        count_events_with_topic(&env, TOPIC_ESCROW_RELEASED),
        1,
        "EscrowReleased must remain exactly once"
    );

    // Separate refund path on fresh invoice
    let invoice_id_2 = create_verified_invoice(&env, &client, &business, &currency);
    let bid_id_2 = client.place_bid(&investor, &invoice_id_2, &EVENT_AMOUNT, &EVENT_RETURN);
    client.accept_bid(&invoice_id_2, &bid_id_2);
    let escrow_2 = client.get_escrow_details(&invoice_id_2);
    assert_eq!(client.get_escrow_status(&invoice_id_2), EscrowStatus::Held);
    assert_eq!(
        count_events_with_topic(&env, TOPIC_ESCROW_CREATED),
        2,
        "each escrow create transition must emit exactly one EscrowCreated event"
    );

    client.refund_escrow_funds(&invoice_id_2, &business);
    assert_eq!(
        client.get_escrow_status(&invoice_id_2),
        EscrowStatus::Refunded
    );
    assert_eq!(
        count_events_with_topic(&env, TOPIC_ESCROW_REFUNDED),
        1,
        "expected exactly one EscrowRefunded event"
    );
    let refunded_event: EscrowRefunded = latest_payload(&env, TOPIC_ESCROW_REFUNDED);
    assert_eq!(refunded_event.escrow_id, escrow_2.escrow_id);
    assert_eq!(refunded_event.invoice_id, invoice_id_2);
    assert_eq!(refunded_event.investor, investor);
    assert_eq!(refunded_event.amount, escrow_2.amount);

    // A second refund attempt must be rejected and emit nothing new
    let refund_again = client.try_refund_escrow_funds(&invoice_id_2, &business);
    assert!(
        refund_again.is_err(),
        "second refund attempt must be rejected"
    );
    assert_eq!(
        count_events_with_topic(&env, TOPIC_ESCROW_REFUNDED),
        1,
        "EscrowRefunded must remain exactly once per escrow refund transition"
    );

    // No extra create/release events from the refund-only path
    assert_eq!(
        count_events_with_topic(&env, TOPIC_ESCROW_RELEASED),
        1,
        "EscrowReleased must remain exactly once after refund-only path"
    );
}
