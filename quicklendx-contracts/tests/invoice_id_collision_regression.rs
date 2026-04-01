use core::convert::TryInto;

use quicklendx_contracts::{types::InvoiceCategory, QuickLendXContract, QuickLendXContractClient};
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

fn invoice_counter_segment(invoice_id: &BytesN<32>) -> u32 {
    let bytes = invoice_id.to_array();
    u32::from_be_bytes(bytes[12..16].try_into().unwrap())
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
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

#[test]
fn invoice_ids_are_unique_and_monotonic_within_a_single_ledger_slot() {
    let (env, client, contract_id) = setup();
    pin_ledger_slot(&env, 1_700_100_000, 17);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let mut ids = std::collections::BTreeSet::new();

    for expected_counter in 0..HIGH_THROUGHPUT_SAMPLE {
        let invoice_id =
            store_invoice_with_description(&env, &client, &business, &currency, "throughput");
        assert!(
            ids.insert(invoice_id.to_array()),
            "invoice ID must be unique within the same ledger slot"
        );
        assert_eq!(invoice_counter_segment(&invoice_id), expected_counter);
    }

    assert_eq!(
        read_invoice_counter(&env, &contract_id),
        HIGH_THROUGHPUT_SAMPLE
    );
}

#[test]
fn counter_rewind_collision_skips_the_existing_invoice_key() {
    let (env, client, contract_id) = setup();
    pin_ledger_slot(&env, 1_700_100_001, 18);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let first_id = store_invoice_with_description(&env, &client, &business, &currency, "first");
    set_invoice_counter(&env, &contract_id, 0);

    let second_id = store_invoice_with_description(&env, &client, &business, &currency, "second");

    assert_ne!(first_id, second_id);
    assert_eq!(invoice_counter_segment(&first_id), 0);
    assert_eq!(invoice_counter_segment(&second_id), 1);
    assert_eq!(
        client.get_invoice(&first_id).description,
        String::from_str(&env, "first")
    );
    assert_eq!(
        client.get_invoice(&second_id).description,
        String::from_str(&env, "second")
    );
    assert_eq!(read_invoice_counter(&env, &contract_id), 2);
}
