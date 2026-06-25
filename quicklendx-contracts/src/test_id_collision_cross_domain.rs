//! Cross-domain ID collision proof for invoice, bid, escrow, and investment IDs.
//!
//! This regression test asserts the core keyspace invariant required by the
//! protocol: IDs from different domains may never collide, even when they are
//! generated in the same ledger slot and the counter inputs advance rapidly.
//!
//! Invariant:
//! - Invoice IDs are encoded from timestamp, sequence, and a monotonic per-invoice
//!   counter.
//! - Bid/Escrow/Investment IDs use domain-specific 2-byte prefixes plus timestamp
//!   and a monotonic per-domain counter.
//! - The escrow prefix (`0xE5C0`) and investment prefix (`0x1A4E`) are disjoint,
//!   ensuring the full 32-byte keyspaces cannot collide purely by using identical
//!   counter/timestamp inputs.
//! - The same ledger inputs plus the same counter state always reproduce the same ID.

#![cfg(test)]

use core::convert::TryInto;

use soroban_sdk::{symbol_short, BytesN, Env, Vec};

use crate::bid::BidStorage;
use crate::invoice::Invoice;
use crate::investment::InvestmentStorage;
use crate::payments::EscrowStorage;
use crate::storage::StorageKeys;

const FIXED_TIMESTAMP: u64 = 1_700_000_000;
const FIXED_SEQUENCE: u32 = 42;
const SAMPLE_COUNT: u32 = 64;

fn invoice_timestamp(id: &BytesN<32>) -> u64 {
    u64::from_be_bytes(id.to_array()[0..8].try_into().unwrap())
}

fn invoice_sequence(id: &BytesN<32>) -> u32 {
    u32::from_be_bytes(id.to_array()[8..12].try_into().unwrap())
}

fn invoice_counter(id: &BytesN<32>) -> u32 {
    u32::from_be_bytes(id.to_array()[12..16].try_into().unwrap())
}

fn dom_counter(id: &BytesN<32>) -> u64 {
    u64::from_be_bytes(id.to_array()[10..18].try_into().unwrap())
}

fn prefix(id: &BytesN<32>) -> [u8; 2] {
    let bytes = id.to_array();
    [bytes[0], bytes[1]]
}

fn assert_reserved_zeroed(id: &BytesN<32>) {
    let bytes = id.to_array();
    assert!(
        bytes[16..32].iter().all(|b| *b == 0),
        "invoice reserved bytes must be zeroed; got {:?}",
        &bytes[16..32]
    );
}

fn fix_ledger(env: &Env) {
    env.ledger().set_timestamp(FIXED_TIMESTAMP);
    env.ledger().set_sequence_number(FIXED_SEQUENCE);
}

fn set_counters(env: &Env, invoice_count: u64, bid_count: u64, escrow_count: u64, investment_count: u64) {
    env.storage()
        .persistent()
        .set(&StorageKeys::invoice_count(), &invoice_count);
    env.storage().instance().set(&symbol_short!("bid_cnt"), &bid_count);
    env.storage().instance().set(&symbol_short!("esc_cnt"), &escrow_count);
    env.storage()
        .instance()
        .set(&symbol_short!("invst_cnt"), &investment_count);
}

#[test]
fn test_cross_domain_id_uniqueness_and_monotonicity() {
    let env = Env::default();
    fix_ledger(&env);
    set_counters(&env, 0, 0, 0, 0);

    let mut seen: Vec<BytesN<32>> = Vec::new(&env);
    let mut last_invoice_counter: Option<u32> = None;
    let mut last_bid_counter: Option<u64> = None;
    let mut last_escrow_counter: Option<u64> = None;
    let mut last_investment_counter: Option<u64> = None;

    for expected_invoice_counter in 0..SAMPLE_COUNT {
        let invoice_id = Invoice::allocate_id(&env);
        assert_eq!(invoice_timestamp(&invoice_id), FIXED_TIMESTAMP);
        assert_eq!(invoice_sequence(&invoice_id), FIXED_SEQUENCE);
        assert_reserved_zeroed(&invoice_id);
        assert_eq!(invoice_counter(&invoice_id), expected_invoice_counter);

        let bid_id = BidStorage::generate_unique_bid_id(&env);
        assert_eq!(prefix(&bid_id), [0xB1, 0xD0]);
        assert_eq!(dom_counter(&bid_id), (expected_invoice_counter + 1) as u64);

        let escrow_id = EscrowStorage::generate_unique_escrow_id(&env);
        assert_eq!(prefix(&escrow_id), [0xE5, 0xC0]);
        assert_eq!(dom_counter(&escrow_id), (expected_invoice_counter + 1) as u64);

        let investment_id = InvestmentStorage::generate_unique_investment_id(&env);
        assert_eq!(prefix(&investment_id), [0x1A, 0x4E]);
        assert_eq!(dom_counter(&investment_id), (expected_invoice_counter + 1) as u64);

        if let Some(last) = last_invoice_counter {
            assert!(invoice_counter(&invoice_id) > last, "invoice counter must advance monotonically");
        }
        if let Some(last) = last_bid_counter {
            assert!(dom_counter(&bid_id) > last, "bid counter must advance monotonically");
        }
        if let Some(last) = last_escrow_counter {
            assert!(dom_counter(&escrow_id) > last, "escrow counter must advance monotonically");
        }
        if let Some(last) = last_investment_counter {
            assert!(dom_counter(&investment_id) > last, "investment counter must advance monotonically");
        }

        for id in [&invoice_id, &bid_id, &escrow_id, &investment_id] {
            assert!(
                !seen.contains(id),
                "duplicate ID detected across or within domains: {:?}",
                id.to_array()
            );
            seen.push_back(id.clone());
        }

        last_invoice_counter = Some(invoice_counter(&invoice_id));
        last_bid_counter = Some(dom_counter(&bid_id));
        last_escrow_counter = Some(dom_counter(&escrow_id));
        last_investment_counter = Some(dom_counter(&investment_id));
    }

    assert_eq!(seen.len(), SAMPLE_COUNT.saturating_mul(4), "must produce all unique IDs across domains");
}

#[test]
fn test_cross_domain_id_generation_is_deterministic_for_same_state() {
    let mut env_a = Env::default();
    let mut env_b = Env::default();
    fix_ledger(&env_a);
    fix_ledger(&env_b);
    set_counters(&env_a, 42, 100, 200, 300);
    set_counters(&env_b, 42, 100, 200, 300);

    assert_eq!(Invoice::allocate_id(&env_a), Invoice::allocate_id(&env_b));
    assert_eq!(
        BidStorage::generate_unique_bid_id(&env_a),
        BidStorage::generate_unique_bid_id(&env_b)
    );
    assert_eq!(
        EscrowStorage::generate_unique_escrow_id(&env_a),
        EscrowStorage::generate_unique_escrow_id(&env_b)
    );
    assert_eq!(
        InvestmentStorage::generate_unique_investment_id(&env_a),
        InvestmentStorage::generate_unique_investment_id(&env_b)
    );
}
