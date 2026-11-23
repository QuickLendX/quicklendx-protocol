use quicklendx_contracts::{
    invoice::InvoiceCategory, types::InvoiceStatus, QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{symbol_short, Address, Env, String, Vec};

fn store_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    description: &str,
) -> soroban_sdk::BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400;
    client.store_invoice(
        business,
        &1_000_i128,
        currency,
        &due_date,
        &String::from_str(env, description),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
    )
}

#[test]
fn counter_rewind_skips_occupied_invoice_id_without_overwriting_it() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_100_001);
    env.ledger().set_sequence_number(18);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let original = store_invoice(&env, &client, &business, &currency, "original");
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&symbol_short!("inv_cnt"), &0_u32);
    });

    let after_rewind = store_invoice(&env, &client, &business, &currency, "after rewind");
    assert_ne!(original, after_rewind);
    assert_eq!(&original.to_array()[12..16], &0_u32.to_be_bytes());
    assert_eq!(&after_rewind.to_array()[12..16], &1_u32.to_be_bytes());

    let original_invoice = client.get_invoice(&original);
    let new_invoice = client.get_invoice(&after_rewind);
    assert_eq!(
        original_invoice.description,
        String::from_str(&env, "original")
    );
    assert_eq!(
        new_invoice.description,
        String::from_str(&env, "after rewind")
    );
    assert_eq!(original_invoice.status, InvoiceStatus::Pending);
    assert_eq!(new_invoice.status, InvoiceStatus::Pending);

    let counter = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get::<_, u32>(&symbol_short!("inv_cnt"))
            .unwrap()
    });
    assert_eq!(counter, 2);
}
