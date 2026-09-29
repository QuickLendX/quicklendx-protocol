#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::init::InitializationParams;
use crate::invoice::InvoiceCategory;
use crate::protocol_limits;
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
            &None)
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
        &None);
    assert_eq!(low_amount, Err(Ok(QuickLendXError::InvalidAmount)));

    let above_new_horizon = client.try_store_invoice(
        &business,
        &200i128,
        &currency,
        &(now + 86_401),
        &String::from_str(&env, "beyond new horizon"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None);
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
            &None)
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
            &None)
        .is_ok());

    let blocked = client.try_upload_invoice(
        &business,
        &10i128,
        &currency,
        &due_date,
        &String::from_str(&env, "second blocked"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None);
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
            &None)
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
                crate::verification::InvestorTier::Basic,
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
                crate::verification::InvestorTier::Basic,
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
        corridors: Vec::new(&env),
        backfill_max_batch_size: 100,
    };

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidTimestamp)));
    assert!(!client.is_initialized());
    assert_eq!(client.get_current_admin(), None);
}

// ---------------------------------------------------------------------------
// Tests for set_protocol_limits_full (Issue: expose min_bid_amount / min_bid_bps)
// ---------------------------------------------------------------------------

#[test]
fn test_set_protocol_limits_full_round_trips_all_fields() {
    let (_, client, admin, _, _) = setup();
    client.set_admin(&admin);

    client.set_protocol_limits_full(
        &admin,
        &500i128,  // min_invoice_amount
        &50i128,   // min_bid_amount
        &200u32,   // min_bid_bps  (2 %)
        &180u64,   // max_due_date_days
        &3_600u64, // grace_period_seconds
        &10u32,    // max_invoices_per_business
    );

    let limits = client.get_protocol_limits();
    assert_eq!(limits.min_invoice_amount, 500);
    assert_eq!(limits.min_bid_amount, 50);
    assert_eq!(limits.min_bid_bps, 200);
    assert_eq!(limits.max_due_date_days, 180);
    assert_eq!(limits.grace_period_seconds, 3_600);
    assert_eq!(limits.max_invoices_per_business, 10);
}

#[test]
fn test_set_protocol_limits_full_non_admin_rejected() {
    let (_, client, admin, non_admin, _) = setup();
    client.set_admin(&admin);

    let result = client.try_set_protocol_limits_full(
        &non_admin,
        &10i128,
        &10i128,
        &100u32,
        &365u64,
        &0u64,
        &100u32,
    );
    assert_eq!(result, Err(Ok(QuickLendXError::NotAdmin)));
}

#[test]
fn test_set_protocol_limits_full_rejects_zero_min_bid_amount() {
    let (_, client, admin, _, _) = setup();
    client.set_admin(&admin);

    let result = client.try_set_protocol_limits_full(
        &admin,
        &10i128,
        &0i128, // invalid
        &100u32,
        &365u64,
        &0u64,
        &100u32,
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
}

#[test]
fn test_set_protocol_limits_full_rejects_min_bid_bps_above_10000() {
    let (_, client, admin, _, _) = setup();
    client.set_admin(&admin);

    let result = client.try_set_protocol_limits_full(
        &admin,
        &10i128,
        &10i128,
        &10_001u32, // invalid (> 100 %)
        &365u64,
        &0u64,
        &100u32,
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
}

#[test]
fn test_narrow_set_protocol_limits_preserves_bid_fields() {
    // set_protocol_limits / update_protocol_limits must NOT silently overwrite
    // min_bid_amount or min_bid_bps that were previously set via
    // set_protocol_limits_full.
    let (_, client, admin, _, _) = setup();
    client.set_admin(&admin);

    // First set custom bid limits via the full entrypoint.
    client.set_protocol_limits_full(
        &admin, &10i128, &75i128, &300u32, &365u64, &0u64, &100u32,
    );

    // Now call the narrow helper — it must preserve min_bid_amount=75 and min_bid_bps=300.
    client.set_protocol_limits(&admin, &20i128, &180u64, &0u64);

    let limits = client.get_protocol_limits();
    assert_eq!(limits.min_invoice_amount, 20, "min_invoice_amount updated");
    assert_eq!(limits.min_bid_amount, 75, "min_bid_amount preserved");
    assert_eq!(limits.min_bid_bps, 300, "min_bid_bps preserved");
}

#[test]
fn test_update_protocol_limits_preserves_bid_fields() {
    let (_, client, admin, _, _) = setup();
    client.set_admin(&admin);

    client.set_protocol_limits_full(
        &admin, &10i128, &50i128, &250u32, &365u64, &0u64, &100u32,
    );

    client.update_protocol_limits(&admin, &15i128, &90u64, &0u64);

    let limits = client.get_protocol_limits();
    assert_eq!(limits.min_bid_amount, 50, "min_bid_amount preserved by update_protocol_limits");
    assert_eq!(limits.min_bid_bps, 250, "min_bid_bps preserved by update_protocol_limits");
}

// ---------------------------------------------------------------------------
// Tests for get_bid_limit_config / reset_max_active_bids_per_investor
// ---------------------------------------------------------------------------

#[test]
fn test_get_bid_limit_config_returns_defaults_before_any_admin_set() {
    let (_, client, admin, _, _) = setup();
    client.set_admin(&admin);

    let cfg = client.get_bid_limit_config();
    // Default limit is 20 (DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR).
    assert_eq!(cfg.limit, 20);
    assert_eq!(cfg.default_limit, 20);
    assert!(!cfg.is_disabled, "limit of 20 is not disabled");
    assert!(!cfg.is_custom, "no admin override yet");
}

#[test]
fn test_set_and_get_bid_limit_config_round_trip() {
    let (_, client, admin, _, _) = setup();
    client.set_admin(&admin);

    client.set_max_active_bids_per_investor(&5u32);

    let cfg = client.get_bid_limit_config();
    assert_eq!(cfg.limit, 5);
    assert!(!cfg.is_disabled);
    assert!(cfg.is_custom);
}

#[test]
fn test_set_bid_limit_to_zero_marks_disabled() {
    let (_, client, admin, _, _) = setup();
    client.set_admin(&admin);

    client.set_max_active_bids_per_investor(&0u32);

    let cfg = client.get_bid_limit_config();
    assert_eq!(cfg.limit, 0);
    assert!(cfg.is_disabled, "limit of 0 means disabled");
    assert!(cfg.is_custom);
}

#[test]
fn test_reset_max_active_bids_per_investor_clears_custom_flag() {
    let (_, client, admin, _, _) = setup();
    client.set_admin(&admin);

    client.set_max_active_bids_per_investor(&3u32);
    assert!(client.get_bid_limit_config().is_custom);

    client.reset_max_active_bids_per_investor();

    let cfg = client.get_bid_limit_config();
    assert_eq!(cfg.limit, 20, "reset restores compile-time default");
    assert!(!cfg.is_custom, "is_custom cleared after reset");
    assert!(!cfg.is_disabled);
}
