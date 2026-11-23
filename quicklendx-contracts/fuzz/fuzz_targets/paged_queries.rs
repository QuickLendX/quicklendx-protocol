//! Fuzz pagination offsets, limits and invoice identifiers.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{BytesN, Env};

#[derive(Arbitrary, Debug)]
struct Input {
    queries: Vec<Query>,
}

#[derive(Arbitrary, Debug)]
struct Query {
    invoice_id: [u8; 32],
    offset: u32,
    limit: u32,
}

fuzz_target!(|input: Input| {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    for query in input.queries.into_iter().take(32) {
        let invoice_id = BytesN::from_array(&env, &query.invoice_id);
        let page = client.get_ranked_bids_paged(&invoice_id, &query.offset, &query.limit);
        assert!(page.items.len() <= 50, "paged bid query exceeded its hard cap");
        if !page.has_more {
            assert!(page.items.len() as u32 <= page.total_count);
        }
        let _ = client.try_get_payment_records(
            &invoice_id,
            &query.offset,
            &query.limit,
        );
    }
});
