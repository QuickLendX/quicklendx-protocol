#![cfg(test)]

use core::convert::TryInto;

use crate::{invoice::InvoiceStatus, QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

const TEST_INVOICE_AMOUNT: i128 = 1_000_000;
// Keep the burst below the current Soroban instance-storage entry size ceiling.
const HIGH_THROUGHPUT_SAMPLE: u32 = 24;

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client, contract_id)
}

fn pin_ledger_slot(env: &Env, timestamp: u64, sequence: u32) {
    env.ledger().set_timestamp(timestamp);
    env.ledger().set_sequence_number(sequence);
}

fn invoice_timestamp_segment(invoice_id: &BytesN<32>) -> u64 {
    let bytes = invoice_id.to_array();
    u64::from_be_bytes(bytes[0..8].try_into().unwrap())
}

fn invoice_sequence_segment(invoice_id: &BytesN<32>) -> u32 {
    let bytes = invoice_id.to_array();
    u32::from_be_bytes(bytes[8..12].try_into().unwrap())
}

fn invoice_counter_segment(invoice_id: &BytesN<32>) -> u32 {
    let bytes = invoice_id.to_array();
    u32::from_be_bytes(bytes[12..16].try_into().unwrap())
}

fn reserved_segment_is_zeroed(invoice_id: &BytesN<32>) -> bool {
    let bytes = invoice_id.to_array();
    bytes[16..32].iter().all(|byte| *byte == 0)
}

fn set_invoice_counter(env: &Env, contract_id: &Address, counter: u32) {
    env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .set(&symbol_short!("inv_cnt"), &counter);
    });
}

fn read_invoice_counter(env: &Env, contract_id: &Address) -> u32 {
    env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .get(&symbol_short!("inv_cnt"))
            .unwrap_or(0)
    })
}

fn store_invoice_with_description(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    description: &str,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400;
    client.store_invoice(
        business,
        &TEST_INVOICE_AMOUNT,
        currency,
        &due_date,
        &String::from_str(env, description),
        &crate::invoice::InvoiceCategory::Services,
        &Vec::new(env),
    )
}

#[test]
fn invoice_ids_remain_unique_under_same_ledger_slot() {
    let (env, client, contract_id) = setup();
    let timestamp = 1_700_000_000;
    let sequence = 42;
    pin_ledger_slot(&env, timestamp, sequence);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let mut ids = Vec::new(&env);

    for expected_counter in 0..HIGH_THROUGHPUT_SAMPLE {
        let invoice_id = store_invoice_with_description(
            &env,
            &client,
            &business,
            &currency,
            "high-throughput-invoice",
        );

        for existing_id in ids.iter() {
            assert_ne!(
                invoice_id, existing_id,
                "invoice IDs must stay unique within the same ledger slot"
            );
        }

        assert_eq!(invoice_timestamp_segment(&invoice_id), timestamp);
        assert_eq!(invoice_sequence_segment(&invoice_id), sequence);
        assert_eq!(invoice_counter_segment(&invoice_id), expected_counter);
        assert!(
            reserved_segment_is_zeroed(&invoice_id),
            "reserved invoice ID bytes must remain zeroed for deterministic IDs"
        );

        let stored_invoice = client.get_invoice(&invoice_id);
        assert_eq!(stored_invoice.id, invoice_id);
        assert_eq!(stored_invoice.business, business);
        assert_eq!(stored_invoice.amount, TEST_INVOICE_AMOUNT);
        assert_eq!(stored_invoice.status, InvoiceStatus::Pending);

        ids.push_back(invoice_id);
        assert_eq!(ids.len(), expected_counter + 1);
    }

    assert_eq!(
        read_invoice_counter(&env, &contract_id),
        HIGH_THROUGHPUT_SAMPLE
    );
}

#[test]
fn counter_rewind_collision_does_not_overwrite_existing_invoice() {
    let (env, client, contract_id) = setup();
    pin_ledger_slot(&env, 1_700_000_001, 77);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let original_id =
        store_invoice_with_description(&env, &client, &business, &currency, "seeded-origin");
    assert_eq!(invoice_counter_segment(&original_id), 0);

    // Simulate a regression where the monotonic counter is rewound while the original invoice
    // key still exists in storage.
    set_invoice_counter(&env, &contract_id, 0);

    let created_id =
        store_invoice_with_description(&env, &client, &business, &currency, "post-collision");

    assert_ne!(created_id, original_id);
    assert_eq!(invoice_counter_segment(&created_id), 1);

    let original_invoice = client.get_invoice(&original_id);
    assert_eq!(original_invoice.id, original_id);
    assert_eq!(
        original_invoice.description,
        String::from_str(&env, "seeded-origin")
    );
    assert_eq!(original_invoice.amount, TEST_INVOICE_AMOUNT);

    let created_invoice = client.get_invoice(&created_id);
    assert_eq!(created_invoice.id, created_id);
    assert_eq!(
        created_invoice.description,
        String::from_str(&env, "post-collision")
    );
    assert_eq!(created_invoice.amount, TEST_INVOICE_AMOUNT);

    assert_eq!(
        read_invoice_counter(&env, &contract_id),
        2,
        "counter must advance past the skipped collision candidate"
    );
}

#[test]
fn allocator_resumes_monotonic_ids_after_collision_skip() {
    let (env, client, contract_id) = setup();
    pin_ledger_slot(&env, 1_700_000_002, 99);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let original_id =
        store_invoice_with_description(&env, &client, &business, &currency, "original");
    assert_eq!(invoice_counter_segment(&original_id), 0);

    set_invoice_counter(&env, &contract_id, 0);

    let first_created =
        store_invoice_with_description(&env, &client, &business, &currency, "first-after-skip");
    let second_created =
        store_invoice_with_description(&env, &client, &business, &currency, "second-after-skip");

    assert_eq!(invoice_counter_segment(&first_created), 1);
    assert_eq!(invoice_counter_segment(&second_created), 2);
    assert_ne!(first_created, second_created);
    assert_ne!(first_created, original_id);
    assert_ne!(second_created, original_id);

    assert_eq!(
        read_invoice_counter(&env, &contract_id),
        3,
        "allocator must continue from the next free counter after a skipped collision"
    );
}
