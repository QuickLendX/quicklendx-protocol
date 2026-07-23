// src/test/duplicate_bid_test.rs
//! Test that submitting the same bid with identical (invoice_id, investor, salt) fails with `DuplicateBid`.

use super::*; // bring the test utilities into scope

#[test]
fn test_duplicate_bid_idempotency() {
    // Setup environment
    let env = Env::default();
    let contract = QuickLendXContractClient::new(&env, &env.register_contract(None, QuickLendXContract {}));

    // Create a mock investor and invoice
    let investor = env.accounts().generate();
    let issuer = env.accounts().generate();
    let invoice_id = BytesN::from_array(&env, &[0u8; 32]);
    let amount: i128 = 1_000;
    let expected_return: i128 = 1_100;

    // Assume invoice creation helper exists
    contract.create_invoice(&issuer, &invoice_id, &amount, &expected_return);

    // Prepare a deterministic salt
    let salt = BytesN::from_array(&env, &[1u8; 32]);

    // First bid should succeed
    let first_bid = contract.place_bid(&investor, &invoice_id, &amount, &(amount + 100), &salt);
    assert_eq!(first_bid, Ok(()));

    // Second bid with same parameters should return DuplicateBid error
    let duplicate_bid = contract.place_bid(&investor, &invoice_id, &amount, &(amount + 100), &salt);
    assert_eq!(duplicate_bid, Err(ContractError::DuplicateBid));
}
