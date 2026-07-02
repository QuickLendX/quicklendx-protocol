//! Regression coverage for currency whitelist removals while funds are held in escrow.

use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::payments::EscrowStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, String, Vec,
};

fn setup_env() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    client.set_admin(&admin);

    (env, client, admin, contract_id)
}

fn verified_business(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn verified_investor(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

fn funded_currency(env: &Env, investor: &Address, contract_id: &Address) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    let balance = 100_000i128;
    sac.mint(investor, &balance);
    tok.approve(
        investor,
        contract_id,
        &balance,
        &(env.ledger().sequence() + 10_000),
    );
    currency
}

fn one_currency(env: &Env, currency: &Address) -> Vec<Address> {
    let mut currencies = Vec::new(env);
    currencies.push_back(currency.clone());
    currencies
}

#[test]
fn remove_currency_rejects_active_held_escrow_and_allows_after_refund() {
    let (env, client, admin, contract_id) = setup_env();
    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, 100_000);
    let currency = funded_currency(&env, &investor, &contract_id);
    let other_currency = Address::generate(&env);
    let amount = 5_000i128;

    client.add_currency(&admin, &currency);
    client.add_currency(&admin, &other_currency);

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Active escrow currency removal regression"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &amount,
        &(amount + 500),
        &BytesN::from_array(&env, &[9; 32]),
    );
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
    assert_eq!(
        client.get_total_locked_escrow(&one_currency(&env, &currency), &1),
        amount
    );

    let err = client
        .try_remove_currency(&admin, &currency)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::InvalidStatus);
    assert!(client.is_allowed_currency(&currency));

    client.refund_escrow_funds(&invoice_id, &business);
    assert_eq!(
        client.get_total_locked_escrow(&one_currency(&env, &currency), &1),
        0
    );

    client.remove_currency(&admin, &currency);
    assert!(!client.is_allowed_currency(&currency));
    assert!(client.is_allowed_currency(&other_currency));
}
