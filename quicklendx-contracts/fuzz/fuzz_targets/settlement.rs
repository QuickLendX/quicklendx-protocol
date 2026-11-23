//! Exercise settlement queries and payment attempts on a funded invoice.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use quicklendx_contracts::{
    invoice::InvoiceCategory,
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

#[derive(Arbitrary, Debug)]
struct Input {
    payments: std::vec::Vec<PaymentAttempt>,
}

#[derive(Arbitrary, Debug)]
struct PaymentAttempt {
    amount: i32,
    full_settlement: bool,
    partial_payment: bool,
    stale_snapshot: bool,
}

fuzz_target!(|input: Input| {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let currency = env.register_stellar_asset_contract_v2(token_admin).address();
    let token_client = token::Client::new(&env, &currency);
    let token_admin_client = token::StellarAssetClient::new(&env, &currency);

    let invoice_amount = 10_000i128;
    let balance = 100_000i128;
    token_admin_client.mint(&business, &balance);
    token_admin_client.mint(&investor, &balance);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &balance, &expiration);
    token_client.approve(&investor, &contract_id, &balance, &expiration);

    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "fuzz-business"));
    client.verify_business(&admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "fuzz-investor"));
    client.verify_investor(&investor, &balance);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &invoice_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "fuzz settlement invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &invoice_amount,
        &(invoice_amount + 500),
        &BytesN::from_array(&env, &[9u8; 32]),
    );
    let funding = client.try_accept_bid(&invoice_id, &bid_id);
    assert!(matches!(funding, Ok(Ok(_))), "funding fixture must succeed");

    for (index, attempt) in input.payments.into_iter().take(16).enumerate() {
        let Some(mut snapshot) = client.get_investment(&invoice_id) else {
            panic!("accepted bid must create an investment");
        };
        if attempt.stale_snapshot {
            snapshot.amount = snapshot.amount.saturating_add(1);
        }
        if attempt.partial_payment {
            let payment = 1 + (attempt.amount.unsigned_abs() as i128 % (invoice_amount - 1));
            // The contract requires a 64-character hexadecimal transaction
            // nonce and rejects re-use, so derive a unique nonce per attempt.
            let nonce = String::from_str(&env, &format!("{:064x}", index + 1));
            let _ = client.try_process_partial_payment(&invoice_id, &payment, &nonce);
        } else {
            let payment = if attempt.full_settlement {
                client.get_invoice_progress(&invoice_id).remaining_due
            } else {
                1 + (attempt.amount.unsigned_abs() as i128 % (invoice_amount - 1))
            };
            let _ = client.try_settle_invoice(&invoice_id, &payment, &snapshot);
        }

        // Query endpoints must remain usable after both successful and
        // rejected payment attempts, and page size must remain bounded.
        if let Ok(Ok(progress)) = client.try_get_invoice_progress(&invoice_id) {
            assert!(progress.total_paid >= 0);
            assert!(progress.total_paid <= progress.total_due);
            assert!(progress.remaining_due >= 0);
            assert_eq!(progress.total_paid + progress.remaining_due, progress.total_due);
        }
        let _ = client.try_get_payment_count(&invoice_id);
        let records = client.try_get_payment_records(&invoice_id, &0u32, &u32::MAX);
        if let Ok(Ok(records)) = records {
            assert!(records.len() <= 50);
        }
    }
});
