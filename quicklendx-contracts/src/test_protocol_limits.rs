#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::init::InitializationParams;
use crate::invoice::InvoiceCategory;
use crate::protocol_limits::ProtocolLimitsContract;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

fn setup() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    (env, client, admin, non_admin, contract_id)
}

#[test]
fn test_admin_limit_update_applies_immediately_to_validation_and_default_date() {
    let (env, client, admin, _, contract_id) = setup();
    client.set_admin(&admin);

    client.set_protocol_limits(&admin, &100i128, &30u64, &60u64);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let now = env.ledger().timestamp();
    let initial_due_date = now + 86_400;

    assert!(client
        .try_store_invoice(
            &business,
            &100i128,
            &currency,
            &initial_due_date,
            &String::from_str(&env, "allowed by initial limits"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .is_ok());

    client.update_protocol_limits(&admin, &200i128, &1u64, &120u64);

    let low_amount = client.try_store_invoice(
        &business,
        &199i128,
        &currency,
        &initial_due_date,
        &String::from_str(&env, "below updated min"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(low_amount, Err(Ok(QuickLendXError::InvalidAmount)));

    let above_new_horizon = client.try_store_invoice(
        &business,
        &200i128,
        &currency,
        &(now + 86_401),
        &String::from_str(&env, "beyond new horizon"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(
        above_new_horizon,
        Err(Ok(QuickLendXError::InvoiceDueDateInvalid))
    );

    let updated_default = env.as_contract(&contract_id, || {
        ProtocolLimitsContract::get_default_date(env.clone(), now + 86_400)
    });
    assert_eq!(updated_default, now + 86_400 + 120);
}

#[test]
fn test_non_admin_limit_updates_are_rejected_across_all_entrypoints() {
    let (env, client, admin, non_admin, _) = setup();
    client.set_admin(&admin);

    let original_limits = client.get_protocol_limits();

    let set_result = client.try_set_protocol_limits(&non_admin, &10i128, &365u64, &0u64);
    assert_eq!(set_result, Err(Ok(QuickLendXError::NotAdmin)));

    let update_result = client.try_update_protocol_limits(&non_admin, &10i128, &365u64, &0u64);
    assert_eq!(update_result, Err(Ok(QuickLendXError::NotAdmin)));

    let update_with_cap =
        client.try_update_limits_max_invoices(&non_admin, &10i128, &365u64, &0u64, &2u32);
    assert_eq!(update_with_cap, Err(Ok(QuickLendXError::NotAdmin)));

    let limits_after = client.get_protocol_limits();
    assert_eq!(limits_after.min_invoice_amount, original_limits.min_invoice_amount);
    assert_eq!(limits_after.max_due_date_days, original_limits.max_due_date_days);
    assert_eq!(limits_after.grace_period_seconds, original_limits.grace_period_seconds);
    assert_eq!(
        limits_after.max_invoices_per_business,
        original_limits.max_invoices_per_business
    );

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;
    assert!(client
        .try_store_invoice(
            &business,
            &original_limits.min_invoice_amount,
            &currency,
            &due_date,
            &String::from_str(&env, "still governed by original limits"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .is_ok());
}

#[test]
fn test_set_protocol_limits_rejects_invalid_parameter_bounds() {
    let (_, client, admin, _, _) = setup();
    client.set_admin(&admin);

    assert_eq!(
        client.try_set_protocol_limits(&admin, &0i128, &365u64, &0u64),
        Err(Ok(QuickLendXError::InvalidAmount))
    );

    assert_eq!(
        client.try_set_protocol_limits(&admin, &10i128, &0u64, &0u64),
        Err(Ok(QuickLendXError::InvoiceDueDateInvalid))
    );

    assert_eq!(
        client.try_set_protocol_limits(&admin, &10i128, &731u64, &0u64),
        Err(Ok(QuickLendXError::InvoiceDueDateInvalid))
    );

    assert_eq!(
        client.try_set_protocol_limits(&admin, &10i128, &365u64, &2_592_001u64),
        Err(Ok(QuickLendXError::InvalidTimestamp))
    );
}

#[test]
fn test_set_protocol_limits_rejects_invalid_parameter_combination() {
    let (_, client, admin, _, _) = setup();
    client.set_admin(&admin);

    // 1 day horizon cannot have > 1 day grace period.
    let result = client.try_set_protocol_limits(&admin, &10i128, &1u64, &86_401u64);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidTimestamp)));
}

#[test]
fn test_update_limits_max_invoices_applies_immediately() {
    let (env, client, admin, _, _) = setup();
    client.set_admin(&admin);

    client.update_limits_max_invoices(&admin, &10i128, &365u64, &0u64, &1u32);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);
    client.add_currency(&admin, &currency);
    let due_date = env.ledger().timestamp() + 86_400;

    assert!(client
        .try_upload_invoice(
            &business,
            &10i128,
            &currency,
            &due_date,
            &String::from_str(&env, "first"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .is_ok());

    let blocked = client.try_upload_invoice(
        &business,
        &10i128,
        &currency,
        &due_date,
        &String::from_str(&env, "second blocked"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(
        blocked,
        Err(Ok(QuickLendXError::MaxInvoicesPerBusinessExceeded))
    );

    client.update_limits_max_invoices(&admin, &10i128, &365u64, &0u64, &2u32);

    assert!(client
        .try_upload_invoice(
            &business,
            &10i128,
            &currency,
            &due_date,
            &String::from_str(&env, "second allowed"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .is_ok());
}

#[test]
fn test_internal_protocol_limit_updates_reject_invalid_bid_constraints() {
    let (env, client, admin, _, contract_id) = setup();
    client.set_admin(&admin);

    assert_eq!(
        env.as_contract(&contract_id, || {
            ProtocolLimitsContract::set_protocol_limits(
                env.clone(),
                admin.clone(),
                10,
                0,
                100,
                365,
                0,
                100,
            )
        }),
        Err(QuickLendXError::InvalidAmount)
    );

    assert_eq!(
        env.as_contract(&contract_id, || {
            ProtocolLimitsContract::set_protocol_limits(
                env.clone(),
                admin.clone(),
                10,
                10,
                10_001,
                365,
                0,
                100,
            )
        }),
        Err(QuickLendXError::InvalidAmount)
    );
}

#[test]
fn test_initialize_rejects_invalid_limit_combination_before_state_commit() {
    let (env, client, admin, _, _) = setup();

    let params = InitializationParams {
        admin: admin.clone(),
        treasury: Address::generate(&env),
        fee_bps: 200,
        min_invoice_amount: 10,
        max_due_date_days: 1,
        grace_period_seconds: 86_401,
        initial_currencies: Vec::new(&env),
    };

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidTimestamp)));
    assert!(!client.is_initialized());
    assert_eq!(client.get_current_admin(), None);
}
