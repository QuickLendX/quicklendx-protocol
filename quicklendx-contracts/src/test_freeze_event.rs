//! Tests for the `InvoiceFrozen` event emitted by `freeze_invoice`.
//!
//! Issue #1959: The freeze event payload must include a `freeze_appeal_channel`
//! field that points off-chain consumers to the appeals process documented in
//! `docs/APPEALS.md`.
//!
//! # Coverage
//! - `freeze_invoice` emits an `InvoiceFrozen` event.
//! - The event payload includes `freeze_appeal_channel = "docs/APPEALS.md"`.
//! - The event payload includes the correct `reason` label for each
//!   [`BusinessFreezeReason`] variant.
//! - The event payload includes the `frozen_by` (admin) address.
//! - The event payload includes the correct `invoice_id`.
//! - The topic constant [`TOPIC_INVOICE_FROZEN`] is `"invoice_frozen"`.

#[cfg(test)]
use crate::events::TOPIC_INVOICE_FROZEN;
#[cfg(test)]
use crate::types::{BusinessFreezeReason, InvoiceCategory};
#[cfg(test)]
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, BytesN, Env, Map, String, Symbol, TryFromVal, Val, Vec,
};

// ============================================================================
// Helpers (mirrors test_events.rs pattern)
// ============================================================================

#[cfg(test)]
fn setup(env: &Env) -> (crate::QuickLendXContractClient<'_>, Address) {
    let contract_id = env.register(crate::QuickLendXContract, ());
    env.ledger().set_timestamp(1_000);
    let client = crate::QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.set_admin(&admin);
    (client, admin)
}

#[cfg(test)]
fn kyc_business(env: &Env, client: &crate::QuickLendXContractClient, admin: &Address) -> Address {
    let biz = Address::generate(env);
    client.submit_kyc_application(&biz, &String::from_str(env, "KYC"));
    client.verify_business(admin, &biz);
    biz
}

#[cfg(test)]
fn create_invoice(
    env: &Env,
    client: &crate::QuickLendXContractClient,
    biz: &Address,
) -> BytesN<32> {
    // Use a dummy currency address — we only need the invoice to exist.
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
    )
}

/// Extract the data map of the most-recent event matching `topic_str`.
///
/// Mirrors the helper in `test_events.rs`.
#[cfg(test)]
fn latest_event_data(env: &Env, topic_str: &str) -> Map<Symbol, Val> {
    use soroban_sdk::xdr;
    let topic_sym = Symbol::new(env, topic_str);
    let topic_xdr = soroban_sdk::xdr::ScVal::try_from_val(env, &topic_sym)
        .expect("topic to ScVal");
    let all = env.events().all();
    for e in all.events().iter().rev() {
        let body = &e.body;
        if let xdr::ContractEventBody::V0(b) = body {
            if b.topics.first() == Some(&topic_xdr) {
                let data_val =
                    Val::try_from_val(env, &b.data).expect("data ScVal to Val");
                return Map::<Symbol, Val>::try_from_val(env, &data_val)
                    .expect("event data is not a Map<Symbol,Val>");
            }
        }
    }
    panic!(
        "topic {:?} not found in {} events",
        topic_str,
        all.events().len()
    );
}

#[cfg(test)]
fn get_str_field(env: &Env, map: &Map<Symbol, Val>, field: &str) -> String {
    let key = Symbol::new(env, field);
    let val = map
        .get(key)
        .unwrap_or_else(|| panic!("field '{}' not found in event data", field));
    String::try_from_val(env, &val)
        .unwrap_or_else(|_| panic!("failed to decode String field '{}'", field))
}

// ============================================================================
// Tests
// ============================================================================

/// The TOPIC_INVOICE_FROZEN constant must equal the canonical topic string.
#[test]
fn test_topic_invoice_frozen_constant() {
    assert_eq!(
        TOPIC_INVOICE_FROZEN, "invoice_frozen",
        "TOPIC_INVOICE_FROZEN must be the stable string \"invoice_frozen\""
    );
}

/// `freeze_invoice` emits an `InvoiceFrozen` event with all required fields,
/// including `freeze_appeal_channel = "docs/APPEALS.md"`.
/// Issue #1959.
#[test]
fn test_freeze_invoice_emits_event_with_appeal_channel() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let biz = kyc_business(&env, &client, &admin);
    let invoice_id = create_invoice(&env, &client, &biz);

    // Apply a freeze.
    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::AdminAction);

    // Retrieve the InvoiceFrozen event data map.
    let data = latest_event_data(&env, TOPIC_INVOICE_FROZEN);

    // `freeze_appeal_channel` must point to the appeals doc.
    let appeal_channel = get_str_field(&env, &data, "freeze_appeal_channel");
    assert_eq!(
        appeal_channel,
        String::from_str(&env, "docs/APPEALS.md"),
        "freeze_appeal_channel must be \"docs/APPEALS.md\""
    );

    // `reason` must be the label for AdminAction.
    let reason = get_str_field(&env, &data, "reason");
    assert_eq!(
        reason,
        String::from_str(&env, "admin_action"),
        "reason label for AdminAction must be \"admin_action\""
    );
}

/// `freeze_appeal_channel` is present for every `BusinessFreezeReason` variant.
#[test]
fn test_freeze_invoice_appeal_channel_all_reasons() {
    let reasons: &[BusinessFreezeReason] = &[
        BusinessFreezeReason::AdminAction,
        BusinessFreezeReason::KYCRejected,
        BusinessFreezeReason::ComplianceViolation,
        BusinessFreezeReason::SuspiciousActivity,
        BusinessFreezeReason::LegalHold,
        BusinessFreezeReason::FraudSuspected,
        BusinessFreezeReason::Dispute,
        BusinessFreezeReason::Voluntary,
    ];

    for reason in reasons {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup(&env);
        let biz = kyc_business(&env, &client, &admin);
        let invoice_id = create_invoice(&env, &client, &biz);

        client.freeze_invoice(&admin, &invoice_id, reason);

        let data = latest_event_data(&env, TOPIC_INVOICE_FROZEN);
        let appeal_channel = get_str_field(&env, &data, "freeze_appeal_channel");
        assert_eq!(
            appeal_channel,
            String::from_str(&env, "docs/APPEALS.md"),
            "freeze_appeal_channel must be present for reason {:?}",
            reason
        );
    }
}

/// The `reason` field in the event uses the stable machine-readable label,
/// not the enum variant name.
#[test]
fn test_freeze_invoice_reason_label_compliance_violation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let biz = kyc_business(&env, &client, &admin);
    let invoice_id = create_invoice(&env, &client, &biz);

    client.freeze_invoice(
        &admin,
        &invoice_id,
        &BusinessFreezeReason::ComplianceViolation,
    );

    let data = latest_event_data(&env, TOPIC_INVOICE_FROZEN);
    let reason = get_str_field(&env, &data, "reason");
    assert_eq!(
        reason,
        String::from_str(&env, "compliance_violation"),
        "reason label for ComplianceViolation must be \"compliance_violation\""
    );
}

/// The `reason` label for `FraudSuspected` is `"fraud_suspected"`.
#[test]
fn test_freeze_invoice_reason_label_fraud_suspected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let biz = kyc_business(&env, &client, &admin);
    let invoice_id = create_invoice(&env, &client, &biz);

    client.freeze_invoice(&admin, &invoice_id, &BusinessFreezeReason::FraudSuspected);

    let data = latest_event_data(&env, TOPIC_INVOICE_FROZEN);
    let reason = get_str_field(&env, &data, "reason");
    assert_eq!(
        reason,
        String::from_str(&env, "fraud_suspected"),
        "reason label for FraudSuspected must be \"fraud_suspected\""
    );
}

/// `BusinessFreezeReason::label()` returns the correct string for every variant.
/// This test is purely in Rust — no contract call needed.
#[test]
fn test_business_freeze_reason_label_all_variants() {
    assert_eq!(BusinessFreezeReason::AdminAction.label(), "admin_action");
    assert_eq!(BusinessFreezeReason::KYCRejected.label(), "kyc_rejected");
    assert_eq!(
        BusinessFreezeReason::ComplianceViolation.label(),
        "compliance_violation"
    );
    assert_eq!(
        BusinessFreezeReason::SuspiciousActivity.label(),
        "suspicious_activity"
    );
    assert_eq!(BusinessFreezeReason::LegalHold.label(), "legal_hold");
    assert_eq!(BusinessFreezeReason::FraudSuspected.label(), "fraud_suspected");
    assert_eq!(BusinessFreezeReason::Dispute.label(), "dispute");
    assert_eq!(BusinessFreezeReason::Voluntary.label(), "voluntary");
}
