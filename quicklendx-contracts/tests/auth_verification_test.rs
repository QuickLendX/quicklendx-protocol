#![cfg(test)]

extern crate std; 

use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction},
    Address, BytesN, Env, IntoVal, String, Symbol, Vec
};

use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient, InvoiceCategory};

proptest! {
    #[test]
    fn asserts_single_auth_count_when_user_places_bid(
        bid_amount in 1..50_000i128,
        expected_return in 50_000..100_000i128
    ) {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        let investor = Address::generate(&env);
        let business = Address::generate(&env);
        let currency = Address::generate(&env);
        let salt = BytesN::from_array(&env, &[0u8; 32]);
        
        env.mock_all_auths();
        
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        
        client.initialize_protocol_limits(&admin, &10, &30, &86400);

        let due_date = env.ledger().timestamp() + 86400;
        let invoice_amount = 100_000i128;
        
        let invoice_id = client.upload_invoice(
            &business,
            &invoice_amount,
            &currency,
            &due_date,
            &String::from_str(&env, "Test Invoice"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );

        client.verify_invoice(&invoice_id);
        client.verify_investor(&investor, &100_000i128);

        client.place_bid(&investor, &invoice_id, &bid_amount, &expected_return, &salt);

        let auths = env.auths();
        
        let last_auth = auths.last().expect("Should have at least one auth");
        let (auth_address, invocation) = last_auth;

        assert_eq!(auth_address, &investor, "The auth address must match the investor");
        
        assert_eq!(
            invocation.function,
            AuthorizedFunction::Contract((
                contract_id.clone(),
                Symbol::new(&env, "place_bid"),
                (&investor, invoice_id, bid_amount, expected_return, salt).into_val(&env),
            )),
            "Auth payload does not match expected place_bid function boundaries"
        );
    }
}

#[test]
fn fails_when_authorization_is_missing_for_protected_entrypoint() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let rogue_user = Address::generate(&env);
    
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    let salt = BytesN::from_array(&env, &[0u8; 32]);
    let bid_amount = 1000i128;
    let expected_return = 5000i128;
    
    let result = client.try_place_bid(&rogue_user, &invoice_id, &bid_amount, &expected_return, &salt);
    
    assert!(
        result.is_err(),
        "Contract must trap/fail when an operation requiring auth() is invoked without proper authorization"
    );
}