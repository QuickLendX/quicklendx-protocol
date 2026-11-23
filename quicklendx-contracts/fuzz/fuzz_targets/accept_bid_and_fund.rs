//! Structured operation-sequence fuzzing for bid acceptance and funding.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, BytesN, Env};

#[derive(Arbitrary, Debug)]
struct Input {
    operations: Vec<Operation>,
}

#[derive(Arbitrary, Debug)]
enum Operation {
    Accept { invoice: [u8; 32], bid: [u8; 32] },
    AcceptWithKey { invoice: [u8; 32], bid: [u8; 32], key: [u8; 32] },
    ReadBid { bid: [u8; 32] },
    ReadRanked { invoice: [u8; 32] },
}

fuzz_target!(|input: Input| {
    // Bound work per input so a single corpus entry cannot create an unbounded
    // number of host calls or storage records.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    for operation in input.operations.into_iter().take(16) {
        match operation {
            Operation::Accept { invoice, bid } => {
                let invoice = BytesN::from_array(&env, &invoice);
                let bid = BytesN::from_array(&env, &bid);
                // Invalid/missing state is expected to return a contract error.
                let _ = client.try_accept_bid_and_fund(&invoice, &bid);
            }
            Operation::AcceptWithKey { invoice, bid, key } => {
                let invoice = BytesN::from_array(&env, &invoice);
                let bid = BytesN::from_array(&env, &bid);
                let key = BytesN::from_array(&env, &key);
                let _ = client.try_accept_bid_and_fund_with_key(&invoice, &bid, &key);
            }
            Operation::ReadBid { bid } => {
                let bid = BytesN::from_array(&env, &bid);
                let _ = client.get_bid(&bid);
            }
            Operation::ReadRanked { invoice } => {
                let invoice = BytesN::from_array(&env, &invoice);
                let _ = client.get_ranked_bids(&invoice);
            }
        }
    }
});
