//! Integration tests for the `get_freshness` contract endpoint.
//!
//! Verifies that the contract-level freshness query returns the correct
//! Map<String, String> with all four required keys and correct values.
//!
//! # Before/At/After Boundary Tests for `indexed_ledger_seq`
//!
//! - Before 0: `indexed_ledger_seq = 1` succeeds (minimum valid Soroban sequence).
//! - At 0: `indexed_ledger_seq = 0` is rejected with `InvalidLedgerSequence`.
//! - After: `indexed_ledger_seq = u32::MAX` succeeds at the opposite boundary.
//!
//! These tests run on every CI matrix entry (no feature gate).

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

    let result = client
        .get_freshness(&500u32, &1_700_000_000u64, &0u32)
        .unwrap();

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

    let result = client
        .get_freshness(&500u32, &1_700_000_000u64, &0u32)
        .unwrap();

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

    let result = client
        .get_freshness(&499u32, &1_700_000_000u64, &0u32)
        .unwrap();

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

    let result = client
        .get_freshness(&1000u32, &1_700_000_000u64, &25u32)
        .unwrap();

    let cursor = result.get(String::from_str(&env, "cursor")).unwrap();
    assert_eq!(cursor, String::from_str(&env, "1000_25"));
}

#[test]
fn test_get_freshness_last_updated_at_is_iso8601() {
    let (env, client) = setup();
    env.ledger().set_sequence_number(1000);
    env.ledger().set_timestamp(1_700_000_000);

    let result = client
        .get_freshness(&1000u32, &1_700_000_000u64, &0u32)
        .unwrap();

    let ts = result
        .get(String::from_str(&env, "last_updated_at"))
        .unwrap();
    // Must be exactly 20 chars: "YYYY-MM-DDTHH:MM:SSZ"
    assert_eq!(ts.len(), 20);
    assert_eq!(ts, String::from_str(&env, "2023-11-14T22:13:20Z"));
}

// =============================================================================
// Before/At/After boundary tests for indexed_ledger_seq
// =============================================================================

/// `indexed_ledger_seq = 1` is the minimum valid Soroban ledger sequence.
/// This is the "before zero" boundary: the smallest value that must succeed.
#[test]
fn get_freshness_ledger_seq_one_succeeds() {
    let (env, client) = setup();
    env.ledger().set_sequence_number(1);
    env.ledger().set_timestamp(1_700_000_000);

    let result = client.get_freshness(&1u32, &1_700_000_000u64, &0u32);
    assert!(
        result.is_ok(),
        "ledger_seq = 1 must succeed as minimum valid sequence"
    );
}

/// `indexed_ledger_seq = 0` is NOT a valid Soroban ledger sequence.
/// This is the "at zero" boundary: must be rejected with `InvalidLedgerSequence`.
#[test]
fn get_freshness_ledger_seq_zero_rejected() {
    let (env, client) = setup();
    env.ledger().set_timestamp(1_700_000_000);

    let err = client
        .try_get_freshness(&0u32, &1_700_000_000u64, &0u32)
        .expect_err("ledger_seq = 0 must be rejected");
    assert_eq!(
        err,
        Ok(crate::errors::QuickLendXError::InvalidLedgerSequence),
        "expected InvalidLedgerSequence error for ledger_seq = 0"
    );
}

/// `indexed_ledger_seq = u32::MAX` tests the opposite boundary.
/// This is the "after" boundary: the largest possible value that must still succeed.
#[test]
fn get_freshness_ledger_seq_max_succeeds() {
    let (env, client) = setup();
    env.ledger().set_sequence_number(u32::MAX);
    env.ledger().set_timestamp(1_700_000_000);

    let result = client.get_freshness(&u32::MAX, &1_700_000_000u64, &0u32);
    assert!(
        result.is_ok(),
        "ledger_seq = u32::MAX must succeed at the upper boundary"
    );
}

/// `indexed_ledger_seq = u32::MAX - 1` is just below the max boundary.
/// Ensures the upper boundary is genuinely valid, not just u32::MAX coincidentally.
#[test]
fn get_freshness_ledger_seq_one_below_max_succeeds() {
    let (env, client) = setup();
    let seq = u32::MAX - 1;
    env.ledger().set_sequence_number(seq);
    env.ledger().set_timestamp(1_700_000_000);

    let result = client.get_freshness(&seq, &1_700_000_000u64, &0u32);
    assert!(result.is_ok(), "ledger_seq = u32::MAX - 1 must succeed");
}

#[test]
fn test_get_freshness_no_topology_in_values() {
    let (env, client) = setup();
    env.ledger().set_sequence_number(1000);
    env.ledger().set_timestamp(1_700_000_000);

    let result = client
        .get_freshness(&1000u32, &1_700_000_000u64, &0u32)
        .unwrap();

    // Cursor must only contain digits and underscore.
    let cursor = result.get(String::from_str(&env, "cursor")).unwrap();
    let cursor_len = cursor.len() as usize;
    let mut buf = [0u8; 22];
    cursor.copy_into_slice(&mut buf[..cursor_len]);
    for b in &buf[..cursor_len] {
        assert!(b.is_ascii_digit() || *b == b'_');
    }
}
