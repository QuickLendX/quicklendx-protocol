use soroban_sdk::{symbol_short, Address, Bytes, BytesN, Env, Symbol};
use crate::storage::extend_persistent_ttl;

/// Storage key for the idempotency map.
pub const IDEMPOTENCY_MAP_KEY: Symbol = symbol_short!("idem_map");

pub fn idempotency_key(
    invoice_id: &BytesN<32>,
    investor: &Address,
    bid_amount: i128,
    expected_return: i128,
    salt: &BytesN<32>,
    env: &Env,
) -> BytesN<32> {
    // Hash the stable bid inputs so exact retries are rejected without
    // blocking distinct bids on the same invoice.
    let bid_amount_bytes = bid_amount.to_be_bytes();
    let expected_return_bytes = expected_return.to_be_bytes();
    let mut data = Bytes::new(env);
    data.append(&Bytes::from_array(env, &invoice_id.to_array()));
    data.append(&investor.to_string().to_bytes());
    data.append(&Bytes::from_array(env, &bid_amount_bytes));
    data.append(&Bytes::from_array(env, &expected_return_bytes));
    data.append(&Bytes::from_array(env, &salt.to_array()));
    env.crypto().sha256(&data).into()
}

/// Return `true` when an idempotency record for `key` is already present in
/// persistent storage. Uses a composite `(IDEMPOTENCY_MAP_KEY, key)` tuple
/// key, which is the form the modern `soroban-sdk` storage API expects.
pub fn idempotency_exists(env: &Env, key: &BytesN<32>) -> bool {
    env.storage().persistent().has(&(IDEMPOTENCY_MAP_KEY, key.clone()))
}

/// Mark `key` as processed in persistent storage. Stores a zero-filled
/// placeholder (the value is opaque — only presence matters) and bumps the
/// TTL so the marker does not expire mid-flight.
pub fn store_idempotency(env: &Env, key: &BytesN<32>) {
    let storage_key = (IDEMPOTENCY_MAP_KEY, key.clone());
    env.storage().persistent().set(&storage_key, &true);
    extend_persistent_ttl(env, &storage_key);
}
