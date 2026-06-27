use soroban_sdk::{symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, Symbol};

use crate::storage::extend_persistent_ttl;

/// Storage key for the idempotency map.
pub const IDEMPOTENCY_MAP_KEY: Symbol = symbol_short!("idem_map");

/// Generate a deterministic 32-byte idempotency key from the invoice,
/// investor, and a caller-supplied salt. Soroban `String`/`Address` are
/// host types without a direct `as_bytes()` accessor, so we serialize each
/// component to XDR before hashing.
pub fn idempotency_key(
    invoice_id: &BytesN<32>,
    investor: &Address,
    salt: &BytesN<32>,
    env: &Env,
) -> BytesN<32> {
    let mut data = Bytes::new(env);
    data.append(&invoice_id.to_xdr(env));
    data.append(&investor.to_xdr(env));
    data.append(&salt.to_xdr(env));
    env.crypto().sha256(&data).into()
}

/// Return `true` when an idempotency record for `key` is already present in
/// persistent storage. Uses a composite `(IDEMPOTENCY_MAP_KEY, key)` tuple
/// key, which is the form the modern `soroban-sdk` storage API expects.
pub fn idempotency_exists(env: &Env, key: &BytesN<32>) -> bool {
    let storage_key = (IDEMPOTENCY_MAP_KEY, key.clone());
    env.storage().persistent().has(&storage_key)
}

/// Mark `key` as processed in persistent storage. Stores a zero-filled
/// placeholder (the value is opaque — only presence matters) and bumps the
/// TTL so the marker does not expire mid-flight.
pub fn store_idempotency(env: &Env, key: &BytesN<32>) {
    let placeholder: BytesN<32> = BytesN::from_array(env, &[0u8; 32]);
    let storage_key = (IDEMPOTENCY_MAP_KEY, key.clone());
    env.storage().persistent().set(&storage_key, &placeholder);
    extend_persistent_ttl(env, &storage_key);
}
