//! Exercise successful and rejected bid acceptance against a real fixture.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use quicklendx_contracts::{invoice::InvoiceCategory, QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

#[derive(Arbitrary, Debug)]
struct Input {
    amount: u16,
    operations: std::vec::Vec<Operation>,
}

#[derive(Arbitrary, Debug)]
enum Operation {
    Accept,
    AcceptAgain,
    ReadBid,
    ReadInvoiceBids,
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

    // Keep values inside a small, positive range accepted by the contract.
    let amount = 1_000i128 + (input.amount as i128 % 10_000);
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
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "fuzz invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &amount,
        &(amount + 100),
        &BytesN::from_array(&env, &[7u8; 32]),
    );

    // Use the fallible client entrypoint: contract-level rejections must be
    // treated as fuzz outcomes, not as panics from the generated client.
    let first_accept = client.try_accept_bid(&invoice_id, &bid_id);
    let funded = matches!(first_accept, Ok(Ok(_)));

    // A successful acceptance must create one consistent funded state. A
    // rejected/failed call must leave the verified invoice and investor funds
    // untouched; this also catches partial state changes.
    if funded {
        let investment = client.get_investment(&invoice_id)
            .expect("successful acceptance must create an investment");
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.funded_amount, investment.amount);
        assert!(matches!(client.try_get_escrow_details(&invoice_id), Ok(Ok(_))));
        assert_eq!(token_client.balance(&contract_id), investment.amount);
    } else {
        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.funded_amount, 0);
        assert!(invoice.investor.is_none());
        assert!(client.get_investment(&invoice_id).is_none());
        assert!(!matches!(client.try_get_escrow_details(&invoice_id), Ok(Ok(_))));
        assert_eq!(token_client.balance(&contract_id), 0);
    }

    for operation in input.operations.into_iter().take(16) {
        match operation {
            Operation::Accept | Operation::AcceptAgain => {
                let _ = client.try_accept_bid(&invoice_id, &bid_id);
            }
            Operation::ReadBid => {
                let _ = client.get_bid(&bid_id);
            }
            Operation::ReadInvoiceBids => {
                let _ = client.get_bids_for_invoice(&invoice_id);
            }
        }

        // Once acceptance succeeds, the invoice must be funded and an
        // investment record must exist. Repeated acceptance must not panic.
        if let Some(investment) = client.get_investment(&invoice_id) {
            let invoice = client.get_invoice(&invoice_id);
            assert_eq!(invoice.funded_amount, investment.amount);
            assert!(client.try_get_escrow_details(&invoice_id).is_ok());
        }
    }
});
