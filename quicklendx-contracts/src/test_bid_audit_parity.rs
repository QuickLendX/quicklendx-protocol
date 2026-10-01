//! Bid submission and auction selection: event <-> audit parity (issue #2447).
//!
//! Every committed bid lifecycle transition must publish BOTH:
//!   1. a `#[contractevent]` (observed by off-chain indexers), and
//!   2. an immutable audit-log entry on the same invoice trail.
//!
//! This module pins the two channels together so bid events and audit entries
//! stay in lock-step forever:
//!   - `BidPlaced`   (bid submission)
//!   - `BidAccepted` (auction selection / escrow funding)
//!   - `BidWithdrawn`
//!   - `BidCancelled`
//!   - `BidExpired`  (both immediate and paginated cleanup)
//!   - rejected transitions must NOT leave a partial audit trail.
//!
//! The parity link between an event and its audit entry is the bid id, which
//! the audit `additional_data` field carries as lowercase hex.

use super::*;
use crate::audit::AuditOperation;
use crate::bid::MIN_BID_TTL_DAYS;
use crate::events::{
    TOPIC_BID_ACCEPTED, TOPIC_BID_CANCELLED, TOPIC_BID_EXPIRED, TOPIC_BID_PLACED,
    TOPIC_BID_WITHDRAWN,
};
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, xdr, Address, BytesN, Env, String, Symbol, Vec,
};

const DAY: u64 = 86_400;
const BID_AMOUNT: i128 = 5_000;
const EXPECTED_RETURN: i128 = 6_000;

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn make_token(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    investor: &Address,
) -> Address {
    let contract_id = client.address.clone();
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(business, &100_000i128);
    sac.mint(investor, &100_000i128);
    sac.mint(&contract_id, &1i128);
    let exp = env.ledger().sequence() + 100_000;
    tok.approve(business, &contract_id, &400_000i128, &exp);
    tok.approve(investor, &contract_id, &400_000i128, &exp);
    currency
}

/// Register a verified invoice and KYC'd business/investor, returning all ids.
fn funded_setup(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    amount: i128,
) -> (Address, Address, BytesN<32>) {
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let currency = make_token(env, client, &business, &investor);

    client.submit_kyc_application(&business, &String::from_str(env, "KYC"));
    client.verify_business(admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC"));
    client.verify_investor(&investor, &200_000i128);

    let due_date = env.ledger().timestamp() + 30 * DAY;
    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Bid audit parity"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
        &None,
        &None,
    );
    client.verify_invoice(&invoice_id);
    (business, investor, invoice_id)
}

fn place_bid(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    invoice_id: &BytesN<32>,
) -> BytesN<32> {
    client.place_bid(
        investor,
        invoice_id,
        &BID_AMOUNT,
        &EXPECTED_RETURN,
        &BytesN::from_array(env, &[0u8; 32]),
    )
}

/// Number of emitted events whose topic matches `topic`.
fn event_count(env: &Env, topic: &str) -> usize {
    let topic_sym = Symbol::new(env, topic);
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym).expect("topic to ScVal");
    env.events()
        .all()
        .events()
        .iter()
        .filter(|e| matches!(&e.body, xdr::ContractEventBody::V0(body) if body.topics.first() == Some(&topic_xdr)))
        .count()
}

/// Fetch the sole audit entry for `op`, panicking on any deviation from 1.
fn single_audit_entry(
    env: &Env,
    client: &QuickLendXContractClient,
    op: AuditOperation,
) -> soroban_sdk::Vec<BytesN<32>> {
    let ids = client.get_audit_entries_by_operation(&op);
    assert_eq!(ids.len(), 1, "expected exactly one audit entry for {op:?}");
    ids
}

fn bid_hex(env: &Env, bid_id: &BytesN<32>) -> String {
    let mut out = [0u8; 64];
    for (i, byte) in bid_id.to_array().iter().enumerate() {
        out[2 * i] = hex_nibble(byte >> 4);
        out[2 * i + 1] = hex_nibble(byte & 0x0F);
    }
    String::from_str(env, core::str::from_utf8(&out).expect("hex is ascii"))
}

fn hex_nibble(v: u8) -> u8 {
    if v < 10 {
        b'0' + v
    } else {
        b'a' + (v - 10)
    }
}

/// Bid submission: one `BidPlaced` event and one matching audit entry.
#[test]
fn test_bid_placed_event_and_audit_parity() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);

    let bid_id = place_bid(&env, &client, &investor, &invoice_id);

    assert_eq!(event_count(&env, TOPIC_BID_PLACED), 1);
    let ids = single_audit_entry(&env, &client, AuditOperation::BidPlaced);
    let entry = client
        .get_audit_entry(ids.get(0).unwrap())
        .expect("BidPlaced audit entry exists");
    assert_eq!(entry.operation, AuditOperation::BidPlaced);
    assert_eq!(entry.invoice_id, invoice_id);
    assert_eq!(entry.actor, investor);
    assert_eq!(entry.amount, Some(BID_AMOUNT));
    assert_eq!(entry.additional_data, Some(bid_hex(&env, &bid_id)));
}

/// Auction selection: accepted bid produces event + audit in lock-step.
#[test]
fn test_bid_accepted_event_and_audit_parity() {
    let (env, client, admin) = setup();
    let (business, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id);

    client.accept_bid(&invoice_id, &bid_id);

    assert_eq!(event_count(&env, TOPIC_BID_ACCEPTED), 1);
    let ids = single_audit_entry(&env, &client, AuditOperation::BidAccepted);
    let entry = client
        .get_audit_entry(ids.get(0).unwrap())
        .expect("BidAccepted audit entry exists");
    assert_eq!(entry.operation, AuditOperation::BidAccepted);
    assert_eq!(entry.invoice_id, invoice_id);
    assert_eq!(entry.actor, business);
    assert_eq!(entry.amount, Some(BID_AMOUNT));
    assert_eq!(entry.additional_data, Some(bid_hex(&env, &bid_id)));
}

/// Investor withdraw: event + audit entry carry the same bid id.
#[test]
fn test_bid_withdrawn_event_and_audit_parity() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id);

    client.withdraw_bid(&bid_id);

    assert_eq!(event_count(&env, TOPIC_BID_WITHDRAWN), 1);
    let ids = single_audit_entry(&env, &client, AuditOperation::BidWithdrawn);
    let entry = client
        .get_audit_entry(ids.get(0).unwrap())
        .expect("BidWithdrawn audit entry exists");
    assert_eq!(entry.operation, AuditOperation::BidWithdrawn);
    assert_eq!(entry.invoice_id, invoice_id);
    assert_eq!(entry.actor, investor);
    assert_eq!(entry.amount, None);
    assert_eq!(entry.additional_data, Some(bid_hex(&env, &bid_id)));
}

/// Investor cancel: event + audit entry carry the same bid id.
#[test]
fn test_bid_cancelled_event_and_audit_parity() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id);

    assert!(client.cancel_bid(&bid_id));

    assert_eq!(event_count(&env, TOPIC_BID_CANCELLED), 1);
    let ids = single_audit_entry(&env, &client, AuditOperation::BidCancelled);
    let entry = client
        .get_audit_entry(ids.get(0).unwrap())
        .expect("BidCancelled audit entry exists");
    assert_eq!(entry.operation, AuditOperation::BidCancelled);
    assert_eq!(entry.invoice_id, invoice_id);
    assert_eq!(entry.actor, investor);
    assert_eq!(entry.amount, None);
    assert_eq!(entry.additional_data, Some(bid_hex(&env, &bid_id)));
}

/// Expiry via the permissionless cleanup entrypoint: event + audit parity.
#[test]
fn test_bid_expired_cleanup_event_and_audit_parity() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&MIN_BID_TTL_DAYS);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id);
    let bid = client.get_bid(&bid_id).unwrap();
    env.ledger().set_timestamp(bid.expiration_timestamp + 1);

    let cleaned = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(cleaned, 1);

    assert_eq!(event_count(&env, TOPIC_BID_EXPIRED), 1);
    let ids = single_audit_entry(&env, &client, AuditOperation::BidExpired);
    let entry = client
        .get_audit_entry(ids.get(0).unwrap())
        .expect("BidExpired audit entry exists");
    assert_eq!(entry.operation, AuditOperation::BidExpired);
    assert_eq!(entry.invoice_id, invoice_id);
    assert_eq!(entry.actor, investor);
    assert_eq!(entry.amount, Some(BID_AMOUNT));
    assert_eq!(entry.additional_data, Some(bid_hex(&env, &bid_id)));
}

/// Expiry via the paginated cleanup entrypoint: event + audit parity.
#[test]
fn test_bid_expired_paged_cleanup_event_and_audit_parity() {
    let (env, client, admin) = setup();
    client.set_bid_ttl_days(&MIN_BID_TTL_DAYS);
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id);
    let bid = client.get_bid(&bid_id).unwrap();
    env.ledger().set_timestamp(bid.expiration_timestamp + 1);

    let (cleaned, _remaining) = client.cleanup_expired_bids_paged(&invoice_id, &0, &1);
    assert_eq!(cleaned, 1);

    assert_eq!(event_count(&env, TOPIC_BID_EXPIRED), 1);
    let ids = single_audit_entry(&env, &client, AuditOperation::BidExpired);
    let entry = client
        .get_audit_entry(ids.get(0).unwrap())
        .expect("BidExpired paged audit entry exists");
    assert_eq!(entry.operation, AuditOperation::BidExpired);
    assert_eq!(entry.invoice_id, invoice_id);
    assert_eq!(entry.actor, investor);
    assert_eq!(entry.amount, Some(BID_AMOUNT));
    assert_eq!(entry.additional_data, Some(bid_hex(&env, &bid_id)));
}

/// Rejected transitions are atomic: a failed cancel emits no event and appends
/// no audit entry, so the persisted two-channel record can never drift.
#[test]
fn test_rejected_cancel_writes_no_event_or_audit_entry() {
    let (env, client, admin) = setup();
    let (_, investor, invoice_id) = funded_setup(&env, &client, &admin, 10_000);
    let bid_id = place_bid(&env, &client, &investor, &invoice_id);

    assert!(client.cancel_bid(&bid_id));

    assert_eq!(event_count(&env, TOPIC_BID_CANCELLED), 1);
    assert_eq!(
        client
            .get_audit_entries_by_operation(&AuditOperation::BidCancelled)
            .len(),
        1
    );

    // Cancelling an already-cancelled bid must be a no-op: no second event,
    // no second audit entry (no double-action / no partial write).
    assert!(!client.cancel_bid(&bid_id));

    assert_eq!(
        event_count(&env, TOPIC_BID_CANCELLED),
        1,
        "rejected cancel must not emit a second event"
    );
    assert_eq!(
        client
            .get_audit_entries_by_operation(&AuditOperation::BidCancelled)
            .len(),
        1,
        "rejected cancel must not append a second audit entry"
    );
}
