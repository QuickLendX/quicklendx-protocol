//! Integration tests for the `get_freshness` contract endpoint.
//!
//! Verifies that the contract-level freshness query returns the correct
//! Map<String, String> with all four required keys and correct values.

use super::*;
use soroban_sdk::{testutils::Ledger, Env, String};

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

#[test]
fn test_get_freshness_returns_all_keys() {
    let (env, client) = setup();
    env.ledger().set_sequence_number(500);
    env.ledger().set_timestamp(1_700_000_000);

    let result = client.get_freshness(&500u32, &1_700_000_000u64, &0u32);

    assert!(result.contains_key(String::from_str(&env, "last_indexed_ledger")));
    assert!(result.contains_key(String::from_str(&env, "index_lag_seconds")));
    assert!(result.contains_key(String::from_str(&env, "last_updated_at")));
    assert!(result.contains_key(String::from_str(&env, "cursor")));
}

#[test]
fn test_get_freshness_zero_lag() {
    let (env, client) = setup();
    env.ledger().set_sequence_number(500);
    env.ledger().set_timestamp(1_700_000_000);

    let result = client.get_freshness(&500u32, &1_700_000_000u64, &0u32);

    let lag = result
        .get(String::from_str(&env, "index_lag_seconds"))
        .unwrap();
    assert_eq!(lag, String::from_str(&env, "0"));
}

#[test]
fn test_get_freshness_positive_lag() {
    let (env, client) = setup();
    env.ledger().set_sequence_number(500);
    env.ledger().set_timestamp(1_700_000_060); // 60 s ahead of indexed

    let result = client.get_freshness(&499u32, &1_700_000_000u64, &0u32);

    let lag = result
        .get(String::from_str(&env, "index_lag_seconds"))
        .unwrap();
    assert_eq!(lag, String::from_str(&env, "60"));
}

#[test]
fn test_get_freshness_cursor_encodes_seq_and_offset() {
    let (env, client) = setup();
    env.ledger().set_sequence_number(1000);
    env.ledger().set_timestamp(1_700_000_000);

    let result = client.get_freshness(&1000u32, &1_700_000_000u64, &25u32);

    let cursor = result.get(String::from_str(&env, "cursor")).unwrap();
    assert_eq!(cursor, String::from_str(&env, "1000_25"));
}

#[test]
fn test_get_freshness_last_updated_at_is_iso8601() {
    let (env, client) = setup();
    env.ledger().set_sequence_number(1000);
    env.ledger().set_timestamp(1_700_000_000);

    let result = client.get_freshness(&1000u32, &1_700_000_000u64, &0u32);

    let ts = result
        .get(String::from_str(&env, "last_updated_at"))
        .unwrap();
    // Must be exactly 20 chars: "YYYY-MM-DDTHH:MM:SSZ"
    assert_eq!(ts.len(), 20);
    assert_eq!(ts, String::from_str(&env, "2023-11-14T22:13:20Z"));
}

#[test]
fn test_get_freshness_no_topology_in_values() {
    let (env, client) = setup();
    env.ledger().set_sequence_number(1000);
    env.ledger().set_timestamp(1_700_000_000);

    let result = client.get_freshness(&1000u32, &1_700_000_000u64, &0u32);

    // Cursor must only contain digits and underscore.
    let cursor = result.get(String::from_str(&env, "cursor")).unwrap();
    let cursor_len = cursor.len() as usize;
    let mut buf = [0u8; 22];
    cursor.copy_into_slice(&mut buf[..cursor_len]);
    for b in &buf[..cursor_len] {
        assert!(b.is_ascii_digit() || *b == b'_');
    }
}
