use crate::storage::extend_persistent_ttl;
use soroban_sdk::{symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, Symbol};

pub const IDEMPOTENCY_MAP_KEY: Symbol = symbol_short!("idem_map");

pub fn idempotency_key(
    invoice_id: &BytesN<32>,
    investor: &Address,
    salt: &BytesN<32>,
    env: &Env,
) -> BytesN<32> {
    // Hash the concatenation of invoice_id, investor, and salt to produce a unique key
    let mut data = Bytes::from_array(env, &invoice_id.to_array());
    data.append(&investor.to_xdr(env));
    data.append(&Bytes::from_array(env, &salt.to_array()));
    env.crypto().sha256(&data).into()
}

fn storage_key(key: &BytesN<32>) -> (Symbol, BytesN<32>) {
    (IDEMPOTENCY_MAP_KEY, key.clone())
}

pub fn idempotency_exists(env: &Env, key: &BytesN<32>) -> bool {
    env.storage().persistent().has(&storage_key(key))
}

pub fn store_idempotency(env: &Env, key: &BytesN<32>) {
    // Store a placeholder value (empty Bytes) to mark existence
    let placeholder: BytesN<32> = BytesN::from_array(env, &[0u8; 32]);
    let storage_key = storage_key(key);
    env.storage().persistent().set(&storage_key, &placeholder);
    extend_persistent_ttl(env, &storage_key);
}
