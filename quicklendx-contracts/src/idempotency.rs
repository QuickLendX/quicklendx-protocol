use crate::storage::{bump_persistent, extend_persistent_ttl};

/// Storage key for the idempotency map.
pub const IDEMPOTENCY_MAP_KEY: Symbol = symbol_short!("idem_map");

pub fn idempotency_key(
    invoice_id: &BytesN<32>,
    investor: &Address,
    salt: &BytesN<32>,
    env: &Env,
) -> BytesN<32> {
    // Hash the concatenation of invoice_id, investor, and salt to produce a unique key
    let mut data = Bytes::new(env);
    data.append(&Bytes::from_array(env, &invoice_id.to_array()));
    data.append(&investor.to_xdr(env));
    data.append(&Bytes::from_array(env, &salt.to_array()));
    env.crypto().sha256(&data).into()
}

/// Return `true` when an idempotency record for `key` is already present in
/// persistent storage. Uses a composite `(IDEMPOTENCY_MAP_KEY, key)` tuple
/// key, which is the form the modern `soroban-sdk` storage API expects.
pub fn idempotency_exists(env: &Env, key: &BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .has(&(IDEMPOTENCY_MAP_KEY, key.clone()))
}

/// Mark `key` as processed in persistent storage. Stores a zero-filled
/// placeholder (the value is opaque — only presence matters) and bumps the
/// TTL so the marker does not expire mid-flight.
pub fn store_idempotency(env: &Env, key: &BytesN<32>) {
    let composite_key = (IDEMPOTENCY_MAP_KEY, key.clone());
    env.storage().persistent().set(&composite_key, &true);
    extend_persistent_ttl(env, &composite_key);
}

pub fn get_idempotency_result<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(env: &Env, key: &BytesN<32>) -> Option<T> {
    env.storage()
        .persistent()
        .get(&(IDEMPOTENCY_MAP_KEY, key.clone()))
}

pub fn store_idempotency_result<T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, key: &BytesN<32>, result: &T) {
    let composite_key = (IDEMPOTENCY_MAP_KEY, key.clone());
    env.storage().persistent().set(&composite_key, result);
    extend_persistent_ttl(env, &composite_key);
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_idempotency_result() {
        let env = Env::default();
        let key = BytesN::from_array(&env, &[1; 32]);
        let result = BytesN::from_array(&env, &[2; 32]);
        
        assert_eq!(get_idempotency_result::<BytesN<32>>(&env, &key), None);
        store_idempotency_result(&env, &key, &result);
        assert_eq!(get_idempotency_result::<BytesN<32>>(&env, &key), Some(result));
    }
}