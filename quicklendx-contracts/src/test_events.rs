/// Event payload validation tests for critical protocol operations.
///
/// These tests assert exact Soroban event topics and payload tuples for:
/// - Invoice lifecycle (uploaded/verified/cancelled/defaulted)
/// - Bid lifecycle (placed/accepted/withdrawn)
/// - Escrow lifecycle (created/released)
/// - Audit events (query/integrity validation)
/// - Platform fee configuration updates
use super::*;
use crate::audit::{AuditOperationFilter, AuditQueryFilter};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::EscrowStatus;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    token, Address, Env, String, TryFromVal, Val, Vec,
};

fn setup_contract(env: &Env) -> (QuickLendXContractClient, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    env.ledger().set_timestamp(1);
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.set_admin(&admin);
    (client, admin, contract_id)
}

fn verify_business_for_test(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
) {
    client.submit_kyc_application(business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, business);
}

fn verify_investor_for_test(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    limit: i128,
) {
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(investor, &limit);
}

fn init_currency_for_test(
    env: &Env,
    contract_id: &Address,
    business: &Address,
    investor: Option<&Address>,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);

    let initial_balance = 10_000i128;
    sac_client.mint(business, &initial_balance);
    sac_client.mint(contract_id, &1i128);

    if let Some(inv) = investor {
        sac_client.mint(inv, &initial_balance);
        let expiration = env.ledger().sequence() + 1_000;
        token_client.approve(business, contract_id, &initial_balance, &expiration);
        token_client.approve(inv, contract_id, &initial_balance, &expiration);
    }

    currency
}

fn latest_event_payload<T>(env: &Env, topic: soroban_sdk::Symbol) -> T
where
    T: TryFromVal<Env, Val> + core::fmt::Debug + PartialEq,
{
    let events = env.events().all();

    let mut index = events.len();
    while index > 0 {
        index -= 1;
        let (_, topics, data): (_, soroban_sdk::Vec<Val>, Val) = events.get(index).unwrap();
        if topics.is_empty() {
            continue;
        }

        let mut topic_found = false;
        for topic_part in topics.iter() {
            if let Ok(actual_topic) = soroban_sdk::Symbol::try_from_val(env, &topic_part) {
                if actual_topic == topic {
                    topic_found = true;
                    break;
                }
            }
        }

        if topic_found {
            return T::try_from_val(env, &data)
                .expect("event payload should decode to expected type");
        }
    }

    panic!("expected event topic not found: {:?}; events: {:?}", topic, events);
}

fn assert_latest_event_payload<T>(env: &Env, topic: soroban_sdk::Symbol, expected_payload: T)
where
    T: TryFromVal<Env, Val> + core::fmt::Debug + PartialEq,
{
    let actual_payload: T = latest_event_payload(env, topic);
    assert_eq!(actual_payload, expected_payload);
}

#[test]
fn test_invoice_events_emit_correct_topics_and_payloads() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup_contract(&env);
    let business = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, None);
    let amount = 1000i128;
    let due_date = env.ledger().timestamp() + 86_400;

    verify_business_for_test(&env, &client, &admin, &business);

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

    assert_latest_event_payload(
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

    assert_latest_event_payload(
        &env,
        symbol_short!("inv_ver"),
        (invoice_id.clone(), business.clone(), verify_ts),
    );

    let cancel_ts = verify_ts + 10;
    env.ledger().set_timestamp(cancel_ts);
    client.cancel_invoice(&invoice_id);

    assert_latest_event_payload(
        &env,
        symbol_short!("inv_canc"),
        (invoice_id.clone(), business.clone(), cancel_ts),
    );

    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Cancelled);
}

#[test]
fn test_bid_placed_and_withdrawn_events_emit_correct_payloads() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup_contract(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, Some(&investor));
    let amount = 1000i128;
    let due_date = env.ledger().timestamp() + 86_400;

    verify_business_for_test(&env, &client, &admin, &business);
    verify_investor_for_test(&env, &client, &investor, 5_000i128);

    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Bid events test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_amount = 1000i128;
    let expected_return = 1100i128;
    let placed_ts = 100u64;
    env.ledger().set_timestamp(placed_ts);
    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &expected_return);

    let bid_placed_payload: (BytesN<32>, BytesN<32>, Address, i128, i128, u64, u64) =
        latest_event_payload(&env, symbol_short!("bid_plc"));

    assert_eq!(bid_placed_payload.0, bid_id.clone());
    assert_eq!(bid_placed_payload.1, invoice_id.clone());
    assert_eq!(bid_placed_payload.2, investor.clone());
    assert_eq!(bid_placed_payload.3, bid_amount);
    assert_eq!(bid_placed_payload.4, expected_return);
    assert_eq!(bid_placed_payload.5, placed_ts);
    assert_eq!(bid_placed_payload.6, crate::bid::Bid::default_expiration(placed_ts));

    let withdraw_ts = 120u64;
    env.ledger().set_timestamp(withdraw_ts);
    client.withdraw_bid(&bid_id);

    assert_latest_event_payload(
        &env,
        symbol_short!("bid_wdr"),
        (
            bid_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            bid_amount,
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
    let (client, admin, contract_id) = setup_contract(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, Some(&investor));
    let amount = 1000i128;
    let due_date = env.ledger().timestamp() + 86_400;

    verify_business_for_test(&env, &client, &admin, &business);
    verify_investor_for_test(&env, &client, &investor, 5_000i128);

    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Bid accepted event test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    let accepted_ts = 200u64;
    env.ledger().set_timestamp(accepted_ts);
    client.accept_bid(&invoice_id, &bid_id);

    let bid_accepted_payload: (BytesN<32>, BytesN<32>, Address, Address, i128, i128, u64) =
        latest_event_payload(&env, symbol_short!("bid_acc"));
    let escrow_created_payload: (BytesN<32>, BytesN<32>, Address, Address, i128) =
        latest_event_payload(&env, symbol_short!("esc_cr"));

    let escrow = client.get_escrow_details(&invoice_id);

    assert_eq!(bid_accepted_payload.0, bid_id.clone());
    assert_eq!(bid_accepted_payload.1, invoice_id.clone());
    assert_eq!(bid_accepted_payload.2, investor.clone());
    assert_eq!(bid_accepted_payload.3, business.clone());
    assert_eq!(bid_accepted_payload.4, 1000i128);
    assert_eq!(bid_accepted_payload.5, 1100i128);
    assert_eq!(bid_accepted_payload.6, accepted_ts);

    assert_eq!(escrow_created_payload.0, escrow.escrow_id.clone());
    assert_eq!(escrow_created_payload.1, invoice_id.clone());
    assert_eq!(escrow_created_payload.2, investor.clone());
    assert_eq!(escrow_created_payload.3, business.clone());
    assert_eq!(escrow_created_payload.4, escrow.amount);

    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Funded);
}

#[test]
fn test_escrow_released_event_emits_correct_topic_and_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup_contract(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, Some(&investor));
    let amount = 1000i128;
    let due_date = env.ledger().timestamp() + 86_400;

    verify_business_for_test(&env, &client, &admin, &business);
    verify_investor_for_test(&env, &client, &investor, 5_000i128);

    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Escrow release event test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 100));
    client.accept_bid(&invoice_id, &bid_id);

    let escrow = client.get_escrow_details(&invoice_id);
    client.release_escrow_funds(&invoice_id);

    assert_latest_event_payload(
        &env,
        symbol_short!("esc_rel"),
        (
            escrow.escrow_id.clone(),
            invoice_id.clone(),
            business.clone(),
            escrow.amount,
        ),
    );

    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Released);
}

#[test]
fn test_invoice_defaulted_event_emits_correct_topic_and_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup_contract(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, Some(&investor));
    let amount = 1000i128;
    let due_date = env.ledger().timestamp() + 86_400;

    verify_business_for_test(&env, &client, &admin, &business);
    verify_investor_for_test(&env, &client, &investor, 5_000i128);

    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Default event test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 100));
    client.accept_bid(&invoice_id, &bid_id);

    let default_ts = due_date + 1;
    env.ledger().set_timestamp(default_ts);
    client.handle_default(&invoice_id);

    assert_latest_event_payload(
        &env,
        symbol_short!("inv_def"),
        (
            invoice_id.clone(),
            business.clone(),
            investor.clone(),
            default_ts,
        ),
    );

    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Defaulted);
}

#[test]
fn test_audit_events_emit_correct_topics_and_payloads() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, contract_id) = setup_contract(&env);
    let business = Address::generate(&env);
    let currency = init_currency_for_test(&env, &contract_id, &business, None);
    let due_date = env.ledger().timestamp() + 86_400;

    verify_business_for_test(&env, &client, &admin, &business);

    let invoice_id = client.upload_invoice(
        &business,
        &1000i128,
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
    assert_latest_event_payload(
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

    assert_latest_event_payload(
        &env,
        symbol_short!("aud_val"),
        (invoice_id.clone(), is_valid, validation_ts),
    );
}

#[test]
fn test_platform_fee_updated_event_emits_correct_topic_and_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _contract_id) = setup_contract(&env);

    let update_ts = 400u64;
    env.ledger().set_timestamp(update_ts);
    client.set_platform_fee(&250i128);

    assert_latest_event_payload(
        &env,
        symbol_short!("fee_upd"),
        (250i128, update_ts, admin.clone()),
    );

    assert_eq!(client.get_platform_fee().fee_bps, 250i128);
}
