//! Structured operation-sequence fuzzing for the public bid-acceptance flow.
//!
//! The contract exposes bid acceptance as `accept_bid`; that entry point
//! delegates to the internal `escrow::accept_bid_and_fund` implementation.
//! Keeping the fuzzer on the public entry point exercises the same guarded
//! production path without reaching across the crate's visibility boundary.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{BytesN, Env};

#[derive(Arbitrary, Debug)]
struct Input {
    operations: Vec<Operation>,
}

#[derive(Arbitrary, Debug)]
enum Operation {
    Accept { invoice: [u8; 32], bid: [u8; 32] },
    ReadBid { bid: [u8; 32] },
    ReadInvoiceBids { invoice: [u8; 32] },
}

fuzz_target!(|input: Input| {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Bound each input's operation sequence to keep fuzz iterations cheap.
    for operation in input.operations.into_iter().take(16) {
        match operation {
            Operation::Accept { invoice, bid } => {
                let invoice = BytesN::from_array(&env, &invoice);
                let bid = BytesN::from_array(&env, &bid);
                // Missing or invalid invoice/bid state should be a contract
                // error, not a host panic.
                let _ = client.try_accept_bid(&invoice, &bid);
            }
            Operation::ReadBid { bid } => {
                let bid = BytesN::from_array(&env, &bid);
                let _ = client.get_bid(&bid);
            }
            Operation::ReadInvoiceBids { invoice } => {
                let invoice = BytesN::from_array(&env, &invoice);
                let _ = client.get_bids_for_invoice(&invoice);
            }
        }
    }
});
