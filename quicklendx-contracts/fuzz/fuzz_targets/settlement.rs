//! Structured settlement operation-sequence fuzzing.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use quicklendx_contracts::{
    types::{Investment, InvestmentStatus},
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

#[derive(Arbitrary, Debug)]
struct Input {
    operations: Vec<Operation>,
}

#[derive(Arbitrary, Debug)]
enum Operation {
    ReadProgress { invoice: [u8; 32] },
    ReadPayments { invoice: [u8; 32], offset: u32, limit: u32 },
    AttemptSettlement { invoice: [u8; 32], investment: [u8; 32], amount: i64 },
}

fuzz_target!(|input: Input| {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let investor = Address::generate(&env);

    for operation in input.operations.into_iter().take(16) {
        match operation {
            Operation::ReadProgress { invoice } => {
                let invoice = BytesN::from_array(&env, &invoice);
                let _ = client.try_get_invoice_progress(&invoice);
                let _ = client.try_get_payment_count(&invoice);
            }
            Operation::ReadPayments { invoice, offset, limit } => {
                let invoice = BytesN::from_array(&env, &invoice);
                let _ = client.try_get_payment_records(&invoice, &offset, &limit);
            }
            Operation::AttemptSettlement { invoice, investment, amount } => {
                let invoice_id = BytesN::from_array(&env, &invoice);
                let snapshot = Investment {
                    investment_id: BytesN::from_array(&env, &investment),
                    invoice_id: invoice_id.clone(),
                    investor: investor.clone(),
                    amount: amount as i128,
                    funded_at: 0,
                    status: InvestmentStatus::Active,
                    insurance: Vec::new(&env),
                };
                // A missing/stale snapshot must be rejected cleanly without
                // mutating payment or escrow state.
                let _ = client.try_settle_invoice(&invoice_id, &(amount as i128), &snapshot);
            }
        }
    }
});
