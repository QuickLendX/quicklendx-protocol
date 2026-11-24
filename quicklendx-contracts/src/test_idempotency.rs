use crate::idempotency::{
    get_idempotency_result, idempotency_exists, idempotency_key, invoice_create_key,
    store_idempotency, store_idempotency_result, IDEMPOTENCY_MAP_KEY, IDEMPOTENCY_RESULT_KEY,
};
use crate::storage::{PERSISTENT_BUMP_THRESHOLD, PERSISTENT_EXTEND_TO};
use crate::QuickLendXContract;
use soroban_sdk::{
    contracttype,
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, Symbol,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct StoredResult {
    amount: i128,
    recipient: Address,
}

fn assert_entry_can_be_bumped(env: &Env, key: &(Symbol, BytesN<32>)) {
    let ttl = env.storage().persistent().get_ttl(key);
    assert!(ttl >= PERSISTENT_EXTEND_TO);

    let advance_by = ttl - PERSISTENT_BUMP_THRESHOLD + 1;
    env.ledger()
        .set_sequence_number(env.ledger().sequence() + advance_by);
    assert_eq!(
        env.storage().persistent().get_ttl(key),
        PERSISTENT_BUMP_THRESHOLD - 1
    );
}

#[test]
fn idempotency_keys_are_deterministic_and_domain_separated() {
    let env = Env::default();
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let nonce = BytesN::from_array(&env, &[2; 32]);
    let other_nonce = BytesN::from_array(&env, &[13; 32]);
    let salt = BytesN::from_array(&env, &[3; 32]);
    let caller = Address::generate(&env);
    let other_caller = Address::generate(&env);
    let other_invoice = BytesN::from_array(&env, &[4; 32]);
    let other_salt = BytesN::from_array(&env, &[5; 32]);

    let bid_key = idempotency_key(&invoice_id, &caller, &salt, &env);
    assert_eq!(bid_key, idempotency_key(&invoice_id, &caller, &salt, &env));
    assert_ne!(
        bid_key,
        idempotency_key(&other_invoice, &caller, &salt, &env)
    );
    assert_ne!(
        bid_key,
        idempotency_key(&invoice_id, &other_caller, &salt, &env)
    );
    assert_ne!(
        bid_key,
        idempotency_key(&invoice_id, &caller, &other_salt, &env)
    );

    let invoice_key = invoice_create_key(&env, &caller, &nonce);
    assert_eq!(invoice_key, invoice_create_key(&env, &caller, &nonce));
    assert_ne!(invoice_key, invoice_create_key(&env, &other_caller, &nonce));
    assert_ne!(invoice_key, invoice_create_key(&env, &caller, &other_nonce));
    assert_ne!(
        bid_key,
        invoice_create_key(&env, &caller, &salt),
        "operation key domains must not overlap"
    );
    assert_ne!(bid_key, invoice_key);
}

#[test]
fn idempotency_markers_report_presence_only() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let key = BytesN::from_array(&env, &[6; 32]);

    env.as_contract(&contract_id, || {
        assert!(!idempotency_exists(&env, &key));
        assert_eq!(get_idempotency_result::<i128>(&env, &key), None);

        store_idempotency(&env, &key);

        assert!(idempotency_exists(&env, &key));
        assert_eq!(
            get_idempotency_result::<i128>(&env, &key),
            None,
            "marker and result storage use separate namespaces"
        );
    });
}

#[test]
fn idempotency_results_round_trip_supported_value_types() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let integer_key = BytesN::from_array(&env, &[7; 32]);
    let bytes_key = BytesN::from_array(&env, &[8; 32]);
    let struct_key = BytesN::from_array(&env, &[9; 32]);
    let bytes_result = BytesN::from_array(&env, &[10; 32]);
    let struct_result = StoredResult {
        amount: 987_654_321,
        recipient: Address::generate(&env),
    };

    env.as_contract(&contract_id, || {
        assert_eq!(get_idempotency_result::<i128>(&env, &integer_key), None);
        assert_eq!(get_idempotency_result::<BytesN<32>>(&env, &bytes_key), None);
        assert_eq!(
            get_idempotency_result::<StoredResult>(&env, &struct_key),
            None
        );

        store_idempotency_result(&env, &integer_key, &-123_456_789i128);
        store_idempotency_result(&env, &bytes_key, &bytes_result);
        store_idempotency_result(&env, &struct_key, &struct_result);

        assert_eq!(
            get_idempotency_result::<i128>(&env, &integer_key),
            Some(-123_456_789)
        );
        assert_eq!(
            get_idempotency_result::<BytesN<32>>(&env, &bytes_key),
            Some(bytes_result)
        );
        assert_eq!(
            get_idempotency_result::<StoredResult>(&env, &struct_key),
            Some(struct_result)
        );
    });
}

#[test]
fn storing_markers_and_results_bumps_persistent_ttl() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let marker_key = BytesN::from_array(&env, &[11; 32]);
    let result_key = BytesN::from_array(&env, &[12; 32]);

    env.as_contract(&contract_id, || {
        store_idempotency(&env, &marker_key);
        let marker_storage_key = (IDEMPOTENCY_MAP_KEY, marker_key.clone());
        assert_entry_can_be_bumped(&env, &marker_storage_key);
        store_idempotency(&env, &marker_key);
        assert!(env.storage().persistent().get_ttl(&marker_storage_key) >= PERSISTENT_EXTEND_TO);

        store_idempotency_result(&env, &result_key, &42i128);
        let result_storage_key = (IDEMPOTENCY_RESULT_KEY, result_key.clone());
        assert_entry_can_be_bumped(&env, &result_storage_key);
        store_idempotency_result(&env, &result_key, &42i128);
        assert!(env.storage().persistent().get_ttl(&result_storage_key) >= PERSISTENT_EXTEND_TO);
    });
}
