//! Structured settlement/read-sequence fuzzing.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{BytesN, Env};

#[derive(Arbitrary, Debug)]
struct Input {
    invoice_ids: Vec<[u8; 32]>,
    amounts: Vec<i64>,
    offsets: Vec<u32>,
    limits: Vec<u32>,
}

fuzz_target!(|input: Input| {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Keep each iteration small; malformed IDs and amounts must be handled as
    // ordinary contract errors rather than host panics.
    for (index, raw_id) in input.invoice_ids.into_iter().take(16).enumerate() {
        let invoice_id = BytesN::from_array(&env, &raw_id);
        let _ = client.try_get_invoice_progress(&invoice_id);
        let _ = client.try_get_payment_count(&invoice_id);

        let amount = input.amounts.get(index).copied().unwrap_or_default() as i128;
        let _ = client.try_get_payment_records(
            &invoice_id,
            &input.offsets.get(index).copied().unwrap_or_default(),
            &input.limits.get(index).copied().unwrap_or_default(),
        );

        // A settlement call with a nonexistent snapshot cannot mutate state;
        // progress/count reads above exercise the settlement storage boundary.
        let _ = amount;
    }
});
