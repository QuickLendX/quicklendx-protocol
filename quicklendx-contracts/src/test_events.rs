//! Event schema compatibility tests for the QuickLendX protocol.
//!
//! # Purpose
//! These tests pin the exact topic symbol and payload field ordering for every
//! event emitted by the protocol so that off-chain indexers and analytics
//! tooling are alerted immediately (via CI failure) when the schema changes.
//!
//! # Coverage
//! - Invoice events: uploaded, verified, cancelled, settled, defaulted, expired,
//!   partial-payment, funded
//! - Bid events: placed, accepted, withdrawn, expired
//! - Escrow events: created, released, refunded
//! - Dispute events: created, under-review, resolved
//! - Platform fee events: updated
//! - Audit events: query, validation
//! - Cross-cutting: topic constant stability, no events on reads, lifecycle ordering
//!
//! # Security Notes
//! - All timestamps come from `env.ledger().timestamp()` — tamper-proof in Soroban.
//! - No PII is included in any event payload; only identifiers, addresses, amounts.
//! - Read-only entrypoints emit zero events, confirmed by `test_no_events_emitted_for_reads`.
//! - Topic constants are compile-time `symbol_short!` values — mismatches are compile errors.

use super::*;
use crate::audit::{AuditOperationFilter, AuditQueryFilter};
use crate::errors::QuickLendXError;
use crate::events::{
    TOPIC_BID_ACCEPTED, TOPIC_BID_EXPIRED, TOPIC_BID_PLACED, TOPIC_BID_WITHDRAWN,
    TOPIC_ESCROW_CREATED, TOPIC_ESCROW_REFUNDED, TOPIC_ESCROW_RELEASED, TOPIC_INVOICE_CANCELLED,
    TOPIC_INVOICE_DEFAULTED, TOPIC_INVOICE_EXPIRED, TOPIC_INVOICE_SETTLED,
    TOPIC_INVOICE_SETTLED_FINAL, TOPIC_INVOICE_UPLOADED, TOPIC_INVOICE_VERIFIED,
    TOPIC_PARTIAL_PAYMENT, TOPIC_PAYMENT_RECORDED,
};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::EscrowStatus;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    token, Address, BytesN, Env, String, TryFromVal, Val, Vec,
};

// ============================================================================
// Constants
// ============================================================================

const INV_AMOUNT: i128 = 1_500_000;
const INV_LIMIT: i128 = 5_000_000;
const EXP_RETURN: i128 = 1_650_000;

// ============================================================================
// Helpers
// ============================================================================

fn setup(env: &Env) -> (QuickLendXContractClient, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    env.ledger().set_timestamp(1);
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.set_admin(&admin);
    (client, admin, contract_id)
}

fn kyc_business(env: &Env, client: &QuickLendXContractClient, admin: &Address, biz: &Address) {
    client.submit_kyc_application(biz, &String::from_str(env, "KYC"));
    client.verify_business(admin, biz);
}

fn kyc_investor(env: &Env, client: &QuickLendXContractClient, investor: &Address, limit: i128) {
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
    let tok = token::Client::new(env, &currency);
    let bal = 10_000_000i128;
    sac.mint(biz, &bal);
    sac.mint(contract_id, &1i128);
    if let Some(inv) = investor {
        sac.mint(inv, &bal);
        let exp = env.ledger().sequence() + 1_000;
        tok.approve(biz, contract_id, &bal, &exp);
        tok.approve(inv, contract_id, &bal, &exp);
    }
    currency
}

fn upload_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    biz: &Address,
    currency: &Address,
    desc: &str,
) -> (BytesN<32>, u64) {
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
    (id, due)
}

/// Return the payload of the most-recent event matching `topic`.
fn latest_payload<T>(env: &Env, topic: soroban_sdk::Symbol) -> T
where
    T: TryFromVal<Env, Val> + core::fmt::Debug + PartialEq + Clone,
{
    let events = env.events().all();
    let mut i = events.len();
    while i > 0 {
        i -= 1;
        let (_, topics, data): (_, soroban_sdk::Vec<Val>, Val) = events.get(i).unwrap();
        for t in topics.iter() {
            if let Ok(s) = soroban_sdk::Symbol::try_from_val(env, &t) {
                if s == topic {
                    return T::try_from_val(env, &data).expect("payload decode failed");
                }
            }
        }
    }
    panic!("topic {:?} not found; events: {:?}", topic, events);
}

fn assert_payload<T>(env: &Env, topic: soroban_sdk::Symbol, expected: T)
where
    T: TryFromVal<Env, Val> + core::fmt::Debug + PartialEq,
{
    assert_eq!(latest_payload::<T>(env, topic), expected);
}

fn count_events_with_topic(env: &Env, topic: soroban_sdk::Symbol) -> usize {
    let events = env.events().all();
    let mut count = 0;
    for i in 0..events.len() {
        let (_, topics, _): (_, soroban_sdk::Vec<Val>, Val) = events.get(i).unwrap();
        for t in topics.iter() {
            if let Ok(s) = soroban_sdk::Symbol::try_from_val(env, &t) {
                if s == topic {
                    count += 1;
                    break;
                }
            }
        }
    }
    count
}

// ============================================================================
// 1. Topic Constant Stability
// ============================================================================

#[test]
fn test_topic_constants_are_stable() {
    assert_eq!(TOPIC_INVOICE_UPLOADED, symbol_short!("inv_up"));
    assert_eq!(TOPIC_INVOICE_VERIFIED, symbol_short!("inv_ver"));
    assert_eq!(TOPIC_INVOICE_CANCELLED, symbol_short!("inv_canc"));
    assert_eq!(TOPIC_INVOICE_SETTLED, symbol_short!("inv_set"));
    assert_eq!(TOPIC_INVOICE_DEFAULTED, symbol_short!("inv_def"));
    assert_eq!(TOPIC_INVOICE_EXPIRED, symbol_short!("inv_exp"));
    assert_eq!(TOPIC_PARTIAL_PAYMENT, symbol_short!("inv_pp"));
    assert_eq!(TOPIC_PAYMENT_RECORDED, symbol_short!("pay_rec"));
    assert_eq!(TOPIC_INVOICE_SETTLED_FINAL, symbol_short!("inv_stlf"));
    assert_eq!(TOPIC_BID_PLACED, symbol_short!("bid_plc"));
    assert_eq!(TOPIC_BID_ACCEPTED, symbol_short!("bid_acc"));
    assert_eq!(TOPIC_BID_WITHDRAWN, symbol_short!("bid_wdr"));
    assert_eq!(TOPIC_BID_EXPIRED, symbol_short!("bid_exp"));
    assert_eq!(TOPIC_ESCROW_CREATED, symbol_short!("esc_cr"));
    assert_eq!(TOPIC_ESCROW_RELEASED, symbol_short!("esc_rel"));
    assert_eq!(TOPIC_ESCROW_REFUNDED, symbol_short!("esc_ref"));
}

#[test]
fn test_admin_update_invoice_status_requires_configured_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due = env.ledger().timestamp() + 86_400;

    let id = client.store_invoice(
        &business,
        &INV_AMOUNT,
        &currency,
        &due,
        &String::from_str(&env, "missing admin"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let result = client.try_update_invoice_status(&id, &InvoiceStatus::Verified);
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("expected contract error"),
        QuickLendXError::NotAdmin
    );
}

// ============================================================================
// 2. Invoice Uploaded — field order
// ============================================================================

#[test]
fn test_invoice_uploaded_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, None);
    kyc_business(&env, &client, &admin, &biz);

    let ts = env.ledger().timestamp();
    let due = ts + 86_400;
    let id = client.upload_invoice(
        &biz,
        &INV_AMOUNT,
        &currency,
        &due,
        &String::from_str(&env, "upload field order"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let p: (BytesN<32>, Address, i128, Address, u64, u64) =
        latest_payload(&env, TOPIC_INVOICE_UPLOADED);
    assert_eq!(p.0, id); // field 0: invoice_id
    assert_eq!(p.1, biz); // field 1: business
    assert_eq!(p.2, INV_AMOUNT); // field 2: amount
    assert_eq!(p.3, currency); // field 3: currency
    assert_eq!(p.4, due); // field 4: due_date
    assert_eq!(p.5, ts); // field 5: timestamp
}

// ============================================================================
// 3. Invoice Verified — field order
// ============================================================================

#[test]
fn test_invoice_verified_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, None);
    kyc_business(&env, &client, &admin, &biz);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "verify field order");

    let ts = env.ledger().timestamp() + 5;
    env.ledger().set_timestamp(ts);
    client.verify_invoice(&id);

    assert_payload(&env, TOPIC_INVOICE_VERIFIED, (id.clone(), biz.clone(), ts));
}

#[test]
fn test_admin_update_invoice_status_verified_emits_canonical_event_and_moves_index() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, None);
    kyc_business(&env, &client, &admin, &biz);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "admin verify override");
    let ts = env.ledger().timestamp() + 7;
    env.ledger().set_timestamp(ts);

    client.update_invoice_status(&id, &InvoiceStatus::Verified);

    assert_payload(&env, TOPIC_INVOICE_VERIFIED, (id.clone(), biz.clone(), ts));
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Verified);
    assert!(!client
        .get_invoices_by_status(&InvoiceStatus::Pending)
        .iter()
        .any(|existing| existing == id));
    assert!(client
        .get_invoices_by_status(&InvoiceStatus::Verified)
        .iter()
        .any(|existing| existing == id));
}

// ============================================================================
// 4. Invoice Cancelled — field order
// ============================================================================

#[test]
fn test_invoice_cancelled_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, None);
    kyc_business(&env, &client, &admin, &biz);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "cancel field order");
    client.verify_invoice(&id);

    let ts = env.ledger().timestamp() + 10;
    env.ledger().set_timestamp(ts);
    client.cancel_invoice(&id);

    assert_payload(&env, TOPIC_INVOICE_CANCELLED, (id.clone(), biz.clone(), ts));
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Cancelled);
}

// ============================================================================
// 5. Invoice Defaulted — field order
// ============================================================================

#[test]
fn test_invoice_defaulted_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_LIMIT);

    let (id, due) = upload_invoice(&env, &client, &biz, &currency, "default field order");
    client.verify_invoice(&id);
    let bid_id = client.place_bid(&inv, &id, &INV_AMOUNT, &EXP_RETURN);
    client.accept_bid(&id, &bid_id);

    let ts = due + 1;
    env.ledger().set_timestamp(ts);
    client.handle_default(&id);

    let p: (BytesN<32>, Address, Address, u64) = latest_payload(&env, TOPIC_INVOICE_DEFAULTED);
    assert_eq!(p.0, id); // field 0: invoice_id
    assert_eq!(p.1, biz); // field 1: business
    assert_eq!(p.2, inv); // field 2: investor
    assert_eq!(p.3, ts); // field 3: timestamp
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Defaulted);
}

#[test]
fn test_admin_update_invoice_status_funded_emits_canonical_event_and_moves_index() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, None);
    kyc_business(&env, &client, &admin, &biz);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "admin funded override");
    client.update_invoice_status(&id, &InvoiceStatus::Verified);

    let ts = env.ledger().timestamp() + 9;
    env.ledger().set_timestamp(ts);
    client.update_invoice_status(&id, &InvoiceStatus::Funded);

    let p: (BytesN<32>, Address, i128, u64) = latest_payload(&env, symbol_short!("inv_fnd"));
    assert_eq!(p.0, id);
    assert_eq!(p.1, admin);
    assert_eq!(p.2, INV_AMOUNT);
    assert_eq!(p.3, ts);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Funded);
    assert!(!client
        .get_invoices_by_status(&InvoiceStatus::Verified)
        .iter()
        .any(|existing| existing == id));
    assert!(client
        .get_invoices_by_status(&InvoiceStatus::Funded)
        .iter()
        .any(|existing| existing == id));
}

#[test]
fn test_admin_update_invoice_status_paid_emits_canonical_event_and_moves_index() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, None);
    kyc_business(&env, &client, &admin, &biz);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "admin paid override");
    client.update_invoice_status(&id, &InvoiceStatus::Verified);
    client.update_invoice_status(&id, &InvoiceStatus::Funded);

    let ts = env.ledger().timestamp() + 11;
    env.ledger().set_timestamp(ts);
    client.update_invoice_status(&id, &InvoiceStatus::Paid);

    let p: (BytesN<32>, Address, Address, i128, i128, u64) =
        latest_payload(&env, TOPIC_INVOICE_SETTLED);
    assert_eq!(p.0, id);
    assert_eq!(p.1, biz);
    assert_eq!(p.2, admin);
    assert_eq!(p.3, 0);
    assert_eq!(p.4, 0);
    assert_eq!(p.5, ts);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Paid);
    assert!(!client
        .get_invoices_by_status(&InvoiceStatus::Funded)
        .iter()
        .any(|existing| existing == id));
    assert!(client
        .get_invoices_by_status(&InvoiceStatus::Paid)
        .iter()
        .any(|existing| existing == id));
}

#[test]
fn test_admin_update_invoice_status_defaulted_emits_canonical_event_and_moves_index() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, None);
    kyc_business(&env, &client, &admin, &biz);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "admin default override");
    client.update_invoice_status(&id, &InvoiceStatus::Verified);
    client.update_invoice_status(&id, &InvoiceStatus::Funded);

    let ts = env.ledger().timestamp() + 13;
    env.ledger().set_timestamp(ts);
    client.update_invoice_status(&id, &InvoiceStatus::Defaulted);

    assert_payload(
        &env,
        TOPIC_INVOICE_DEFAULTED,
        (id.clone(), biz.clone(), admin.clone(), ts),
    );
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Defaulted);
    assert!(!client
        .get_invoices_by_status(&InvoiceStatus::Funded)
        .iter()
        .any(|existing| existing == id));
    assert!(client
        .get_invoices_by_status(&InvoiceStatus::Defaulted)
        .iter()
        .any(|existing| existing == id));
}

// ============================================================================
// 6. Invoice Settled — field order
// ============================================================================

#[test]
fn test_invoice_settled_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_LIMIT);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "settle field order");
    client.verify_invoice(&id);
    let bid_id = client.place_bid(&inv, &id, &INV_AMOUNT, &EXP_RETURN);
    client.accept_bid(&id, &bid_id);

    let ts = env.ledger().timestamp() + 1;
    env.ledger().set_timestamp(ts);
    client.make_payment(&id, &EXP_RETURN, &String::from_str(&env, "TX1"));

    // Field order: (invoice_id, business, investor, investor_return, platform_fee, timestamp)
    let p: (BytesN<32>, Address, Address, i128, i128, u64) =
        latest_payload(&env, TOPIC_INVOICE_SETTLED);
    assert_eq!(p.0, id); // field 0: invoice_id
    assert_eq!(p.1, biz); // field 1: business
    assert_eq!(p.2, inv); // field 2: investor
    assert!(p.3 >= 0); // field 3: investor_return
    assert!(p.4 >= 0); // field 4: platform_fee
    assert_eq!(p.5, ts); // field 5: timestamp
}

// ============================================================================
// 7. Invoice Expired — field order
// ============================================================================

#[test]
fn test_invoice_expired_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, None);
    kyc_business(&env, &client, &admin, &biz);

    let (id, due) = upload_invoice(&env, &client, &biz, &currency, "expire field order");
    client.verify_invoice(&id);

    // Advance past due date and trigger expiration
    env.ledger().set_timestamp(due + 1);
    client.expire_invoice(&id);

    let p: (BytesN<32>, Address, u64) = latest_payload(&env, TOPIC_INVOICE_EXPIRED);
    assert_eq!(p.0, id); // field 0: invoice_id
    assert_eq!(p.1, biz); // field 1: business
    assert_eq!(p.2, due); // field 2: due_date (original, not current ts)
}

// ============================================================================
// 8. Partial Payment — field order
// ============================================================================

#[test]
fn test_partial_payment_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_LIMIT);

    let (id, _) = upload_invoice(
        &env,
        &client,
        &biz,
        &currency,
        "partial payment field order",
    );
    client.verify_invoice(&id);
    let bid_id = client.place_bid(&inv, &id, &INV_AMOUNT, &EXP_RETURN);
    client.accept_bid(&id, &bid_id);

    let pay_amount = EXP_RETURN / 2;
    let tx_id = String::from_str(&env, "TX_PARTIAL");
    client.make_payment(&id, &pay_amount, &tx_id);

    // Field order: (invoice_id, business, payment_amount, total_paid, progress_bps, tx_id)
    let p: (BytesN<32>, Address, i128, i128, u32, String) =
        latest_payload(&env, TOPIC_PARTIAL_PAYMENT);
    assert_eq!(p.0, id); // field 0: invoice_id
    assert_eq!(p.1, biz); // field 1: business
    assert_eq!(p.2, pay_amount); // field 2: payment_amount
    assert_eq!(p.3, pay_amount); // field 3: total_paid (first payment)
    assert!(p.4 <= 10_000); // field 4: progress_bps
    assert_eq!(p.5, tx_id); // field 5: transaction_id
}

// ============================================================================
// 9. Bid Placed — field order
// ============================================================================

#[test]
fn test_bid_placed_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_LIMIT);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "bid placed field order");
    client.verify_invoice(&id);

    let ts = 100u64;
    env.ledger().set_timestamp(ts);
    let bid_id = client.place_bid(&inv, &id, &INV_AMOUNT, &EXP_RETURN);

    let p: (BytesN<32>, BytesN<32>, Address, i128, i128, u64, u64) =
        latest_payload(&env, TOPIC_BID_PLACED);
    assert_eq!(p.0, bid_id); // field 0: bid_id
    assert_eq!(p.1, id); // field 1: invoice_id
    assert_eq!(p.2, inv); // field 2: investor
    assert_eq!(p.3, INV_AMOUNT); // field 3: bid_amount
    assert_eq!(p.4, EXP_RETURN); // field 4: expected_return
    assert_eq!(p.5, ts); // field 5: timestamp
    assert!(p.6 > ts); // field 6: expiration_timestamp > placed_ts
}

// ============================================================================
// 10. Bid Accepted — field order
// ============================================================================

#[test]
fn test_bid_accepted_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_LIMIT);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "bid accepted field order");
    client.verify_invoice(&id);
    let bid_id = client.place_bid(&inv, &id, &INV_AMOUNT, &EXP_RETURN);

    let ts = 200u64;
    env.ledger().set_timestamp(ts);
    client.accept_bid(&id, &bid_id);

    let p: (BytesN<32>, BytesN<32>, Address, Address, i128, i128, u64) =
        latest_payload(&env, TOPIC_BID_ACCEPTED);
    assert_eq!(p.0, bid_id); // field 0: bid_id
    assert_eq!(p.1, id); // field 1: invoice_id
    assert_eq!(p.2, inv); // field 2: investor
    assert_eq!(p.3, biz); // field 3: business
    assert_eq!(p.4, INV_AMOUNT); // field 4: bid_amount
    assert_eq!(p.5, EXP_RETURN); // field 5: expected_return
    assert_eq!(p.6, ts); // field 6: timestamp
}

// ============================================================================
// 11. Bid Withdrawn — field order
// ============================================================================

#[test]
fn test_bid_withdrawn_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_LIMIT);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "bid withdraw field order");
    client.verify_invoice(&id);
    let bid_id = client.place_bid(&inv, &id, &INV_AMOUNT, &EXP_RETURN);

    let ts = 120u64;
    env.ledger().set_timestamp(ts);
    client.withdraw_bid(&bid_id);

    let p: (BytesN<32>, BytesN<32>, Address, i128, u64) = latest_payload(&env, TOPIC_BID_WITHDRAWN);
    assert_eq!(p.0, bid_id); // field 0: bid_id
    assert_eq!(p.1, id); // field 1: invoice_id
    assert_eq!(p.2, inv); // field 2: investor
    assert_eq!(p.3, INV_AMOUNT); // field 3: bid_amount
    assert_eq!(p.4, ts); // field 4: timestamp
}

// ============================================================================
// 12. Bid Expired — field order
// ============================================================================

#[test]
fn test_bid_expired_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_LIMIT);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "bid expired field order");
    client.verify_invoice(&id);
    client.set_bid_ttl_days(&Address::generate(&env), 1); // short TTL (admin mock)
    let placed_ts = env.ledger().timestamp();
    let bid_id = client.place_bid(&inv, &id, &INV_AMOUNT, &EXP_RETURN);
    let expiry = crate::bid::Bid::default_expiration(placed_ts);

    // Advance past expiry
    env.ledger().set_timestamp(expiry + 1);
    client.clean_expired_bids(&id);

    let p: (BytesN<32>, BytesN<32>, Address, i128, u64) = latest_payload(&env, TOPIC_BID_EXPIRED);
    assert_eq!(p.0, bid_id); // field 0: bid_id
    assert_eq!(p.1, id); // field 1: invoice_id
    assert_eq!(p.2, inv); // field 2: investor
    assert_eq!(p.3, INV_AMOUNT); // field 3: bid_amount
    assert_eq!(p.4, expiry); // field 4: expiration_timestamp
}

// ============================================================================
// 13. Escrow Created — field order
// ============================================================================

#[test]
fn test_escrow_created_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_LIMIT);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "escrow created field order");
    client.verify_invoice(&id);
    let bid_id = client.place_bid(&inv, &id, &INV_AMOUNT, &EXP_RETURN);
    client.accept_bid(&id, &bid_id);

    let escrow = client.get_escrow_details(&id);
    let p: (BytesN<32>, BytesN<32>, Address, Address, i128) =
        latest_payload(&env, TOPIC_ESCROW_CREATED);
    assert_eq!(p.0, escrow.escrow_id); // field 0: escrow_id
    assert_eq!(p.1, id); // field 1: invoice_id
    assert_eq!(p.2, inv); // field 2: investor
    assert_eq!(p.3, biz); // field 3: business
    assert_eq!(p.4, escrow.amount); // field 4: amount
}

// ============================================================================
// 14. Escrow Released — field order
// ============================================================================

#[test]
fn test_escrow_released_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_LIMIT);

    let (id, _) = upload_invoice(
        &env,
        &client,
        &biz,
        &currency,
        "escrow released field order",
    );
    client.verify_invoice(&id);
    let bid_id = client.place_bid(&inv, &id, &INV_AMOUNT, &EXP_RETURN);
    client.accept_bid(&id, &bid_id);

    let escrow = client.get_escrow_details(&id);
    client.release_escrow_funds(&id);

    let p: (BytesN<32>, BytesN<32>, Address, i128) = latest_payload(&env, TOPIC_ESCROW_RELEASED);
    assert_eq!(p.0, escrow.escrow_id); // field 0: escrow_id
    assert_eq!(p.1, id); // field 1: invoice_id
    assert_eq!(p.2, biz); // field 2: business
    assert_eq!(p.3, escrow.amount); // field 3: amount
    assert_eq!(client.get_escrow_status(&id), EscrowStatus::Released);
}

// ============================================================================
// 15. Escrow Refunded on Cancellation — field order
// ============================================================================

#[test]
fn test_escrow_refunded_field_order_on_cancellation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_LIMIT);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "escrow refund field order");
    client.verify_invoice(&id);
    let bid_id = client.place_bid(&inv, &id, &INV_AMOUNT, &EXP_RETURN);
    client.accept_bid(&id, &bid_id);

    let escrow = client.get_escrow_details(&id);
    client.refund_escrow(&id);

    let p: (BytesN<32>, BytesN<32>, Address, i128) = latest_payload(&env, TOPIC_ESCROW_REFUNDED);
    assert_eq!(p.0, escrow.escrow_id); // field 0: escrow_id
    assert_eq!(p.1, id); // field 1: invoice_id
    assert_eq!(p.2, inv); // field 2: investor
    assert_eq!(p.3, escrow.amount); // field 3: amount
    assert_eq!(client.get_escrow_status(&id), EscrowStatus::Refunded);
}

// ============================================================================
// 16. Dispute Lifecycle — field orders
// ============================================================================

#[test]
fn test_dispute_lifecycle_field_orders() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_LIMIT);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "dispute field order");
    client.verify_invoice(&id);
    let bid_id = client.place_bid(&inv, &id, &INV_AMOUNT, &EXP_RETURN);
    client.accept_bid(&id, &bid_id);

    // DisputeCreated
    let reason = String::from_str(&env, "Amount mismatch");
    let cr_ts = env.ledger().timestamp() + 5;
    env.ledger().set_timestamp(cr_ts);
    client.create_dispute(&biz, &id, &reason);

    let p0: (BytesN<32>, Address, String, u64) = latest_payload(&env, symbol_short!("dsp_cr"));
    assert_eq!(p0.0, id); // field 0: invoice_id
    assert_eq!(p0.1, biz); // field 1: created_by
    assert_eq!(p0.2, reason); // field 2: reason
    assert_eq!(p0.3, cr_ts); // field 3: timestamp

    // DisputeUnderReview
    let ur_ts = cr_ts + 5;
    env.ledger().set_timestamp(ur_ts);
    client.put_dispute_under_review(&id);

    let p1: (BytesN<32>, Address, u64) = latest_payload(&env, symbol_short!("dsp_ur"));
    assert_eq!(p1.0, id); // field 0: invoice_id
                          // field 1: reviewed_by (admin)
    assert_eq!(p1.2, ur_ts); // field 2: timestamp

    // DisputeResolved
    let resolution = String::from_str(&env, "Resolved with partial refund");
    let rs_ts = ur_ts + 5;
    env.ledger().set_timestamp(rs_ts);
    client.resolve_dispute(&id, &resolution);

    let p2: (BytesN<32>, Address, String, u64) = latest_payload(&env, symbol_short!("dsp_rs"));
    assert_eq!(p2.0, id); // field 0: invoice_id
                          // field 1: resolved_by (admin)
    assert_eq!(p2.2, resolution); // field 2: resolution
    assert_eq!(p2.3, rs_ts); // field 3: timestamp
}

// ============================================================================
// 17. Platform Fee Updated — field order
// ============================================================================

#[test]
fn test_platform_fee_updated_field_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup(&env);

    let ts = 400u64;
    env.ledger().set_timestamp(ts);
    client.set_platform_fee(&250i128);

    // fee_upd payload: (fee_bps, updated_at, updated_by)
    let p: (i128, u64, Address) = latest_payload(&env, symbol_short!("fee_upd"));
    assert_eq!(p.0, 250i128); // field 0: fee_bps
    assert_eq!(p.1, ts); // field 1: updated_at
    assert_eq!(p.2, admin); // field 2: updated_by
}

// ============================================================================
// 18. Audit Events — field orders
// ============================================================================

#[test]
fn test_audit_events_field_orders() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, None);
    kyc_business(&env, &client, &admin, &biz);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "audit field order");

    // aud_qry payload: (query_type: String, result_count: u32)
    let filter = AuditQueryFilter {
        invoice_id: Some(id.clone()),
        operation: AuditOperationFilter::Any,
        actor: None,
        start_timestamp: None,
        end_timestamp: None,
    };
    let results = client.query_audit_logs(&filter, &50u32);
    let pq: (String, u32) = latest_payload(&env, symbol_short!("aud_qry"));
    assert_eq!(pq.0, String::from_str(&env, "query_audit_logs"));
    assert_eq!(pq.1, results.len() as u32);

    // aud_val payload: (invoice_id, is_valid, timestamp)
    let val_ts = env.ledger().timestamp() + 10;
    env.ledger().set_timestamp(val_ts);
    let is_valid = client.validate_invoice_audit_integrity(&id);
    let pv: (BytesN<32>, bool, u64) = latest_payload(&env, symbol_short!("aud_val"));
    assert_eq!(pv.0, id);
    assert_eq!(pv.1, is_valid);
    assert_eq!(pv.2, val_ts);
}

// ============================================================================
// 19. No Events Emitted for Read-Only Calls
// ============================================================================

#[test]
fn test_no_events_emitted_for_reads() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, None);
    kyc_business(&env, &client, &admin, &biz);

    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "reads no events");
    let event_count_after_upload = env.events().all().len();

    // Read-only calls — must not add events
    client.get_invoice(&id);
    client.get_business_invoices_paged(&biz, &None, &0u32, &10u32);
    client.get_platform_fee();

    assert_eq!(
        env.events().all().len(),
        event_count_after_upload,
        "read-only calls must not emit events"
    );
}

// ============================================================================
// 20. Event Ordering Across Full Lifecycle
// ============================================================================

#[test]
fn test_event_ordering_across_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let inv = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz, Some(&inv));
    kyc_business(&env, &client, &admin, &biz);
    kyc_investor(&env, &client, &inv, INV_LIMIT);

    // T=10: upload
    env.ledger().set_timestamp(10);
    let (id, _) = upload_invoice(&env, &client, &biz, &currency, "lifecycle ordering");

    // T=20: verify
    env.ledger().set_timestamp(20);
    client.verify_invoice(&id);

    // T=30: bid
    env.ledger().set_timestamp(30);
    let bid_id = client.place_bid(&inv, &id, &INV_AMOUNT, &EXP_RETURN);

    // T=40: accept → escrow created
    env.ledger().set_timestamp(40);
    client.accept_bid(&id, &bid_id);

    // Verify timestamps are strictly increasing
    let up_p: (BytesN<32>, Address, i128, Address, u64, u64) =
        latest_payload(&env, TOPIC_INVOICE_UPLOADED);
    let ver_p: (BytesN<32>, Address, u64) = latest_payload(&env, TOPIC_INVOICE_VERIFIED);
    let bid_p: (BytesN<32>, BytesN<32>, Address, i128, i128, u64, u64) =
        latest_payload(&env, TOPIC_BID_PLACED);
    let acc_p: (BytesN<32>, BytesN<32>, Address, Address, i128, i128, u64) =
        latest_payload(&env, TOPIC_BID_ACCEPTED);

    assert_eq!(up_p.5, 10u64, "upload ts");
    assert_eq!(ver_p.2, 20u64, "verify ts");
    assert_eq!(bid_p.5, 30u64, "bid ts");
    assert_eq!(acc_p.6, 40u64, "accept ts");
    assert!(up_p.5 < ver_p.2);
    assert!(ver_p.2 < bid_p.5);
    assert!(bid_p.5 < acc_p.6);
}

// ============================================================================
// Legacy Tests (retained from original test_events.rs)
// ============================================================================

#[test]
fn test_invoice_events_emit_correct_topics_and_payloads() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);
    let business = Address::generate(&env);
    let currency = mint_currency(&env, &contract_id, &business, None);
    let amount = INV_AMOUNT;
    let due_date = env.ledger().timestamp() + 86_400;

    kyc_business(&env, &client, &admin, &business);

    let upload_ts = env.ledger().timestamp();
    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice event test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert_payload(
        &env,
        symbol_short!("inv_up"),
        (
            invoice_id.clone(),
            business.clone(),
            amount,
            currency.clone(),
            due_date,
            upload_ts,
        ),
    );

    let verify_ts = upload_ts + 10;
    env.ledger().set_timestamp(verify_ts);
    client.verify_invoice(&invoice_id);
    assert_payload(
        &env,
        symbol_short!("inv_ver"),
        (invoice_id.clone(), business.clone(), verify_ts),
    );

    let cancel_ts = verify_ts + 10;
    env.ledger().set_timestamp(cancel_ts);
    client.cancel_invoice(&invoice_id);
    assert_payload(
        &env,
        symbol_short!("inv_canc"),
        (invoice_id.clone(), business.clone(), cancel_ts),
    );
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Cancelled
    );
}

#[test]
fn test_bid_placed_and_withdrawn_events_emit_correct_payloads() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = mint_currency(&env, &contract_id, &business, Some(&investor));
    let due_date = env.ledger().timestamp() + 86_400;

    kyc_business(&env, &client, &admin, &business);
    kyc_investor(&env, &client, &investor, INV_LIMIT);

    let invoice_id = client.upload_invoice(
        &business,
        &INV_AMOUNT,
        &currency,
        &due_date,
        &String::from_str(&env, "Bid events test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let placed_ts = 100u64;
    env.ledger().set_timestamp(placed_ts);
    let bid_id = client.place_bid(&investor, &invoice_id, &INV_AMOUNT, &EXP_RETURN);

    let bid_placed_payload: (BytesN<32>, BytesN<32>, Address, i128, i128, u64, u64) =
        latest_payload(&env, symbol_short!("bid_plc"));
    assert_eq!(bid_placed_payload.0, bid_id.clone());
    assert_eq!(bid_placed_payload.1, invoice_id.clone());
    assert_eq!(bid_placed_payload.2, investor.clone());
    assert_eq!(bid_placed_payload.3, INV_AMOUNT);
    assert_eq!(bid_placed_payload.4, EXP_RETURN);
    assert_eq!(bid_placed_payload.5, placed_ts);
    assert_eq!(
        bid_placed_payload.6,
        crate::bid::Bid::default_expiration(placed_ts)
    );

    let withdraw_ts = 120u64;
    env.ledger().set_timestamp(withdraw_ts);
    client.withdraw_bid(&bid_id);
    assert_payload(
        &env,
        symbol_short!("bid_wdr"),
        (
            bid_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            INV_AMOUNT,
            withdraw_ts,
        ),
    );
    assert_eq!(
        client.get_bid(&bid_id).unwrap().status,
        crate::bid::BidStatus::Withdrawn
    );
}

#[test]
fn test_bid_accepted_and_escrow_created_events_emit_correct_payloads() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = mint_currency(&env, &contract_id, &business, Some(&investor));
    let due_date = env.ledger().timestamp() + 86_400;

    kyc_business(&env, &client, &admin, &business);
    kyc_investor(&env, &client, &investor, INV_LIMIT);

    let invoice_id = client.upload_invoice(
        &business,
        &INV_AMOUNT,
        &currency,
        &due_date,
        &String::from_str(&env, "Bid accepted event test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &INV_AMOUNT, &EXP_RETURN);
    let accepted_ts = 200u64;
    env.ledger().set_timestamp(accepted_ts);
    client.accept_bid(&invoice_id, &bid_id);

    let bid_accepted_payload: (BytesN<32>, BytesN<32>, Address, Address, i128, i128, u64) =
        latest_payload(&env, symbol_short!("bid_acc"));
    let escrow_created_payload: (BytesN<32>, BytesN<32>, Address, Address, i128) =
        latest_payload(&env, symbol_short!("esc_cr"));
    let escrow = client.get_escrow_details(&invoice_id);

    assert_eq!(bid_accepted_payload.0, bid_id.clone());
    assert_eq!(bid_accepted_payload.1, invoice_id.clone());
    assert_eq!(bid_accepted_payload.2, investor.clone());
    assert_eq!(bid_accepted_payload.3, business.clone());
    assert_eq!(bid_accepted_payload.4, INV_AMOUNT);
    assert_eq!(bid_accepted_payload.5, EXP_RETURN);
    assert_eq!(bid_accepted_payload.6, accepted_ts);
    assert_eq!(escrow_created_payload.0, escrow.escrow_id.clone());
    assert_eq!(escrow_created_payload.1, invoice_id.clone());
    assert_eq!(escrow_created_payload.2, investor.clone());
    assert_eq!(escrow_created_payload.3, business.clone());
    assert_eq!(escrow_created_payload.4, escrow.amount);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );
}

#[test]
fn test_escrow_released_event_emits_correct_topic_and_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = mint_currency(&env, &contract_id, &business, Some(&investor));
    let due_date = env.ledger().timestamp() + 86_400;

    kyc_business(&env, &client, &admin, &business);
    kyc_investor(&env, &client, &investor, INV_LIMIT);

    let invoice_id = client.upload_invoice(
        &business,
        &INV_AMOUNT,
        &currency,
        &due_date,
        &String::from_str(&env, "Escrow release event test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &INV_AMOUNT, &EXP_RETURN);
    client.accept_bid(&invoice_id, &bid_id);
    let escrow = client.get_escrow_details(&invoice_id);
    client.release_escrow_funds(&invoice_id);

    assert_payload(
        &env,
        symbol_short!("esc_rel"),
        (
            escrow.escrow_id.clone(),
            invoice_id.clone(),
            business.clone(),
            escrow.amount,
        ),
    );
    assert_eq!(
        client.get_escrow_status(&invoice_id),
        EscrowStatus::Released
    );
}

#[test]
fn test_invoice_defaulted_event_emits_correct_topic_and_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = mint_currency(&env, &contract_id, &business, Some(&investor));
    let due_date = env.ledger().timestamp() + 86_400;

    kyc_business(&env, &client, &admin, &business);
    kyc_investor(&env, &client, &investor, INV_LIMIT);

    let invoice_id = client.upload_invoice(
        &business,
        &INV_AMOUNT,
        &currency,
        &due_date,
        &String::from_str(&env, "Default event test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &INV_AMOUNT, &EXP_RETURN);
    client.accept_bid(&invoice_id, &bid_id);

    let default_ts = due_date + 1;
    env.ledger().set_timestamp(default_ts);
    client.handle_default(&invoice_id);

    assert_payload(
        &env,
        symbol_short!("inv_def"),
        (
            invoice_id.clone(),
            business.clone(),
            investor.clone(),
            default_ts,
        ),
    );
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

#[test]
fn test_audit_events_emit_correct_topics_and_payloads() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);
    let business = Address::generate(&env);
    let currency = mint_currency(&env, &contract_id, &business, None);
    let due_date = env.ledger().timestamp() + 86_400;
    kyc_business(&env, &client, &admin, &business);

    let invoice_id = client.upload_invoice(
        &business,
        &INV_AMOUNT,
        &currency,
        &due_date,
        &String::from_str(&env, "Audit events test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let filter = AuditQueryFilter {
        invoice_id: Some(invoice_id.clone()),
        operation: AuditOperationFilter::Any,
        actor: None,
        start_timestamp: None,
        end_timestamp: None,
    };
    let results = client.query_audit_logs(&filter, &50u32);
    assert_payload(
        &env,
        symbol_short!("aud_qry"),
        (
            String::from_str(&env, "query_audit_logs"),
            results.len() as u32,
        ),
    );

    let validation_ts = 300u64;
    env.ledger().set_timestamp(validation_ts);
    let is_valid = client.validate_invoice_audit_integrity(&invoice_id);
    assert_payload(
        &env,
        symbol_short!("aud_val"),
        (invoice_id.clone(), is_valid, validation_ts),
    );
}

#[test]
fn test_platform_fee_updated_event_emits_correct_topic_and_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup(&env);
    let update_ts = 400u64;
    env.ledger().set_timestamp(update_ts);
    client.set_platform_fee(&250i128);
    assert_payload(
        &env,
        symbol_short!("fee_upd"),
        (250i128, update_ts, admin.clone()),
    );
    assert_eq!(client.get_platform_fee().fee_bps, 250i128);
}

#[test]
fn test_event_timestamp_ordering() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = mint_currency(&env, &contract_id, &business, Some(&investor));
    let due_date = env.ledger().timestamp() + 86400;

    kyc_business(&env, &client, &admin, &business);
    kyc_investor(&env, &client, &investor, INV_LIMIT);

    let time_upload = env.ledger().timestamp();
    let invoice_id = client.upload_invoice(
        &business,
        &INV_AMOUNT,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    env.ledger().set_timestamp(time_upload + 1000);
    let time_verify = env.ledger().timestamp();
    client.verify_invoice(&invoice_id);

    env.ledger().set_timestamp(time_verify + 1000);
    let time_bid = env.ledger().timestamp();
    let bid_id = client.place_bid(&investor, &invoice_id, &INV_AMOUNT, &EXP_RETURN);

    let invoice = client.get_invoice(&invoice_id);
    let bid = client.get_bid(&bid_id).unwrap();

    assert!(invoice.created_at <= time_verify);
    assert!(bid.timestamp >= time_bid);
}

// Helper used only in this test module — suppress unused warning
#[allow(dead_code)]
fn _use_count_events(env: &Env) {
    let _ = count_events_with_topic(env, symbol_short!("inv_up"));
}
