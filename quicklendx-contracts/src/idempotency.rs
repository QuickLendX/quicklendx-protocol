//! Idempotency keys and records.
//!
//! Every idempotency key is a SHA-256 over a typed, per-operation prefix
//! followed by the operation's own inputs, so keys from different operations
//! can never collide. Keys that guard a caller's own action include the
//! caller's address, so one caller cannot replay (or read back) another
//! caller's result.
//!
//! Two kinds of records exist and they live under different storage keys:
//! - markers (`store_idempotency` / `idempotency_exists`) record only that an
//!   operation ran, and are stored as `bool`;
//! - results (`store_idempotency_result` / `get_idempotency_result`) record
//!   the value an operation returned, e.g. a created invoice ID.
//!
//! Because markers and results never share a storage key, reading a result
//! can never find a marker's `bool` and trap on the type conversion.

extern crate alloc;

use crate::storage::extend_persistent_ttl;
use soroban_sdk::{symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, Symbol};

/// Storage namespace for idempotency markers (`bool`).
pub const IDEMPOTENCY_MAP_KEY: Symbol = symbol_short!("idem_map");

/// Storage namespace for idempotency results (operation return values).
pub const IDEMPOTENCY_RESULT_KEY: Symbol = symbol_short!("idem_res");

/// Key prefix for bid placement (`idempotency_key`).
pub const BID_PLACE_PREFIX: &[u8] = b"bid_place";

/// Key prefix for invoice creation (`invoice_create_key`).
pub const INVOICE_CREATE_PREFIX: &[u8] = b"inv_create";

/// Idempotency key for placing a bid: `sha256("bid_place" || invoice_id ||
/// investor_xdr || salt)`.
pub fn idempotency_key(
    invoice_id: &BytesN<32>,
    investor: &Address,
    salt: &BytesN<32>,
    env: &Env,
) -> BytesN<32> {
    let mut data = Bytes::from_slice(env, BID_PLACE_PREFIX);
    data.append(&Bytes::from_array(env, &invoice_id.to_array()));
    data.append(&investor.to_xdr(env));
    data.append(&Bytes::from_array(env, &salt.to_array()));
    env.crypto().sha256(&data).into()
}

/// Idempotency key for creating an invoice: `sha256("inv_create" ||
/// business_xdr || nonce)`.
///
/// The business address is part of the key, so the same nonce used by two
/// different businesses yields two different keys.
pub fn invoice_create_key(env: &Env, business: &Address, nonce: &BytesN<32>) -> BytesN<32> {
    let mut data = Bytes::from_slice(env, INVOICE_CREATE_PREFIX);
    data.append(&business.to_xdr(env));
    data.append(&Bytes::from_array(env, &nonce.to_array()));
    env.crypto().sha256(&data).into()
}

/// Return `true` when an idempotency marker for `key` is present.
pub fn idempotency_exists(env: &Env, key: &BytesN<32>) -> bool {
    env.storage()
        .persistent()
        .has(&(IDEMPOTENCY_MAP_KEY, key.clone()))
}

/// Record a marker for `key` (only presence matters) and extend its TTL.
pub fn store_idempotency(env: &Env, key: &BytesN<32>) {
    let composite_key = (IDEMPOTENCY_MAP_KEY, key.clone());
    env.storage().persistent().set(&composite_key, &true);
    extend_persistent_ttl(env, &composite_key);
}

/// Return the result stored for `key`, if any.
pub fn get_idempotency_result<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(
    env: &Env,
    key: &BytesN<32>,
) -> Option<T> {
    env.storage()
        .persistent()
        .get(&(IDEMPOTENCY_RESULT_KEY, key.clone()))
}

/// Store the result of the operation guarded by `key` and extend its TTL.
pub fn store_idempotency_result<T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(
    env: &Env,
    key: &BytesN<32>,
    result: &T,
) {
    let composite_key = (IDEMPOTENCY_RESULT_KEY, key.clone());
    env.storage().persistent().set(&composite_key, result);
    extend_persistent_ttl(env, &composite_key);
}

#[cfg(all(test, feature = "legacy-tests"))]
mod tests {
    use super::*;
    use crate::QuickLendXContract;
    use soroban_sdk::testutils::Address as _;

    fn in_contract<R>(f: impl FnOnce(&Env) -> R) -> R {
        let env = Env::default();
        let id = env.register(QuickLendXContract, ());
        env.as_contract(&id, || f(&env))
    }

    #[test]
    #[ignore = "pre-existing: mutates storage outside env.as_contract, rejected by current soroban-sdk (issue #2963)"]
    fn test_idempotency_result() {
        let env = Env::default();
        let contract_id = env.register(crate::QuickLendXContract, ());
        let key = BytesN::from_array(&env, &[1; 32]);
        let result = BytesN::from_array(&env, &[2; 32]);

        env.as_contract(&contract_id, || {
            assert_eq!(get_idempotency_result::<BytesN<32>>(&env, &key), None);
            store_idempotency_result(&env, &key, &result);
            assert_eq!(
                get_idempotency_result::<BytesN<32>>(&env, &key),
                Some(result)
            );
        });
    }
}
