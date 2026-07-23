#![cfg(test)]

use crate::invoice::Invoice;
use crate::payments::EscrowStorage;
use soroban_sdk::{symbol_short, testutils::Ledger, BytesN, Env};

#[test]
fn test_id_generation_stability_snapshots() {
    let env = Env::default();

    // Run 50 regression variations to assert structural rigidity across sequential blocks
    for i in 1..=50 {
        let mock_sequence = 100_000 + i;
        let mock_timestamp = 1_700_000_000 + (i * 60); // 1-minute steps

        // Set up the deterministic ledger state
        env.ledger().set(soroban_sdk::testutils::LedgerInfo {
            number: mock_sequence,
            timestamp: mock_timestamp,
            protocol_version: 20,
            sequence_number: mock_sequence,
            network_id: [0u8; 32],
            base_reserve: 100,
        });

        // 1. Validate INVOICE ID Allocator
        let inv_cnt_key = symbol_short!("inv_cnt");
        env.storage().instance().set(&inv_cnt_key, &(i as u32));

        let invoice_id = Invoice::allocate_id(&env);
        let inv_bytes = invoice_id.to_array();

        let expected_inv_ts = mock_timestamp.to_be_bytes();
        let expected_inv_seq = mock_sequence.to_be_bytes();
        let expected_inv_cnt = ((i - 1) as u32).to_be_bytes();

        assert_eq!(inv_bytes[0..8], expected_inv_ts);
        assert_eq!(inv_bytes[8..12], expected_inv_seq);
        assert_eq!(inv_bytes[12..16], expected_inv_cnt);

        // 2. Validate BID ID Allocator
        let bid_cnt_key = symbol_short!("bid_cnt");
        env.storage().instance().set(&bid_cnt_key, &(i as u64 - 1));

        let bid_id = crate::bid::BidIndexKey::generate_unique_bid_id(&env);
        let bid_bytes = bid_id.to_array();

        assert_eq!(bid_bytes[0], 0xB1);
        assert_eq!(bid_bytes[1], 0xD0);
        assert_eq!(bid_bytes[2..10], mock_timestamp.to_be_bytes());
        assert_eq!(bid_bytes[10..18], (i as u64).to_be_bytes());

        let expected_mix = mock_timestamp
            .saturating_add(i as u64)
            .saturating_add(0xB1D0);
        let expected_pad_byte = (expected_mix % 256) as u8;
        for byte_idx in 18..32 {
            assert_eq!(bid_bytes[byte_idx], expected_pad_byte);
        }

        // 3. Validate ESCROW ID Allocator
        let esc_cnt_key = symbol_short!("esc_cnt");
        env.storage().instance().set(&esc_cnt_key, &(i as u64 - 1));

        let escrow_id = EscrowStorage::generate_unique_escrow_id(&env);
        let esc_bytes = escrow_id.to_array();

        assert_eq!(esc_bytes[0], 0xE5);
        assert_eq!(esc_bytes[1], 0xC0);
    }
}
