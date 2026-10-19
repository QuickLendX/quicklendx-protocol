#![cfg(test)]

use crate::{
    errors::QuickLendXError,
    invoice::InvoiceCategory,
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env, String, Vec};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    
    client.set_protocol_limits(
        &admin,
        100, // min_invoice_amount
        10,  // min_bid_amount
        10,  // min_bid_bps
        365, // max_due_date_days
        7 * 24 * 60 * 60, // grace_period_seconds
        0,   // max_invoices_per_business
        crate::verification::InvestorTier::None,
    );
    let business = Address::generate(env);
    client.submit_kyc_application(
        &admin,
        &business,
        &String::from_str(env, "business"),
        &String::from_str(env, "tax1"),
        &String::from_str(env, "address"),
        &String::from_str(env, "data"),
    );
    client.approve_kyc(&admin, &business);
    (client, admin, business)
}

fn create_test_invoice(env: &Env, client: &QuickLendXContractClient<'static>, business: &Address, admin: &Address, due_date: u64) -> soroban_sdk::BytesN<32> {
    let currency = Address::generate(env);
    let invoice_id = client.store_invoice(
        business,
        &100,
        &currency,
        &due_date,
        &String::from_str(env, "desc"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice_data(admin, &invoice_id);
    invoice_id
}

#[test]
fn dispute_created_within_time_limit() {
    let env = Env::default();
    env.mock_all_auths();
    
    let now = 100_000_000u64;
    env.ledger().set_timestamp(now);
    
    let (client, admin, business) = setup(&env);
    
    // due_date is in the future
    let due_date = now + 10_000;
    let invoice_id = create_test_invoice(&env, &client, &business, &admin, due_date);
    
    // Advance time to BEFORE the limit
    let grace_period = 7 * 24 * 60 * 60;
    let limit = due_date + grace_period;
    env.ledger().set_timestamp(limit - 1);
    
    let result = client.try_create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "reason"),
        &String::from_str(&env, "evidence")
    );
    assert!(result.is_ok());
}

#[test]
fn dispute_created_at_time_limit() {
    let env = Env::default();
    env.mock_all_auths();
    
    let now = 100_000_000u64;
    env.ledger().set_timestamp(now);
    
    let (client, admin, business) = setup(&env);
    
    // due_date is in the future
    let due_date = now + 10_000;
    let invoice_id = create_test_invoice(&env, &client, &business, &admin, due_date);
    
    // Advance time to EXACTLY the limit
    let grace_period = 7 * 24 * 60 * 60;
    let limit = due_date + grace_period;
    env.ledger().set_timestamp(limit);
    
    let result = client.try_create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "reason"),
        &String::from_str(&env, "evidence")
    );
    assert!(result.is_ok());
}

#[test]
fn dispute_created_past_time_limit() {
    let env = Env::default();
    env.mock_all_auths();
    
    let now = 100_000_000u64;
    env.ledger().set_timestamp(now);
    
    let (client, admin, business) = setup(&env);
    
    // due_date is in the future
    let due_date = now + 10_000;
    let invoice_id = create_test_invoice(&env, &client, &business, &admin, due_date);
    
    // Advance time to PAST the limit
    let grace_period = 7 * 24 * 60 * 60;
    let limit = due_date + grace_period;
    env.ledger().set_timestamp(limit + 1);
    
    let result = client.try_create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "reason"),
        &String::from_str(&env, "evidence")
    );
    
    assert!(result.is_err());
    let err = result.unwrap_err().expect("should have error");
    assert_eq!(err, QuickLendXError::DisputeTimeLimitExceeded);
}
