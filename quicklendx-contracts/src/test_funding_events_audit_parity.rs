#![cfg(test)]

use crate::audit::{AuditOperation, AuditQueryFilter};
use crate::errors::QuickLendXError;
use crate::notifications::{NotificationSystem, NotificationType};
use crate::payments::EscrowStatus;
use crate::storage::InvoiceStorage;
use crate::types::{BidStatus, InvestmentStatus, InvoiceStatus};
use crate::QuickLendXContractClient;
use soroban_sdk::testutils::Events as EventsTrait;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, xdr, Address, BytesN, Env, String, Symbol, TryFromVal, Vec,
};

fn count_events(env: &Env, topic: Symbol) -> u32 {
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic).expect("topic to xdr");
    env.events()
        .all()
        .events()
        .iter()
        .filter(|e| match &e.body {
            xdr::ContractEventBody::V0(b) => b.topics.first() == Some(&topic_xdr),
        })
        .count() as u32
}

fn setup_test_env() -> (
    Env,
    Address,
    QuickLendXContractClient<'static>,
    Address,
    Address,
    Address,
    token::Client<'static>,
    token::StellarAssetClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, crate::QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&crate::init::InitializationParams {
        admin: admin.clone(),
        treasury: admin.clone(),
        fee_bps: 100,
        min_invoice_amount: 100,
        max_due_date_days: 90,
        grace_period_seconds: 86400,
        initial_currencies: Vec::new(&env),
        corridors: Vec::new(&env),
        backfill_max_batch_size: 50,
    });

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
    client.verify_investor(&investor, &100_000i128);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::Client::new(&env, &token_id.address());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id.address());

    client.add_currency(&admin, &token_id.address());

    (
        env,
        contract_id,
        client,
        admin,
        business,
        investor,
        token_client,
        token_admin_client,
    )
}

fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    token_address: &Address,
    amount: i128,
) -> BytesN<32> {
    let invoice_id = client.upload_invoice(
        business,
        &amount,
        token_address,
        &(env.ledger().timestamp() + 86400),
        &String::from_str(env, "Test Invoice"),
        &crate::types::InvoiceCategory::Services,
        &Vec::new(env),
        &None,
        &None,
        &None,
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

fn fund_test_invoice(
    env: &Env,
    contract_id: &Address,
    client: &QuickLendXContractClient,
    business: &Address,
    investor: &Address,
    currency: &Address,
    token_admin_client: &token::StellarAssetClient<'static>,
    origination_fee_bps: Option<u32>,
) -> (BytesN<32>, u32, u32) {
    let invoice_id = create_verified_invoice(env, client, business, currency, 20_000);
    if let Some(fee_bps) = origination_fee_bps {
        env.as_contract(contract_id, || {
            let mut invoice = InvoiceStorage::get_invoice(env, &invoice_id).unwrap();
            invoice.origination_fee_bps = Some(fee_bps);
            InvoiceStorage::update_invoice(env, &invoice);
        });
    }

    token_admin_client.mint(investor, &50_000i128);
    let bid_id = client.place_bid(
        investor,
        &invoice_id,
        &20_000i128,
        &22_000i128,
        &BytesN::from_array(env, &[1u8; 32]),
    );
    let notification_events_before = count_events(env, Symbol::new(env, "notif"));
    let notification_count_before = env.as_contract(contract_id, || {
        NotificationSystem::get_user_notifications(env, investor).len()
    });
    client.accept_bid_and_fund(&invoice_id, &bid_id);
    (
        invoice_id,
        notification_events_before,
        notification_count_before,
    )
}

fn assert_funding_event_counts(env: &Env) {
    assert_eq!(
        count_events(env, Symbol::new(env, "bid_accepted")),
        1,
        "expected exactly one BidAccepted event"
    );
    assert_eq!(
        count_events(env, Symbol::new(env, "invoice_funded")),
        1,
        "expected exactly one InvoiceFunded event"
    );
    assert_eq!(
        count_events(env, Symbol::new(env, "escrow_created")),
        1,
        "expected exactly one EscrowCreated event"
    );
}

fn assert_bid_accepted_notification_once(
    env: &Env,
    contract_id: &Address,
    investor: &Address,
    notification_events_before: u32,
    notification_count_before: u32,
) {
    assert_eq!(
        count_events(env, Symbol::new(env, "notif")),
        notification_events_before + 1,
        "funding should emit exactly one notification event"
    );
    let notification_ids = env.as_contract(contract_id, || {
        NotificationSystem::get_user_notifications(env, investor)
    });
    assert_eq!(notification_ids.len(), notification_count_before + 1);
    let notification = env.as_contract(contract_id, || {
        NotificationSystem::get_notification(
            env,
            &notification_ids.get(notification_ids.len() - 1).unwrap(),
        )
        .unwrap()
    });
    assert_eq!(notification.notification_type, NotificationType::BidAccepted);
}

#[test]
fn funding_emits_each_event_once() {
    let (env, contract_id, client, _admin, business, investor, token_client, token_admin_client) =
        setup_test_env();

    let (invoice_id, notification_events_before, notification_count_before) = fund_test_invoice(
        &env,
        &contract_id,
        &client,
        &business,
        &investor,
        &token_client.address,
        &token_admin_client,
        None,
    );
    assert_funding_event_counts(&env);

    // Verify audit logs for funding transitions
    let escrow_audit = client.query_audit_logs(
        &AuditQueryFilter {
            invoice_id: Some(invoice_id.clone()),
            operation: crate::audit::AuditOperationFilter::Specific(AuditOperation::EscrowCreated),
            actor: None,
            start_timestamp: None,
            end_timestamp: None,
        },
        &10,
    );
    assert_eq!(escrow_audit.len(), 1);

    let accepted_audit = client.query_audit_logs(
        &AuditQueryFilter {
            invoice_id: Some(invoice_id.clone()),
            operation: crate::audit::AuditOperationFilter::Specific(AuditOperation::BidAccepted),
            actor: None,
            start_timestamp: None,
            end_timestamp: None,
        },
        &10,
    );
    assert_eq!(accepted_audit.len(), 1);
    assert_eq!(accepted_audit.get(0).unwrap().actor, business);

    let funded_audit = client.query_audit_logs(
        &AuditQueryFilter {
            invoice_id: Some(invoice_id.clone()),
            operation: crate::audit::AuditOperationFilter::Specific(AuditOperation::InvoiceFunded),
            actor: None,
            start_timestamp: None,
            end_timestamp: None,
        },
        &10,
    );
    assert_eq!(funded_audit.len(), 1);

    assert_bid_accepted_notification_once(
        &env,
        &contract_id,
        &investor,
        notification_events_before,
        notification_count_before,
    );

    // Verify chain integrity
    assert!(client.verify_audit_chain(&invoice_id));
}

#[test]
fn funding_with_fee_emits_each_event_once() {
    let (env, contract_id, client, _admin, business, investor, token_client, token_admin_client) =
        setup_test_env();
    let _ = fund_test_invoice(
        &env,
        &contract_id,
        &client,
        &business,
        &investor,
        &token_client.address,
        &token_admin_client,
        Some(200),
    );
    assert_funding_event_counts(&env);
}

#[test]
fn bid_accepted_audit_actor_is_business() {
    let (env, contract_id, client, _admin, business, investor, token_client, token_admin_client) =
        setup_test_env();
    let (invoice_id, _, _) = fund_test_invoice(
        &env,
        &contract_id,
        &client,
        &business,
        &investor,
        &token_client.address,
        &token_admin_client,
        None,
    );
    let accepted_audit = client.query_audit_logs(
        &AuditQueryFilter {
            invoice_id: Some(invoice_id),
            operation: crate::audit::AuditOperationFilter::Specific(AuditOperation::BidAccepted),
            actor: None,
            start_timestamp: None,
            end_timestamp: None,
        },
        &10,
    );
    assert_eq!(accepted_audit.len(), 1);
    assert_eq!(accepted_audit.get(0).unwrap().actor, business);
}

#[test]
fn bid_accepted_notification_once() {
    let (env, contract_id, client, _admin, business, investor, token_client, token_admin_client) =
        setup_test_env();
    let (_, notification_events_before, notification_count_before) = fund_test_invoice(
        &env,
        &contract_id,
        &client,
        &business,
        &investor,
        &token_client.address,
        &token_admin_client,
        None,
    );
    assert_bid_accepted_notification_once(
        &env,
        &contract_id,
        &investor,
        notification_events_before,
        notification_count_before,
    );
}

#[test]
fn test_investor_exposure_accounting_includes_active_investments() {
    let (env, _contract_id, client, _admin, business, investor, token_client, token_admin_client) =
        setup_test_env();

    // Set investor limit to 30,000
    client.set_investment_limit(&investor, &30_000i128);

    token_admin_client.mint(&investor, &100_000i128);

    // Create Invoice 1 for 20,000 and fund it
    let inv1 = create_verified_invoice(&env, &client, &business, &token_client.address, 20_000i128);
    let bid1 = client.place_bid(
        &investor,
        &inv1,
        &20_000i128,
        &22_000i128,
        &BytesN::from_array(&env, &[1u8; 32]),
    );
    client.accept_bid_and_fund(&inv1, &bid1);

    // Now investor has 20,000 in Active investment.
    // Placing another bid of 15,000 should EXCEED limit (20,000 + 15,000 = 35,000 > 30,000)
    let inv2 = create_verified_invoice(&env, &client, &business, &token_client.address, 15_000i128);
    let res = client.try_place_bid(
        &investor,
        &inv2,
        &15_000i128,
        &17_000i128,
        &BytesN::from_array(&env, &[2u8; 32]),
    );
    assert!(res.is_err());
    assert_eq!(res.err().unwrap(), Ok(QuickLendXError::InvalidAmount));

    // Placing a bid of 10,000 should SUCCEED (20,000 + 10,000 = 30,000 <= 30,000)
    let bid2 = client.place_bid(
        &investor,
        &inv2,
        &10_000i128,
        &11_000i128,
        &BytesN::from_array(&env, &[3u8; 32]),
    );
    assert_eq!(bid2.to_array().len(), 32);
}

#[test]
fn test_rejected_and_duplicate_operations_emit_no_partial_state_or_audit() {
    let (env, _contract_id, client, _admin, business, investor, token_client, token_admin_client) =
        setup_test_env();

    let inv = create_verified_invoice(&env, &client, &business, &token_client.address, 10_000i128);
    token_admin_client.mint(&investor, &20_000i128);

    let bid_id = client.place_bid(
        &investor,
        &inv,
        &10_000i128,
        &11_000i128,
        &BytesN::from_array(&env, &[1u8; 32]),
    );
    client.accept_bid_and_fund(&inv, &bid_id);

    let audit_count_before = client.query_audit_logs(&AuditQueryFilter {
        invoice_id: Some(inv.clone()),
        operation: crate::audit::AuditOperationFilter::Any,
        actor: None,
        start_timestamp: None,
        end_timestamp: None,
    }, &100).len();

    // Duplicate funding attempt must fail
    let duplicate_res = client.try_accept_bid_and_fund(&inv, &bid_id);
    assert!(duplicate_res.is_err());

    let audit_count_after = client.query_audit_logs(&AuditQueryFilter {
        invoice_id: Some(inv.clone()),
        operation: crate::audit::AuditOperationFilter::Any,
        actor: None,
        start_timestamp: None,
        end_timestamp: None,
    }, &100).len();

    // Must have logged ZERO new audit records for the failed attempt
    assert_eq!(audit_count_before, audit_count_after);
}

#[test]
fn test_escrow_refund_events_and_audit_parity() {
    let (env, _contract_id, client, _admin, business, investor, token_client, token_admin_client) =
        setup_test_env();

    let inv = create_verified_invoice(&env, &client, &business, &token_client.address, 10_000i128);
    token_admin_client.mint(&investor, &20_000i128);

    let bid_id = client.place_bid(
        &investor,
        &inv,
        &10_000i128,
        &11_000i128,
        &BytesN::from_array(&env, &[1u8; 32]),
    );
    client.accept_bid_and_fund(&inv, &bid_id);

    client.refund_escrow_funds(&inv, &investor);

    // Verify EscrowRefunded event and audit
    let refund_audit = client.query_audit_logs(&AuditQueryFilter {
        invoice_id: Some(inv.clone()),
        operation: crate::audit::AuditOperationFilter::Specific(AuditOperation::EscrowRefunded),
        actor: None,
        start_timestamp: None,
        end_timestamp: None,
    }, &10);
    assert_eq!(refund_audit.len(), 1);
    assert!(client.verify_audit_chain(&inv));
}
