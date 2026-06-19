#![cfg(test)]

//! Pause-state coverage tests
//! Asserts that read-only query entrypoints remain callable while the contract is paused,
//! enabling observability during incidents, while mutating entrypoints remain blocked.

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, String, Vec};

fn setup_and_fund_invoice(env: &Env) -> (QuickLendXContractClient<'static>, Address, soroban_sdk::BytesN<32>, soroban_sdk::BytesN<32>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let business = Address::generate(env);
    let investor = Address::generate(env);
    client.initialize_admin(&admin);

    let token_admin = Address::generate(env);
    let currency = env.register_stellar_asset_contract_v2(token_admin).address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(&business, &20_000i128);
    sac.mint(&investor, &15_000i128);
    sac.mint(&contract_id, &1i128);
    let exp = env.ledger().sequence() + 100_000;
    tok.approve(&business, &contract_id, &20_000i128, &exp);
    tok.approve(&investor, &contract_id, &15_000i128, &exp);
    client.add_currency(&admin, &currency);

    let due_date = env.ledger().timestamp() + 86_400;
    
    // First invoice - we will fund it to get investment and escrow
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(env, "Funded invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    
    // KYC
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &15_000i128);
    
    let bid_id = client.place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    // Second invoice - keep it active with a bid so get_best_bid works
    let invoice2_id = client.store_invoice(
        &business,
        &2_000i128,
        &currency,
        &due_date,
        &String::from_str(env, "Active invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice2_id);
    client.place_bid(&investor, &invoice2_id, &2_000i128, &2_200i128);

    (client, admin, invoice_id, invoice2_id, investor)
}

/// Assert reads succeed while paused, reads return identical values, and mutations fail.
#[test]
fn test_pause_reads_available() {
    let env = Env::default();
    let (client, admin, funded_invoice_id, active_invoice_id, investor) = setup_and_fund_invoice(&env);
    
    // 1. Gather state before pause
    let inv_before = client.get_invoice(&funded_invoice_id);
    let best_bid_before = client.get_best_bid(&active_invoice_id);
    let ranked_bids_before = client.get_ranked_bids(&active_invoice_id);
    let investment_before = client.get_invoice_investment(&funded_invoice_id);
    let escrow_before = client.get_escrow_status(&funded_invoice_id);
    let count_before = client.get_total_invoice_count();
    
    // 2. Pause the contract
    client.pause(&admin);
    assert!(client.is_paused(), "Contract must be paused");
    
    // 3. Assert mutating entrypoints fail with ContractPaused
    let place_bid_err = client.try_place_bid(&investor, &active_invoice_id, &2_000i128, &2_100i128);
    assert_eq!(place_bid_err.unwrap_err().unwrap(), QuickLendXError::ContractPaused);
    
    let dummy_bid_id = soroban_sdk::BytesN::from_array(&env, &[0; 32]);
    let accept_bid_err = client.try_accept_bid_and_fund(&active_invoice_id, &dummy_bid_id);
    assert_eq!(accept_bid_err.unwrap_err().unwrap(), QuickLendXError::ContractPaused);
    
    let settle_err = client.try_settle_invoice(&funded_invoice_id, &1_000i128);
    assert_eq!(settle_err.unwrap_err().unwrap(), QuickLendXError::ContractPaused);

    // 4. Assert read-only entrypoints remain available and return identical values
    let inv_after = client.get_invoice(&funded_invoice_id);
    assert_eq!(inv_after.amount, inv_before.amount, "get_invoice should return same state");
    assert_eq!(inv_after.status, inv_before.status);
    
    let best_bid_after = client.get_best_bid(&active_invoice_id);
    assert_eq!(best_bid_after.unwrap().amount, best_bid_before.unwrap().amount, "get_best_bid should remain accessible");
    
    let ranked_bids_after = client.get_ranked_bids(&active_invoice_id);
    assert_eq!(ranked_bids_after.len(), ranked_bids_before.len(), "get_ranked_bids should remain accessible");
    
    let investment_after = client.get_invoice_investment(&funded_invoice_id);
    assert_eq!(investment_after.amount, investment_before.amount, "get_invoice_investment should remain accessible");
    
    let escrow_after = client.get_escrow_status(&funded_invoice_id);
    assert_eq!(escrow_after.amount, escrow_before.amount, "get_escrow_status should remain accessible");

    let count_after = client.get_total_invoice_count();
    assert_eq!(count_after, count_before, "get_total_invoice_count should remain accessible");
}
