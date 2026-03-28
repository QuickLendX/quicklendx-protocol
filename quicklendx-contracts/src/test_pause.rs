#![cfg(test)]

use crate::emergency::DEFAULT_EMERGENCY_TIMELOCK_SECS;
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env, String, Vec};

fn setup(
    env: &Env,
) -> (
    QuickLendXContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let currency = Address::generate(env);
    client.initialize_admin(&admin);
    (client, admin, business, investor, currency)
}

fn submit_business_kyc(env: &Env, client: &QuickLendXContractClient, business: &Address) {
    client.submit_kyc_application(business, &String::from_str(env, "Business KYC"));
}

fn submit_investor_kyc(env: &Env, client: &QuickLendXContractClient, investor: &Address) {
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
}

#[test]
fn test_pause_blocks_user_and_invoice_state_mutations() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.pause(&admin);
    assert!(client.is_paused());

    let upload_err = client
        .try_store_invoice(
            &business,
            &1_000i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Blocked"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .err()
        .expect("pause should block invoice creation")
        .expect("contract error");
    assert_eq!(upload_err, QuickLendXError::OperationNotAllowed);

    let verify_err = client
        .try_verify_invoice(&invoice_id)
        .err()
        .expect("pause should block invoice verification")
        .expect("contract error");
    assert_eq!(verify_err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_pause_allows_governance_configuration_updates() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    client.pause(&admin);

    assert_eq!(client.set_bid_ttl_days(&14), 14);

    client.set_platform_fee(&250i128);
    assert_eq!(client.get_platform_fee().fee_bps, 250);

    client.add_currency(&admin, &currency);
    assert!(client.is_allowed_currency(&currency));

    client.update_protocol_limits(&admin, &25i128, &45u64, &3_600u64);

    client.unpause(&admin);

    let below_limit_err = client
        .try_store_invoice(
            &business,
            &24i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Below min"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .err()
        .expect("updated limits should affect later invoice validation")
        .expect("contract error");
    assert_eq!(below_limit_err, QuickLendXError::InvalidAmount);
}

#[test]
fn test_pause_allows_kyc_review_admin_operations() {
    let env = Env::default();
    let (client, admin, business, investor, _currency) = setup(&env);

    submit_business_kyc(&env, &client, &business);
    submit_investor_kyc(&env, &client, &investor);

    client.pause(&admin);

    client.verify_business(&admin, &business);
    client.verify_investor(&investor, &1_500i128);

    let business_status = client
        .get_business_verification_status(&business)
        .expect("business verification");
    let investor_status = client
        .get_investor_verification(&investor)
        .expect("investor verification");

    assert!(matches!(
        business_status.status,
        crate::verification::BusinessVerificationStatus::Verified
    ));
    assert!(matches!(
        investor_status.status,
        crate::verification::BusinessVerificationStatus::Verified
    ));
}

#[test]
fn test_pause_allows_admin_rotation_and_new_admin_unpause() {
    let env = Env::default();
    let (client, admin, _business, _investor, _currency) = setup(&env);
    let new_admin = Address::generate(&env);

    client.pause(&admin);
    client.transfer_admin(&new_admin);
    assert_eq!(client.get_current_admin(), Some(new_admin.clone()));

    let old_admin_err = client
        .try_unpause(&admin)
        .err()
        .expect("old admin should lose authority")
        .expect("contract error");
    assert_eq!(old_admin_err, QuickLendXError::NotAdmin);

    client.unpause(&new_admin);
    assert!(!client.is_paused());
}

#[test]
fn test_pause_allows_emergency_withdraw_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    client.initialize_fee_system(&admin);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    token_admin_client.mint(&contract_id, &amount);
    client.pause(&admin);

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);
    assert!(client.get_pending_emergency_withdraw().is_some());

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);
    client.execute_emergency_withdraw(&admin);

    assert_eq!(token_client.balance(&target), amount);
    assert!(client.get_pending_emergency_withdraw().is_none());
    assert!(client.is_paused());
}

#[test]
fn test_pause_blocks_accept_bid_and_fund() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    client.pause(&admin);

    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_pause_blocks_release_escrow_funds() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    
    // Debug: check if accept_bid fails
    let accept_res = client.try_accept_bid(&invoice_id, &bid_id);
    if let Err(err) = accept_res {
        panic!("Setup failed at accept_bid: {:?}", err);
    }

    client.pause(&admin);

    let result = client.try_release_escrow_funds(&invoice_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_pause_blocks_refund_escrow_funds() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    client.accept_bid(&invoice_id, &bid_id);

    client.pause(&admin);

    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_pause_blocks_cancel_bid() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    client.pause(&admin);

    let result = client.try_cancel_bid(&bid_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_pause_blocks_update_invoice_category() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.pause(&admin);

    let result = client.try_update_invoice_category(&invoice_id, &InvoiceCategory::Products);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_pause_blocks_settle_invoice() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let _bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    // Normally accept_bid_and_fund happens here
    
    client.pause(&admin);
    let result = client.try_settle_invoice(&invoice_id, &1000i128);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_pause_blocks_add_investment_insurance() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let _bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    client.accept_bid_and_fund(&invoice_id, &_bid_id);
    client.release_escrow_funds(&invoice_id);
    
    let investment = client.get_invoice_investment(&invoice_id);
    let provider = Address::generate(&env);

    client.pause(&admin);
    let result = client.try_add_investment_insurance(&investment.investment_id, &provider, &80);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_pause_blocks_admin_set_platform_fee() {
    let env = Env::default();
    let (client, admin, _business, _investor, _currency) = setup(&env);

    client.pause(&admin);
    let result = client.try_set_platform_fee(&200i128);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_pause_blocks_kyc_submission() {
    let env = Env::default();
    let (client, admin, business, _investor, _currency) = setup(&env);

    client.pause(&admin);
    let result = client.try_submit_kyc_application(&business, &String::from_str(&env, "Data"));
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_pause_blocks_cancel_bid() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    client.pause(&admin);
    let result = client.try_cancel_bid(&bid_id);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_pause_blocks_protocol_limits_update() {
    let env = Env::default();
    let (client, admin, _business, _investor, _currency) = setup(&env);

    client.pause(&admin);
    let result = client.try_set_protocol_limits(&admin, &100i128, &90, &604800);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);
}

#[test]
fn test_pause_blocks_tag_management() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.pause(&admin);
    let result = client.try_add_invoice_tag(&invoice_id, &String::from_str(&env, "Urgent"));
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);
}
