#![cfg(test)]

use crate::audit::{AuditOperation, AuditQueryFilter, AuditStorage};
use crate::contract::QuickLendXContractClient;
use crate::errors::QuickLendXError;
use crate::events::{
    BidAccepted, BidPlaced, EscrowCreated, EscrowRefunded, EscrowReleased, InvoiceFunded,
    TOPIC_BID_ACCEPTED, TOPIC_BID_PLACED, TOPIC_ESCROW_CREATED, TOPIC_ESCROW_REFUNDED,
    TOPIC_ESCROW_RELEASED, TOPIC_INVOICE_FUNDED,
};
use crate::payments::EscrowStatus;
use crate::types::{BidStatus, InvestmentStatus, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, Address, BytesN, Env, String, Symbol, Vec,
};

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

    let contract_id = env.register_contract(None, crate::contract::QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    client.submit_business_kyc(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&business);

    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
    client.verify_investor(&investor, &100_000i128);

    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = token::Client::new(&env, &token_id.address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id.address);

    client.add_allowed_currency(&token_id.address);

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
        amount,
        &(env.ledger().timestamp() + 86400),
        token_address,
        &String::from_str(env, "Test Invoice"),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

#[test]
fn test_funding_events_and_audit_parity_success_flow() {
    let (env, _contract_id, client, _admin, business, investor, token_client, token_admin_client) =
        setup_test_env();

    let invoice_amount = 20_000i128;
    let invoice_id = create_verified_invoice(
        &env,
        &client,
        &business,
        &token_client.address,
        invoice_amount,
    );

    token_admin_client.mint(&investor, &50_000i128);

    // 1. Place Bid
    let bid_amount = 20_000i128;
    let expected_return = 22_000i128;
    let salt = BytesN::from_array(&env, &[1u8; 32]);
    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &expected_return, &salt);

    // Verify BidPlaced event & audit
    let events = env.events().all();
    assert!(events.iter().any(|e| e
        .1
        .iter()
        .any(|v| Symbol::try_from_val(&env, &v) == Ok(TOPIC_BID_PLACED))));

    let filter = AuditQueryFilter {
        invoice_id: Some(invoice_id.clone()),
        operation: Some(AuditOperation::BidPlaced),
        actor: Some(investor.clone()),
        start_time: None,
        end_time: None,
    };
    let audit_entries = client.query_audit_logs(&filter, &10);
    assert_eq!(audit_entries.len(), 1);
    assert_eq!(audit_entries.get(0).unwrap().amount, Some(bid_amount));

    // 2. Accept Bid and Fund
    let escrow_id = client.accept_bid_and_fund(&invoice_id, &bid_id);

    // Verify events: EscrowCreated, BidAccepted, InvoiceFunded
    let events = env.events().all();
    assert!(events.iter().any(|e| e
        .1
        .iter()
        .any(|v| Symbol::try_from_val(&env, &v) == Ok(TOPIC_ESCROW_CREATED))));
    assert!(events.iter().any(|e| e
        .1
        .iter()
        .any(|v| Symbol::try_from_val(&env, &v) == Ok(TOPIC_BID_ACCEPTED))));
    assert!(events.iter().any(|e| e
        .1
        .iter()
        .any(|v| Symbol::try_from_val(&env, &v) == Ok(TOPIC_INVOICE_FUNDED))));

    // Verify audit logs for funding transitions
    let escrow_audit = client.query_audit_logs(
        &AuditQueryFilter {
            invoice_id: Some(invoice_id.clone()),
            operation: Some(AuditOperation::EscrowCreated),
            actor: None,
            start_time: None,
            end_time: None,
        },
        &10,
    );
    assert_eq!(escrow_audit.len(), 1);

    let accepted_audit = client.query_audit_logs(
        &AuditQueryFilter {
            invoice_id: Some(invoice_id.clone()),
            operation: Some(AuditOperation::BidAccepted),
            actor: None,
            start_time: None,
            end_time: None,
        },
        &10,
    );
    assert_eq!(accepted_audit.len(), 1);

    let funded_audit = client.query_audit_logs(
        &AuditQueryFilter {
            invoice_id: Some(invoice_id.clone()),
            operation: Some(AuditOperation::InvoiceFunded),
            actor: None,
            start_time: None,
            end_time: None,
        },
        &10,
    );
    assert_eq!(funded_audit.len(), 1);

    // Verify chain integrity
    assert!(client.verify_audit_chain(&invoice_id));
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

    let audit_count_before = client
        .query_audit_logs(
            &AuditQueryFilter {
                invoice_id: Some(inv.clone()),
                operation: None,
                actor: None,
                start_time: None,
                end_time: None,
            },
            &100,
        )
        .len();

    // Duplicate funding attempt must fail
    let duplicate_res = client.try_accept_bid_and_fund(&inv, &bid_id);
    assert!(duplicate_res.is_err());

    let audit_count_after = client
        .query_audit_logs(
            &AuditQueryFilter {
                invoice_id: Some(inv.clone()),
                operation: None,
                actor: None,
                start_time: None,
                end_time: None,
            },
            &100,
        )
        .len();

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

    client.refund_escrow_funds(&inv);

    // Verify EscrowRefunded event and audit
    let refund_audit = client.query_audit_logs(
        &AuditQueryFilter {
            invoice_id: Some(inv.clone()),
            operation: Some(AuditOperation::EscrowRefunded),
            actor: None,
            start_time: None,
            end_time: None,
        },
        &10,
    );
    assert_eq!(refund_audit.len(), 1);
    assert!(client.verify_audit_chain(&inv));
}
