use soroban_sdk::{symbol_short, Address, Bytes, BytesN, Env, Symbol, Vec, IntoVal, String};
use crate::storage::{bump_persistent, extend_persistent_ttl};

pub const IDEMPOTENCY_MAP_KEY: Symbol = symbol_short!("idem_map");

pub fn idempotency_key(invoice_id: &BytesN<32>, investor: &Address, salt: &BytesN<32>, env: &Env) -> BytesN<32> {
    // Hash the concatenation of invoice_id, investor, and salt to produce a unique key
    let mut data = Bytes::new(env);
    data.append(&Bytes::from_array(env, &invoice_id.to_array()));
    data.append(&investor.to_string().to_bytes());
    data.append(&Bytes::from_array(env, &salt.to_array()));
    env.crypto().sha256(&data).into()
}

pub fn idempotency_exists(env: &Env, key: &BytesN<32>) -> bool {
    env.storage().persistent().has(&(IDEMPOTENCY_MAP_KEY, key.clone()))
}

pub fn store_idempotency(env: &Env, key: &BytesN<32>) {
    // Store a placeholder value (empty Bytes) to mark existence
    let placeholder: BytesN<32> = BytesN::from_array(env, &[0u8; 32]);
    env.storage().persistent().set(&(IDEMPOTENCY_MAP_KEY, key.clone()), &true);
    extend_persistent_ttl(env, &IDEMPOTENCY_MAP_KEY);
}
