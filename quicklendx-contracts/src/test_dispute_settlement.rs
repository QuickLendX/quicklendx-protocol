#![cfg(test)]

use crate::contract::{QuickLendXContract, QuickLendXContractClient};
use crate::errors::QuickLendXError;
use crate::types::{DisputeStatus, InvoiceCategory, InvoiceStatus};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, String, BytesN};

fn create_token_contract<'a>(
    env: &Env,
    admin: &Address,
) -> (token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract_address = env.register_stellar_asset_contract(admin.clone());
    (
        token::Client::new(env, &contract_address),
        token::StellarAssetClient::new(env, &contract_address),
    )
}

fn setup_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    investor: &Address,
    admin: &Address,
    amount: i128,
) -> (BytesN<32>, Address) {
    let (token_client, token_admin) = create_token_contract(env, admin);
    let currency = token_client.address.clone();

    token_admin.mint(investor, &(amount * 2));

    let invoice_id = client.create_invoice(
        business,
        &amount,
        &currency,
        &(env.ledger().timestamp() + 86400),
        &String::from_str(env, "Test invoice for dispute settlement test"),
        &InvoiceCategory::Services,
    );

    client.verify_invoice(admin, &invoice_id);

    let bid_id = client.place_bid(&invoice_id, investor, &amount, &(amount + 1000));
    client.accept_bid(business, &invoice_id, &bid_id);

    (invoice_id, currency)
}

#[test]
fn test_settle_invoice_blocks_when_dispute_is_open() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let amount: i128 = 100_000;
    let (invoice_id, _currency) =
        setup_funded_invoice(&env, &client, &business, &investor, &admin, amount);

    // Business opens dispute
    client.create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "dispute reason"),
        &String::from_str(&env, "evidence description"),
    );

    // Verify dispute status is Disputed (Open dispute)
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.dispute_status, DisputeStatus::Disputed);

    // Settle invoice should be BLOCKED (returns InvalidStatus)
    let result = client.try_settle_invoice(&invoice_id, &amount);
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);

    // Advance to UnderReview
    client.put_dispute_under_review(&admin, &invoice_id);
    let invoice_review = client.get_invoice(&invoice_id);
    assert_eq!(invoice_review.dispute_status, DisputeStatus::UnderReview);

    // Settle invoice should STILL be BLOCKED under review
    let result_review = client.try_settle_invoice(&invoice_id, &amount);
    assert_eq!(result_review.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);
}

#[test]
fn test_settle_invoice_allows_when_dispute_is_resolved() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let amount: i128 = 100_000;
    let (invoice_id, currency) =
        setup_funded_invoice(&env, &client, &business, &investor, &admin, amount);

    // Business opens dispute
    client.create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "dispute reason"),
        &String::from_str(&env, "evidence description"),
    );

    // Review dispute
    client.put_dispute_under_review(&admin, &invoice_id);

    // Resolve dispute in favor of business (DisputeStatus becomes Resolved)
    client.resolve_dispute(&admin, &invoice_id, &String::from_str(&env, "dispute resolved"));
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.dispute_status, DisputeStatus::Resolved);

    // Mint token allowance/balance for business to make payment
    let token_client = token::Client::new(&env, &currency);
    let token_admin = token::StellarAssetClient::new(&env, &currency);
    token_admin.mint(&business, &amount);
    token_client.approve(&business, &contract_id, &amount, &(env.ledger().timestamp() + 1000));

    // Settle invoice should SUCCEED
    let result = client.try_settle_invoice(&invoice_id, &amount);
    assert!(result.is_ok());

    let final_invoice = client.get_invoice(&invoice_id);
    assert_eq!(final_invoice.status, InvoiceStatus::Paid);
}

#[test]
fn test_settle_invoice_allows_when_no_dispute_exists() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let amount: i128 = 100_000;
    let (invoice_id, currency) =
        setup_funded_invoice(&env, &client, &business, &investor, &admin, amount);

    // Verify dispute status is None (No dispute)
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.dispute_status, DisputeStatus::None);

    // Mint token allowance/balance for business to make payment
    let token_client = token::Client::new(&env, &currency);
    let token_admin = token::StellarAssetClient::new(&env, &currency);
    token_admin.mint(&business, &amount);
    token_client.approve(&business, &contract_id, &amount, &(env.ledger().timestamp() + 1000));

    // Settle invoice should SUCCEED
    let result = client.try_settle_invoice(&invoice_id, &amount);
    assert!(result.is_ok());

    let final_invoice = client.get_invoice(&invoice_id);
    assert_eq!(final_invoice.status, InvoiceStatus::Paid);
}
