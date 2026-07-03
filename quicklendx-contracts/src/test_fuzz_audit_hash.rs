#![cfg(all(test, feature = "fuzz-tests"))]

//! Fuzz harness for audit-chain `entry_hash` determinism and field sensitivity.

use crate::audit::{AuditLogEntry, AuditOperation, AuditStorage};
use crate::QuickLendXContract;
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String as SorobanString,
};

#[derive(Clone, Debug)]
struct AuditFields {
    operation: AuditOperation,
    timestamp: u64,
    old_value: Option<&'static str>,
    new_value: Option<&'static str>,
    amount: Option<i128>,
    additional_data: Option<&'static str>,
    block_height: u32,
    transaction_hash: Option<[u8; 32]>,
}

fn arb_operation() -> impl Strategy<Value = AuditOperation> {
    prop_oneof![
        Just(AuditOperation::InvoiceCreated),
        Just(AuditOperation::InvoiceUploaded),
        Just(AuditOperation::InvoiceVerified),
        Just(AuditOperation::InvoiceFunded),
        Just(AuditOperation::InvoicePaid),
        Just(AuditOperation::InvoiceDefaulted),
        Just(AuditOperation::InvoiceStatusChanged),
        Just(AuditOperation::InvoiceRated),
        Just(AuditOperation::BidPlaced),
        Just(AuditOperation::BidAccepted),
        Just(AuditOperation::BidWithdrawn),
        Just(AuditOperation::EscrowCreated),
        Just(AuditOperation::EscrowReleased),
        Just(AuditOperation::EscrowRefunded),
        Just(AuditOperation::PaymentProcessed),
        Just(AuditOperation::SettlementCompleted),
        Just(AuditOperation::ConfigProtocolChanged),
        Just(AuditOperation::ConfigFeeChanged),
        Just(AuditOperation::ConfigTreasuryChanged),
        Just(AuditOperation::ConfigFeeStructureChanged),
        Just(AuditOperation::ConfigRevenueDistributionChanged),
    ]
}

fn arb_optional_string() -> impl Strategy<Value = Option<&'static str>> {
    prop_oneof![
        Just(None),
        Just(Some("")),
        Just(Some("a")),
        Just(Some("status:verified")),
        Just(Some("status:defaulted")),
    ]
}

fn arb_optional_amount() -> impl Strategy<Value = Option<i128>> {
    prop_oneof![
        Just(None),
        Just(Some(0)),
        Just(Some(1)),
        Just(Some(100_000)),
        Just(Some(i128::MAX)),
    ]
}

fn arb_optional_hash() -> impl Strategy<Value = Option<[u8; 32]>> {
    prop_oneof![
        Just(None),
        Just(Some([0; 32])),
        Just(Some([1; 32])),
        Just(Some([0xAA; 32])),
    ]
}

fn arb_fields() -> impl Strategy<Value = AuditFields> {
    (
        arb_operation(),
        0u64..=1_000_000,
        arb_optional_string(),
        arb_optional_string(),
        arb_optional_amount(),
        arb_optional_string(),
        0u32..=1_000_000,
        arb_optional_hash(),
    )
        .prop_map(
            |(
                operation,
                timestamp,
                old_value,
                new_value,
                amount,
                additional_data,
                block_height,
                transaction_hash,
            )| AuditFields {
                operation,
                timestamp,
                old_value,
                new_value,
                amount,
                additional_data,
                block_height,
                transaction_hash,
            },
        )
}

fn make_contract_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(2_000_000);
    env.ledger().set_sequence(2_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

fn entry_from_fields(
    env: &Env,
    fields: &AuditFields,
    audit_id: BytesN<32>,
    invoice_id: BytesN<32>,
    actor: Address,
    prev_hash: BytesN<32>,
) -> AuditLogEntry {
    AuditLogEntry {
        audit_id,
        invoice_id,
        operation: fields.operation.clone(),
        actor,
        timestamp: fields.timestamp,
        old_value: fields
            .old_value
            .map(|value| SorobanString::from_str(env, value)),
        new_value: fields
            .new_value
            .map(|value| SorobanString::from_str(env, value)),
        amount: fields.amount,
        additional_data: fields
            .additional_data
            .map(|value| SorobanString::from_str(env, value)),
        block_height: fields.block_height,
        transaction_hash: fields
            .transaction_hash
            .map(|hash| BytesN::from_array(env, &hash)),
        prev_hash,
    }
}

fn alternate_operation(operation: &AuditOperation) -> AuditOperation {
    if matches!(operation, AuditOperation::InvoiceCreated) {
        AuditOperation::BidPlaced
    } else {
        AuditOperation::InvoiceCreated
    }
}

fn flip_optional_string(value: Option<&'static str>) -> Option<&'static str> {
    match value {
        None => Some("present"),
        Some("") => Some("non-empty"),
        Some(_) => None,
    }
}

fn flip_optional_amount(value: Option<i128>) -> Option<i128> {
    match value {
        None => Some(0),
        Some(0) => Some(1),
        Some(_) => None,
    }
}

fn flip_optional_hash(value: Option<[u8; 32]>) -> Option<[u8; 32]> {
    match value {
        None => Some([0; 32]),
        Some(hash) if hash == [0; 32] => Some([1; 32]),
        Some(_) => None,
    }
}

fn hash_for_fields(env: &Env, contract_id: &Address, fields: &AuditFields) -> BytesN<32> {
    env.as_contract(contract_id, || {
        let entry = entry_from_fields(
            env,
            fields,
            BytesN::from_array(env, &[0xA1; 32]),
            BytesN::from_array(env, &[0xB2; 32]),
            Address::generate(env),
            AuditLogEntry::genesis_prev_hash(env),
        );
        entry.entry_hash(env)
    })
}

proptest! {
    #![proptest_config(ProptestConfig::from_env())]

    #[test]
    fn test_fuzz_audit_hash_is_stable_for_identical_entry(fields in arb_fields()) {
        let (env, contract_id) = make_contract_env();

        let (first, second) = env.as_contract(&contract_id, || {
            let entry = entry_from_fields(
                &env,
                &fields,
                BytesN::from_array(&env, &[0xA1; 32]),
                BytesN::from_array(&env, &[0xB2; 32]),
                Address::generate(&env),
                AuditLogEntry::genesis_prev_hash(&env),
            );
            (entry.entry_hash(&env), entry.entry_hash(&env))
        });

        prop_assert_eq!(first, second);
    }

    #[test]
    fn test_fuzz_audit_hash_changes_when_single_fields_flip(fields in arb_fields()) {
        let (env, contract_id) = make_contract_env();
        let base_hash = hash_for_fields(&env, &contract_id, &fields);

        let mut changed_operation = fields.clone();
        changed_operation.operation = alternate_operation(&fields.operation);
        prop_assert_ne!(base_hash, hash_for_fields(&env, &contract_id, &changed_operation));

        let mut changed_optional_string = fields.clone();
        changed_optional_string.old_value = flip_optional_string(fields.old_value);
        prop_assert_ne!(
            base_hash,
            hash_for_fields(&env, &contract_id, &changed_optional_string)
        );

        let mut changed_optional_amount = fields.clone();
        changed_optional_amount.amount = flip_optional_amount(fields.amount);
        prop_assert_ne!(
            base_hash,
            hash_for_fields(&env, &contract_id, &changed_optional_amount)
        );

        let mut changed_optional_hash = fields.clone();
        changed_optional_hash.transaction_hash = flip_optional_hash(fields.transaction_hash);
        prop_assert_ne!(base_hash, hash_for_fields(&env, &contract_id, &changed_optional_hash));
    }

    #[test]
    fn test_fuzz_audit_hash_chain_verifies_adjacent_distinct_entries(
        first_value in arb_optional_string(),
        second_value in arb_optional_string(),
        amount in arb_optional_amount(),
        transaction_hash in arb_optional_hash(),
    ) {
        let (env, contract_id) = make_contract_env();

        env.as_contract(&contract_id, || {
            let invoice_id = BytesN::from_array(&env, &[0xC3; 32]);
            let actor = Address::generate(&env);
            let first = AuditFields {
                operation: AuditOperation::InvoiceCreated,
                timestamp: 100,
                old_value: None,
                new_value: first_value,
                amount: None,
                additional_data: None,
                block_height: 100,
                transaction_hash: None,
            };
            let first_entry = entry_from_fields(
                &env,
                &first,
                BytesN::from_array(&env, &[0xD4; 32]),
                invoice_id.clone(),
                actor.clone(),
                AuditLogEntry::genesis_prev_hash(&env),
            );
            let first_hash = first_entry.entry_hash(&env);

            let second = AuditFields {
                operation: AuditOperation::BidPlaced,
                timestamp: 101,
                old_value: first_value,
                new_value: second_value,
                amount,
                additional_data: Some("adjacent-entry"),
                block_height: 101,
                transaction_hash,
            };
            let second_entry = entry_from_fields(
                &env,
                &second,
                BytesN::from_array(&env, &[0xE5; 32]),
                invoice_id.clone(),
                actor,
                first_hash.clone(),
            );
            let second_hash = second_entry.entry_hash(&env);

            prop_assert_ne!(first_hash, second_hash);
            AuditStorage::store_audit_entry(&env, &first_entry);
            AuditStorage::store_audit_entry(&env, &second_entry);
            prop_assert!(AuditStorage::verify_audit_chain(&env, &invoice_id));

            let mut tampered_first = first_entry.clone();
            tampered_first.new_value = Some(SorobanString::from_str(&env, "tampered"));
            env.storage()
                .instance()
                .set(&first_entry.audit_id, &tampered_first);
            prop_assert!(!AuditStorage::verify_audit_chain(&env, &invoice_id));

            Ok(())
        })?;
    }
}
