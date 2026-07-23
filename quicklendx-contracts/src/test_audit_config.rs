//! Regression tests: each admin config change emits exactly one tamper-evident
//! audit entry on the `CONFIG_AUDIT_SENTINEL` virtual trail, and chain integrity
//! holds across sequential changes from all five admin config functions.

#[cfg(test)]
use super::*;

use crate::admin::AdminStorage;
use crate::audit::{AuditOperation, CONFIG_AUDIT_SENTINEL};
use crate::fees::FeeType;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

// ─── Setup helpers ────────────────────────────────────────────────────────────

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || {
        AdminStorage::initialize(&env, &admin).unwrap();
    });
    (env, client, admin)
}

fn setup_with_fees() -> (Env, QuickLendXContractClient<'static>, Address) {
    let (env, client, admin) = setup();
    client.initialize_fee_system(&admin);
    (env, client, admin)
}

fn sentinel(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &CONFIG_AUDIT_SENTINEL)
}

fn trail_len(client: &QuickLendXContractClient, env: &Env) -> u32 {
    client.get_invoice_audit_trail(&sentinel(env)).len()
}

// ─── set_protocol_config ─────────────────────────────────────────────────────

#[test]
fn test_proto_cfg_emits_one_entry() {
    let (env, client, admin) = setup();
    let before = trail_len(&client, &env);
    client.set_protocol_config(&admin, &1_000_000i128, &365u64, &604_800u64);
    assert_eq!(trail_len(&client, &env), before + 1);

    let ids = client.get_invoice_audit_trail(&sentinel(&env));
    let last_id = ids.get(ids.len() - 1).unwrap();
    let entry = client.get_audit_entry(&last_id).unwrap();
    assert_eq!(entry.operation, AuditOperation::ConfigProtocolChanged);
    assert_eq!(entry.actor, admin);
    assert!(entry.new_value.is_some());
    assert_eq!(
        entry.additional_data,
        Some(String::from_str(&env, "proto_cfg"))
    );
}

#[test]
fn test_proto_cfg_first_change_has_no_old_value() {
    let (env, client, admin) = setup();
    client.set_protocol_config(&admin, &1_000_000i128, &365u64, &604_800u64);

    let ids = client.get_invoice_audit_trail(&sentinel(&env));
    let id = ids.get(ids.len() - 1).unwrap();
    let entry = client.get_audit_entry(&id).unwrap();
    assert_eq!(entry.old_value, None, "first change has no old value");
}

#[test]
fn test_proto_cfg_second_change_records_old_value() {
    let (env, client, admin) = setup();
    client.set_protocol_config(&admin, &500_000i128, &180u64, &86_400u64);
    client.set_protocol_config(&admin, &1_000_000i128, &365u64, &604_800u64);

    let ids = client.get_invoice_audit_trail(&sentinel(&env));
    let id = ids.get(ids.len() - 1).unwrap();
    let entry = client.get_audit_entry(&id).unwrap();
    assert!(
        entry.old_value.is_some(),
        "second change must record old value"
    );
    assert!(entry.new_value.is_some());
}

#[test]
fn test_proto_cfg_non_admin_produces_no_entry() {
    let (env, client, admin) = setup();
    client.set_protocol_config(&admin, &1_000_000i128, &365u64, &604_800u64);
    let before = trail_len(&client, &env);
    let non_admin = Address::generate(&env);
    assert!(client
        .try_set_protocol_config(&non_admin, &500_000i128, &180u64, &86_400u64)
        .is_err());
    assert_eq!(
        trail_len(&client, &env),
        before,
        "rejected call must not emit entry"
    );
}

#[test]
fn test_proto_cfg_invalid_params_produces_no_entry() {
    let (env, client, admin) = setup();
    client.set_protocol_config(&admin, &1_000_000i128, &365u64, &604_800u64);
    let before = trail_len(&client, &env);
    // min_invoice_amount = 0 is invalid
    assert!(client
        .try_set_protocol_config(&admin, &0i128, &365u64, &604_800u64)
        .is_err());
    assert_eq!(trail_len(&client, &env), before);
}

#[test]
fn test_proto_cfg_chain_integrity_after_three_changes() {
    let (env, client, admin) = setup();
    client.set_protocol_config(&admin, &500_000i128, &180u64, &86_400u64);
    client.set_protocol_config(&admin, &1_000_000i128, &365u64, &604_800u64);
    client.set_protocol_config(&admin, &2_000_000i128, &730u64, &604_800u64);
    assert!(client.verify_audit_chain(&sentinel(&env)));
    assert_eq!(client.first_audit_chain_divergence(&sentinel(&env)), None);
}

// ─── set_fee_config ──────────────────────────────────────────────────────────

#[test]
fn test_fee_config_emits_one_entry() {
    let (env, client, admin) = setup();
    // First call to establish state
    client.set_fee_config(&admin, &200u32);
    let before = trail_len(&client, &env);
    client.set_fee_config(&admin, &300u32);
    assert_eq!(trail_len(&client, &env), before + 1);

    let ids = client.get_invoice_audit_trail(&sentinel(&env));
    let id = ids.get(ids.len() - 1).unwrap();
    let entry = client.get_audit_entry(&id).unwrap();
    assert_eq!(entry.operation, AuditOperation::ConfigFeeChanged);
    assert_eq!(entry.actor, admin);
    assert!(entry.old_value.is_some());
    assert!(entry.new_value.is_some());
    assert_eq!(
        entry.additional_data,
        Some(String::from_str(&env, "fee_bps"))
    );
}

#[test]
fn test_fee_config_non_admin_produces_no_entry() {
    let (env, client, admin) = setup();
    client.set_fee_config(&admin, &200u32);
    let before = trail_len(&client, &env);
    let non_admin = Address::generate(&env);
    assert!(client.try_set_fee_config(&non_admin, &300u32).is_err());
    assert_eq!(trail_len(&client, &env), before);
}

#[test]
fn test_fee_config_invalid_bps_produces_no_entry() {
    let (env, client, admin) = setup();
    client.set_fee_config(&admin, &200u32);
    let before = trail_len(&client, &env);
    // > MAX_FEE_BPS (1000)
    assert!(client.try_set_fee_config(&admin, &1001u32).is_err());
    assert_eq!(trail_len(&client, &env), before);
}

// ─── set_treasury ────────────────────────────────────────────────────────────

#[test]
fn test_set_treasury_emits_one_entry() {
    let (env, client, admin) = setup();
    let treasury = Address::generate(&env);
    let before = trail_len(&client, &env);
    client.set_treasury(&admin, &treasury);
    assert_eq!(trail_len(&client, &env), before + 1);

    let ids = client.get_invoice_audit_trail(&sentinel(&env));
    let id = ids.get(ids.len() - 1).unwrap();
    let entry = client.get_audit_entry(&id).unwrap();
    assert_eq!(entry.operation, AuditOperation::ConfigTreasuryChanged);
    assert_eq!(entry.actor, admin);
    assert_eq!(
        entry.old_value, None,
        "first treasury set has no previous address"
    );
    assert!(entry.new_value.is_some());
    assert_eq!(
        entry.additional_data,
        Some(String::from_str(&env, "treasury"))
    );
}

#[test]
fn test_set_treasury_second_change_records_old_value() {
    let (env, client, admin) = setup();
    let treasury1 = Address::generate(&env);
    let treasury2 = Address::generate(&env);
    client.set_treasury(&admin, &treasury1);
    client.set_treasury(&admin, &treasury2);

    let ids = client.get_invoice_audit_trail(&sentinel(&env));
    let id = ids.get(ids.len() - 1).unwrap();
    let entry = client.get_audit_entry(&id).unwrap();
    assert!(
        entry.old_value.is_some(),
        "second treasury change must record old address"
    );
    assert!(entry.new_value.is_some());
}

#[test]
fn test_set_treasury_non_admin_produces_no_entry() {
    let (env, client, admin) = setup();
    let treasury = Address::generate(&env);
    client.set_treasury(&admin, &treasury);
    let before = trail_len(&client, &env);
    let non_admin = Address::generate(&env);
    let other = Address::generate(&env);
    assert!(client.try_set_treasury(&non_admin, &other).is_err());
    assert_eq!(trail_len(&client, &env), before);
}

#[test]
fn test_set_treasury_admin_eq_treasury_produces_no_entry() {
    let (env, client, admin) = setup();
    let before = trail_len(&client, &env);
    // treasury == admin is rejected with InvalidAddress
    assert!(client.try_set_treasury(&admin, &admin).is_err());
    assert_eq!(trail_len(&client, &env), before);
}

// ─── update_fee_structure ────────────────────────────────────────────────────

#[test]
fn test_update_fee_structure_new_entry_emits_audit() {
    let (env, client, admin) = setup_with_fees();
    let before = trail_len(&client, &env);
    // EarlyPayment is not in the default fee structures, so this is a new insertion
    client.update_fee_structure(
        &admin,
        &FeeType::EarlyPayment,
        &200u32,
        &100i128,
        &10_000i128,
        &true,
    );
    assert_eq!(trail_len(&client, &env), before + 1);

    let ids = client.get_invoice_audit_trail(&sentinel(&env));
    let id = ids.get(ids.len() - 1).unwrap();
    let entry = client.get_audit_entry(&id).unwrap();
    assert_eq!(entry.operation, AuditOperation::ConfigFeeStructureChanged);
    assert_eq!(entry.actor, admin);
    assert_eq!(
        entry.old_value, None,
        "new fee-type insertion has no old value"
    );
    assert!(entry.new_value.is_some());
    assert_eq!(
        entry.additional_data,
        Some(String::from_str(&env, "EarlyPayment"))
    );
}

#[test]
fn test_update_fee_structure_update_records_old_value() {
    let (env, client, admin) = setup_with_fees();
    client.update_fee_structure(
        &admin,
        &FeeType::Platform,
        &200u32,
        &100i128,
        &10_000i128,
        &true,
    );
    client.update_fee_structure(
        &admin,
        &FeeType::Platform,
        &300u32,
        &200i128,
        &20_000i128,
        &true,
    );

    let ids = client.get_invoice_audit_trail(&sentinel(&env));
    let id = ids.get(ids.len() - 1).unwrap();
    let entry = client.get_audit_entry(&id).unwrap();
    assert!(
        entry.old_value.is_some(),
        "update of existing structure must record old value"
    );
    assert!(entry.new_value.is_some());
}

#[test]
fn test_update_fee_structure_invalid_bps_produces_no_entry() {
    let (env, client, admin) = setup_with_fees();
    let before = trail_len(&client, &env);
    // base_fee_bps > MAX_FEE_BPS (1000)
    assert!(client
        .try_update_fee_structure(
            &admin,
            &FeeType::Platform,
            &1001u32,
            &100i128,
            &10_000i128,
            &true
        )
        .is_err());
    assert_eq!(trail_len(&client, &env), before);
}

#[test]
fn test_update_fee_structure_fee_type_label_in_additional_data() {
    let (env, client, admin) = setup_with_fees();
    client.update_fee_structure(
        &admin,
        &FeeType::LatePayment,
        &500u32,
        &100i128,
        &5_000i128,
        &true,
    );

    let ids = client.get_invoice_audit_trail(&sentinel(&env));
    let id = ids.get(ids.len() - 1).unwrap();
    let entry = client.get_audit_entry(&id).unwrap();
    assert_eq!(
        entry.additional_data,
        Some(String::from_str(&env, "LatePayment"))
    );
}

#[test]
fn test_update_fee_structure_chain_integrity() {
    let (env, client, admin) = setup_with_fees();
    client.update_fee_structure(
        &admin,
        &FeeType::Platform,
        &200u32,
        &100i128,
        &10_000i128,
        &true,
    );
    client.update_fee_structure(
        &admin,
        &FeeType::Processing,
        &150u32,
        &50i128,
        &5_000i128,
        &true,
    );
    assert!(client.verify_audit_chain(&sentinel(&env)));
}

// ─── configure_revenue_distribution ─────────────────────────────────────────

#[test]
fn test_configure_revenue_distribution_emits_one_entry() {
    let (env, client, admin) = setup();
    let treasury = Address::generate(&env);
    let before = trail_len(&client, &env);
    client.configure_revenue_distribution(
        &admin, &treasury, &5000u32, &3000u32, &2000u32, &false, &0i128,
    );
    assert_eq!(trail_len(&client, &env), before + 1);

    let ids = client.get_invoice_audit_trail(&sentinel(&env));
    let id = ids.get(ids.len() - 1).unwrap();
    let entry = client.get_audit_entry(&id).unwrap();
    assert_eq!(
        entry.operation,
        AuditOperation::ConfigRevenueDistributionChanged
    );
    assert_eq!(entry.actor, admin);
    assert_eq!(
        entry.old_value, None,
        "first distribution config has no old value"
    );
    assert!(entry.new_value.is_some());
    assert_eq!(
        entry.additional_data,
        Some(String::from_str(&env, "rev_dist"))
    );
}

#[test]
fn test_configure_revenue_distribution_second_call_records_old_value() {
    let (env, client, admin) = setup();
    let treasury = Address::generate(&env);
    client.configure_revenue_distribution(
        &admin, &treasury, &5000u32, &3000u32, &2000u32, &false, &0i128,
    );
    client.configure_revenue_distribution(
        &admin, &treasury, &6000u32, &2500u32, &1500u32, &true, &1000i128,
    );

    let ids = client.get_invoice_audit_trail(&sentinel(&env));
    let id = ids.get(ids.len() - 1).unwrap();
    let entry = client.get_audit_entry(&id).unwrap();
    assert!(
        entry.old_value.is_some(),
        "second config must record old value"
    );
    assert!(entry.new_value.is_some());
}

#[test]
fn test_configure_revenue_distribution_non_admin_produces_no_entry() {
    let (env, client, admin) = setup();
    let treasury = Address::generate(&env);
    client.configure_revenue_distribution(
        &admin, &treasury, &5000u32, &3000u32, &2000u32, &false, &0i128,
    );
    let before = trail_len(&client, &env);
    let non_admin = Address::generate(&env);
    assert!(client
        .try_configure_revenue_distribution(
            &non_admin, &treasury, &5000u32, &3000u32, &2000u32, &false, &0i128,
        )
        .is_err());
    assert_eq!(trail_len(&client, &env), before);
}

#[test]
fn test_configure_revenue_distribution_invalid_sum_produces_no_entry() {
    let (env, client, admin) = setup();
    let treasury = Address::generate(&env);
    let before = trail_len(&client, &env);
    // shares don't sum to 10_000
    assert!(client
        .try_configure_revenue_distribution(
            &admin, &treasury, &3000u32, &3000u32, &3000u32, &false, &0i128,
        )
        .is_err());
    assert_eq!(trail_len(&client, &env), before);
}

// ─── Cross-function chain integrity ──────────────────────────────────────────

#[test]
fn test_all_five_functions_form_one_valid_chain() {
    let (env, client, admin) = setup_with_fees();
    let treasury = Address::generate(&env);

    client.set_protocol_config(&admin, &1_000_000i128, &365u64, &604_800u64);
    client.set_fee_config(&admin, &200u32);
    client.set_treasury(&admin, &treasury);
    client.update_fee_structure(
        &admin,
        &FeeType::Platform,
        &200u32,
        &100i128,
        &10_000i128,
        &true,
    );
    client.configure_revenue_distribution(
        &admin, &treasury, &5000u32, &3000u32, &2000u32, &false, &0i128,
    );

    assert_eq!(trail_len(&client, &env), 5);
    assert!(client.verify_audit_chain(&sentinel(&env)));
    assert_eq!(client.first_audit_chain_divergence(&sentinel(&env)), None);
}

#[test]
fn test_get_audit_entries_by_operation_returns_correct_counts() {
    let (env, client, admin) = setup();

    client.set_protocol_config(&admin, &1_000_000i128, &365u64, &604_800u64);
    client.set_protocol_config(&admin, &2_000_000i128, &730u64, &604_800u64);
    client.set_fee_config(&admin, &200u32);

    let proto_ids = client.get_audit_entries_by_operation(&AuditOperation::ConfigProtocolChanged);
    assert_eq!(proto_ids.len(), 2);

    let fee_ids = client.get_audit_entries_by_operation(&AuditOperation::ConfigFeeChanged);
    assert_eq!(fee_ids.len(), 1);
}

#[test]
fn test_audit_stats_total_grows_with_each_change() {
    let (env, client, admin) = setup();
    let stats_before = client.get_audit_stats();

    client.set_fee_config(&admin, &200u32);
    client.set_fee_config(&admin, &300u32);

    let stats_after = client.get_audit_stats();
    assert_eq!(stats_after.total_entries, stats_before.total_entries + 2);
}
