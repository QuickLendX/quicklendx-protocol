//! Serialisation-stability tests for the `freeze_appeal_channel` field.
//!
//! Issue #1960: lock down the `InvoiceFrozen` event schema and the
//! `freeze_appeal_channel` value so that accidental renames, deletions,
//! or silent type changes are caught by the test suite.
//!
//! # Coverage
//! - Event data-map always contains exactly the expected five field names.
//! - `freeze_appeal_channel` is always `"docs/APPEALS.md"` for every
//!   [`BusinessFreezeReason`] variant (including `Administrative`).
//! - `FreezeInfo` round-trips through Soroban XDR serialisation.
//! - Freeze -> unfreeze -> refreeze preserves the channel value.
//! - `freeze_investor` does **not** emit an `InvoiceFrozen` event.
//! - Property test: channel is invariant across all reason variants.

use crate::events::TOPIC_INVOICE_FROZEN;
use crate::types::{BusinessFreezeReason, FreezeInfo, InvoiceCategory};
use proptest::prelude::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{Address, BytesN, Env, IntoVal, Map, String, Symbol, TryFromVal, Val, Vec};

// ============================================================================
// Helpers (mirrors test_freeze_event.rs)
// ============================================================================

fn setup(env: &Env) -> (crate::QuickLendXContractClient<'_>, Address) {
    let contract_id = env.register(crate::QuickLendXContract, ());
    env.ledger().set_timestamp(1_000);
    let client = crate::QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.set_admin(&admin);
    (client, admin)
}

fn kyc_business(env: &Env, client: &crate::QuickLendXContractClient, admin: &Address) -> Address {
    let biz = Address::generate(env);
    client.submit_kyc_application(&biz, &String::from_str(env, "KYC"));
    client.verify_business(admin, &biz);
    biz
}

fn create_invoice(
    env: &Env,
    client: &crate::QuickLendXContractClient,
    biz: &Address,
) -> BytesN<32> {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let due = env.ledger().timestamp() + 86_400;
    client.upload_invoice(
        biz,
        &1_000_000i128,
        &currency,
        &due,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
        &None,
    )
}

fn latest_event_data(env: &Env, topic_str: &str) -> Map<Symbol, Val> {
    use soroban_sdk::xdr;
    let topic_sym = Symbol::new(env, topic_str);
    let topic_xdr = soroban_sdk::xdr::ScVal::try_from_val(env, &topic_sym).expect("topic to ScVal");
    let all = env.events().all();
    for e in all.events().iter().rev() {
        let body = &e.body;
        let xdr::ContractEventBody::V0(b) = body;
        if b.topics.first() == Some(&topic_xdr) {
            let data_val = Val::try_from_val(env, &b.data).expect("data ScVal to Val");
            return Map::<Symbol, Val>::try_from_val(env, &data_val)
                .expect("event data is not a Map<Symbol,Val>");
        }
    }
    panic!(
        "topic {:?} not found in {} events",
        topic_str,
        all.events().len()
    );
}

fn get_str_field(env: &Env, map: &Map<Symbol, Val>, field: &str) -> String {
    let key = Symbol::new(env, field);
    let val = map
        .get(key)
        .unwrap_or_else(|| panic!("field '{}' not found in event data", field));
    String::try_from_val(env, &val)
        .unwrap_or_else(|_| panic!("failed to decode String field '{}'", field))
}

fn get_u64_field(env: &Env, map: &Map<Symbol, Val>, field: &str) -> u64 {
    let key = Symbol::new(env, field);
    let val = map
        .get(key)
        .unwrap_or_else(|| panic!("field '{}' not found in event data", field));
    u64::try_from_val(env, &val)
        .unwrap_or_else(|_| panic!("failed to decode u64 field '{}'", field))
}

fn get_addr_field(env: &Env, map: &Map<Symbol, Val>, field: &str) -> Address {
    let key = Symbol::new(env, field);
    let val = map
        .get(key)
        .unwrap_or_else(|| panic!("field '{}' not found in event data", field));
    Address::try_from_val(env, &val)
        .unwrap_or_else(|_| panic!("failed to decode Address field '{}'", field))
}

fn get_bytesn32_field(env: &Env, map: &Map<Symbol, Val>, field: &str) -> BytesN<32> {
    let key = Symbol::new(env, field);
    let val = map
        .get(key)
        .unwrap_or_else(|| panic!("field '{}' not found in event data", field));
    BytesN::<32>::try_from_val(env, &val)
        .unwrap_or_else(|_| panic!("failed to decode BytesN<32> field '{}'", field))
}

/// Expected field names in the `InvoiceFrozen` event data map.
/// Any change here (rename, addition, removal) must be accompanied by a
/// migration plan for off-chain indexers.
const EXPECTED_FIELDS: &[&str] = &[
    "freeze_appeal_channel",
    "frozen_by",
    "invoice_id",
    "reason",
    "timestamp",
];

// ============================================================================
// Schema lock tests
// ============================================================================

/// The `InvoiceFrozen` event data map must always contain exactly these five
/// field names. If a field is added, renamed, or removed, this test will
/// fail — forcing a deliberate migration decision.
#[test]
fn event_data_map_contains_exactly_expected_fields() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let biz = kyc_business(&env, &client, &admin);
    let invoice_id = create_invoice(&env, &client, &biz);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    let data = latest_event_data(&env, TOPIC_INVOICE_FROZEN);

    let mut count = 0u32;
    for _ in data.iter() {
        count += 1;
    }

    assert_eq!(
        count,
        EXPECTED_FIELDS.len() as u32,
        "event data map must have exactly {} fields, found {}",
        EXPECTED_FIELDS.len(),
        count
    );

    // Verify each expected field is present.
    for field in EXPECTED_FIELDS {
        let key = Symbol::new(&env, field);
        assert!(
            data.get(key).is_some(),
            "required field '{}' is missing from event data",
            field
        );
    }
}

/// Each expected field must be present and non-empty (where applicable).
/// This guards against silent nullification of the channel.
#[test]
fn event_fields_are_all_non_null() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let biz = kyc_business(&env, &client, &admin);
    let invoice_id = create_invoice(&env, &client, &biz);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::LegalHold);

    let data = latest_event_data(&env, TOPIC_INVOICE_FROZEN);

    // Verify every expected field is present.
    for field in EXPECTED_FIELDS {
        let key = Symbol::new(&env, field);
        assert!(
            data.get(key).is_some(),
            "required field '{}' is missing from event data",
            field
        );
    }

    // The channel must be non-empty.
    let channel = get_str_field(&env, &data, "freeze_appeal_channel");
    assert!(
        !channel.is_empty(),
        "freeze_appeal_channel must not be empty"
    );
    assert_eq!(
        channel,
        String::from_str(&env, "docs/APPEALS.md"),
        "freeze_appeal_channel must be exactly \"docs/APPEALS.md\""
    );
}

/// The `frozen_by` address in the event must match the admin that called
/// `freeze_invoice`, and the `invoice_id` must match the frozen invoice.
#[test]
fn event_identity_fields_match_inputs() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let biz = kyc_business(&env, &client, &admin);
    let invoice_id = create_invoice(&env, &client, &biz);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::FraudSuspected);

    let data = latest_event_data(&env, TOPIC_INVOICE_FROZEN);

    let event_invoice_id = get_bytesn32_field(&env, &data, "invoice_id");
    assert_eq!(event_invoice_id, invoice_id, "invoice_id must match");

    let event_frozen_by = get_addr_field(&env, &data, "frozen_by");
    assert_eq!(event_frozen_by, admin, "frozen_by must match the admin");

    let event_ts = get_u64_field(&env, &data, "timestamp");
    assert_eq!(event_ts, 1_000, "timestamp must match ledger time");
}

// ============================================================================
// Channel value tests — every reason variant
// ============================================================================

/// The `Administrative` variant (alias for `AdminAction`) must also carry
/// the correct appeal channel.
#[test]
fn channel_present_for_administrative_variant() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let biz = kyc_business(&env, &client, &admin);
    let invoice_id = create_invoice(&env, &client, &biz);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::Administrative);

    let data = latest_event_data(&env, TOPIC_INVOICE_FROZEN);
    let channel = get_str_field(&env, &data, "freeze_appeal_channel");
    assert_eq!(
        channel,
        String::from_str(&env, "docs/APPEALS.md"),
        "freeze_appeal_channel must be \"docs/APPEALS.md\" for Administrative"
    );

    let reason = get_str_field(&env, &data, "reason");
    assert_eq!(
        reason,
        String::from_str(&env, "admin_action"),
        "reason label for Administrative must be \"admin_action\""
    );
}

/// The `Administrative` variant must produce the same reason label as `AdminAction`.
#[test]
fn administrative_and_admin_action_share_label() {
    assert_eq!(
        BusinessFreezeReason::Administrative.label(),
        BusinessFreezeReason::AdminAction.label(),
        "Administrative and AdminAction must share the same label"
    );
    assert_eq!(
        BusinessFreezeReason::Administrative.label(),
        "admin_action",
        "Administrative label must be \"admin_action\""
    );
}

/// Freeze -> unfreeze -> refreeze: the appeal channel must be
/// `"docs/APPEALS.md"` on every freeze, regardless of how many cycles.
#[test]
fn channel_consistent_across_freeze_unfreeze_refreeze() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let biz = kyc_business(&env, &client, &admin);
    let invoice_id = create_invoice(&env, &client, &biz);

    let reasons = [
        BusinessFreezeReason::ComplianceViolation,
        BusinessFreezeReason::SuspiciousActivity,
        BusinessFreezeReason::Dispute,
    ];

    for reason in &reasons {
        // Freeze
        client.freeze_invoice(&admin, &invoice_id, reason);
        let data = latest_event_data(&env, TOPIC_INVOICE_FROZEN);
        let channel = get_str_field(&env, &data, "freeze_appeal_channel");
        assert_eq!(
            channel,
            String::from_str(&env, "docs/APPEALS.md"),
            "channel must be correct on freeze with reason {:?}",
            reason
        );

        // Unfreeze
        client.unfreeze_invoice(&admin, &invoice_id);

        // Refreeze with the same reason
        client.freeze_invoice(&admin, &invoice_id, reason);
        let data2 = latest_event_data(&env, TOPIC_INVOICE_FROZEN);
        let channel2 = get_str_field(&env, &data2, "freeze_appeal_channel");
        assert_eq!(
            channel2,
            String::from_str(&env, "docs/APPEALS.md"),
            "channel must be correct on refreeze with reason {:?}",
            reason
        );

        // Clean up for next iteration
        client.unfreeze_invoice(&admin, &invoice_id);
    }
}

/// When the same invoice is frozen multiple times without unfreezing,
/// each `InvoiceFrozen` event must still carry the correct channel.
#[test]
fn channel_correct_on_double_freeze() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let biz = kyc_business(&env, &client, &admin);
    let invoice_id = create_invoice(&env, &client, &biz);

    // First freeze
    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);
    let data1 = latest_event_data(&env, TOPIC_INVOICE_FROZEN);
    let channel1 = get_str_field(&env, &data1, "freeze_appeal_channel");
    assert_eq!(channel1, String::from_str(&env, "docs/APPEALS.md"));

    // Second freeze (without unfreeze) — should update the freeze info
    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::LegalHold);
    let data2 = latest_event_data(&env, TOPIC_INVOICE_FROZEN);
    let channel2 = get_str_field(&env, &data2, "freeze_appeal_channel");
    assert_eq!(channel2, String::from_str(&env, "docs/APPEALS.md"));
}

// ============================================================================
// FreezeInfo struct serialisation round-trip
// ============================================================================

/// `FreezeInfo` must survive a Soroban XDR round-trip with the
/// `reason` enum discriminant preserved.
#[test]
fn freeze_info_xdr_roundtrip_preserves_reason() {
    let env = Env::default();
    let addr = Address::generate(&env);

    let info = FreezeInfo {
        reason: BusinessFreezeReason::FraudSuspected,
        frozen_by: addr.clone(),
        frozen_at: 42,
    };

    let val: Val = info.clone().into_val(&env);
    let restored = FreezeInfo::try_from_val(&env, &val).expect("FreezeInfo round-trip failed");

    assert_eq!(restored.reason, BusinessFreezeReason::FraudSuspected);
    assert_eq!(restored.frozen_by, addr);
    assert_eq!(restored.frozen_at, 42);
}

/// Every `BusinessFreezeReason` variant round-trips through `FreezeInfo`.
#[test]
fn freeze_info_roundtrip_all_reasons() {
    let env = Env::default();
    let addr = Address::generate(&env);

    let reasons = [
        BusinessFreezeReason::AdminAction,
        BusinessFreezeReason::Administrative,
        BusinessFreezeReason::KYCRejected,
        BusinessFreezeReason::ComplianceViolation,
        BusinessFreezeReason::SuspiciousActivity,
        BusinessFreezeReason::LegalHold,
        BusinessFreezeReason::FraudSuspected,
        BusinessFreezeReason::Dispute,
        BusinessFreezeReason::Voluntary,
    ];

    for reason in &reasons {
        let info = FreezeInfo {
            reason: *reason,
            frozen_by: addr.clone(),
            frozen_at: 999,
        };

        let val: Val = info.clone().into_val(&env);
        let restored = FreezeInfo::try_from_val(&env, &val).expect("FreezeInfo round-trip failed");

        assert_eq!(
            restored.reason, *reason,
            "reason must survive round-trip for {:?}",
            reason
        );
        assert_eq!(restored.frozen_by, addr);
        assert_eq!(restored.frozen_at, 999);
    }
}

/// Two `FreezeInfo` instances with different reasons must serialise to
/// different `Val` representations — confirming the discriminant is
/// actually stored.
#[test]
fn freeze_info_different_reasons_serialize_differently() {
    let env = Env::default();
    let addr = Address::generate(&env);

    let info_a = FreezeInfo {
        reason: BusinessFreezeReason::ComplianceViolation,
        frozen_by: addr.clone(),
        frozen_at: 100,
    };
    let info_b = FreezeInfo {
        reason: BusinessFreezeReason::LegalHold,
        frozen_by: addr.clone(),
        frozen_at: 100,
    };

    let val_a: Val = info_a.into_val(&env);
    let val_b: Val = info_b.into_val(&env);

    let scval_a = soroban_sdk::xdr::ScVal::try_from_val(&env, &val_a).expect("val_a to ScVal");
    let scval_b = soroban_sdk::xdr::ScVal::try_from_val(&env, &val_b).expect("val_b to ScVal");

    assert_ne!(
        scval_a, scval_b,
        "FreezeInfo with different reasons must produce different serialisations"
    );
}

// ============================================================================
// Investor freeze — must NOT emit InvoiceFrozen
// ============================================================================

/// `freeze_investor` must NOT emit an `InvoiceFrozen` event (it is a
/// business-only event). This guards against accidental event leakage.
#[test]
fn investor_freeze_does_not_emit_invoice_frozen() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let biz = kyc_business(&env, &client, &admin);
    let _invoice_id = create_invoice(&env, &client, &biz);

    // Freeze an investor.
    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "Inv KYC"));
    client.verify_investor(&investor, &100_000i128);
    client.freeze_investor(
        &admin,
        &investor,
        &crate::types::InvestorFreezeReason::AdminAction,
    );

    // Count InvoiceFrozen events — should be zero.
    let all = env.events().all();
    let topic_sym = Symbol::new(&env, TOPIC_INVOICE_FROZEN);
    let topic_xdr =
        soroban_sdk::xdr::ScVal::try_from_val(&env, &topic_sym).expect("topic to ScVal");
    let mut count = 0u32;
    for e in all.events().iter() {
        let soroban_sdk::xdr::ContractEventBody::V0(b) = &e.body;
        if b.topics.first() == Some(&topic_xdr) {
            count += 1;
        }
    }
    assert_eq!(
        count, 0,
        "freeze_investor must NOT emit InvoiceFrozen events"
    );
}

// ============================================================================
// Administrative variant label + channel via contract call
// ============================================================================

/// `freeze_invoice` with `Administrative` emits correct label and channel.
#[test]
fn administrative_variant_emits_correct_label_and_channel() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let biz = kyc_business(&env, &client, &admin);
    let invoice_id = create_invoice(&env, &client, &biz);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::Administrative);

    let data = latest_event_data(&env, TOPIC_INVOICE_FROZEN);

    let reason = get_str_field(&env, &data, "reason");
    assert_eq!(
        reason,
        String::from_str(&env, "admin_action"),
        "Administrative reason label must be \"admin_action\""
    );

    let channel = get_str_field(&env, &data, "freeze_appeal_channel");
    assert_eq!(
        channel,
        String::from_str(&env, "docs/APPEALS.md"),
        "Administrative variant must carry the correct appeal channel"
    );
}

// ============================================================================
// Proptest: channel is always "docs/APPEALS.md"
// ============================================================================

// Property: for every `BusinessFreezeReason` variant, the emitted
// `InvoiceFrozen` event always has `freeze_appeal_channel == "docs/APPEALS.md"`.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_channel_always_appeals_doc(reason_index in 0u32..9u32) {
        let reasons = [
            BusinessFreezeReason::AdminAction,
            BusinessFreezeReason::Administrative,
            BusinessFreezeReason::KYCRejected,
            BusinessFreezeReason::ComplianceViolation,
            BusinessFreezeReason::SuspiciousActivity,
            BusinessFreezeReason::LegalHold,
            BusinessFreezeReason::FraudSuspected,
            BusinessFreezeReason::Dispute,
            BusinessFreezeReason::Voluntary,
        ];
        let reason = &reasons[reason_index as usize];

        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup(&env);
        let biz = kyc_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &biz);

        client.freeze_invoice(&admin, &invoice_id, reason);

        let data = latest_event_data(&env, TOPIC_INVOICE_FROZEN);
        let channel = get_str_field(&env, &data, "freeze_appeal_channel");

        prop_assert_eq!(
            channel,
            String::from_str(&env, "docs/APPEALS.md"),
            "channel must be \"docs/APPEALS.md\" for reason {:?}",
            reason
        );
    }
}

// Property: `BusinessFreezeReason::label()` always returns a non-empty,
// lowercase, snake_case string for every variant.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_label_is_snake_case(reason_index in 0u32..9u32) {
        let reasons = [
            BusinessFreezeReason::AdminAction,
            BusinessFreezeReason::Administrative,
            BusinessFreezeReason::KYCRejected,
            BusinessFreezeReason::ComplianceViolation,
            BusinessFreezeReason::SuspiciousActivity,
            BusinessFreezeReason::LegalHold,
            BusinessFreezeReason::FraudSuspected,
            BusinessFreezeReason::Dispute,
            BusinessFreezeReason::Voluntary,
        ];
        let reason = &reasons[reason_index as usize];
        let label = reason.label();

        prop_assert!(!label.is_empty(), "label must not be empty for {:?}", reason);
        prop_assert!(
            label.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "label must be lowercase snake_case, got {:?} for {:?}",
            label,
            reason
        );
    }
}

// Property: `FreezeInfo` round-trips through XDR for every variant.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_freeze_info_roundtrip(reason_index in 0u32..9u32) {
        let reasons = [
            BusinessFreezeReason::AdminAction,
            BusinessFreezeReason::Administrative,
            BusinessFreezeReason::KYCRejected,
            BusinessFreezeReason::ComplianceViolation,
            BusinessFreezeReason::SuspiciousActivity,
            BusinessFreezeReason::LegalHold,
            BusinessFreezeReason::FraudSuspected,
            BusinessFreezeReason::Dispute,
            BusinessFreezeReason::Voluntary,
        ];
        let reason = &reasons[reason_index as usize];

        let env = Env::default();
        let addr = Address::generate(&env);

        let info = FreezeInfo {
            reason: *reason,
            frozen_by: addr.clone(),
            frozen_at: 777,
        };

        let val: Val = info.into_val(&env);
        let restored = FreezeInfo::try_from_val(&env, &val)
            .expect("FreezeInfo round-trip must succeed");

        prop_assert_eq!(restored.reason, *reason);
        prop_assert_eq!(restored.frozen_by, addr);
        prop_assert_eq!(restored.frozen_at, 777);
    }
}
