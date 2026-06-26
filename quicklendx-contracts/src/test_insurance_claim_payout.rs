#![cfg(test)]

extern crate alloc;
use super::*;
use crate::errors::QuickLendXError;
use crate::events::InsuranceClaimed;
use crate::investment::{InvestmentStatus, MAX_TOTAL_COVERAGE_PERCENTAGE};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, Address, BytesN, Env, Map, String, Symbol, TryFromVal, Val, Vec,
};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (client, admin, contract_id)
}

fn mint_currency(
    env: &Env,
    contract_id: &Address,
    biz: &Address,
    investor: Option<&Address>,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    let bal = 10_000_000i128;
    sac.mint(biz, &bal);
    sac.mint(contract_id, &1i128);

    // add currency to whitelist
    let admin = Address::generate(env); // we mock auths so this is fine

    if let Some(inv) = investor {
        sac.mint(inv, &bal);
        let exp = env.ledger().sequence() + 1_000;
        tok.approve(biz, contract_id, &bal, &exp);
        tok.approve(inv, contract_id, &bal, &exp);
    }
    currency
}

fn create_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    client.verify_business(admin, &business);
    business
}

fn create_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC data"));
    client.verify_investor(&investor, &limit);
    investor
}

fn create_and_fund_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    investor: &Address,
    amount: i128,
    due_date: u64,
) -> BytesN<32> {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let token_client = token::Client::new(env, &currency);

    client.add_currency(admin, &currency);

    sac_client.mint(investor, &amount);
    let expiry = env.ledger().sequence() + 10_000;
    token_client.approve(investor, &client.address, &amount, &expiry);

    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(investor, &invoice_id, &amount, &(amount + 100));
    client.accept_bid(&invoice_id, &bid_id);

    invoice_id
}

/// Helper to get the latest emitted `InsuranceClaimed` payload
fn latest_insurance_claimed_payload(env: &Env) -> InsuranceClaimed {
    use soroban_sdk::xdr;
    let topic_str = "insurance_claimed";
    let topic_sym = Symbol::new(env, topic_str);
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym).expect("topic to ScVal");
    let all = env.events().all();
    for e in all.events().iter().rev() {
        if let xdr::ContractEventBody::V0(body) = &e.body {
            if body.topics.first() == Some(&topic_xdr) {
                let data_val = Val::try_from_val(env, &body.data).expect("data ScVal to Val");
                return InsuranceClaimed::try_from_val(env, &data_val)
                    .expect("failed to decode event payload");
            }
        }
    }
    panic!(
        "topic {:?} not found in {} events",
        topic_str,
        all.events().len()
    );
}

fn count_insurance_claimed_events(env: &Env) -> usize {
    use soroban_sdk::xdr;
    let topic_str = "insurance_claimed";
    let topic_sym = Symbol::new(env, topic_str);
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym).expect("topic to ScVal");
    env.events()
        .all()
        .events()
        .iter()
        .filter(|e| match &e.body {
            xdr::ContractEventBody::V0(body) => body.topics.first() == Some(&topic_xdr),
        })
        .count()
}

#[test]
fn test_default_triggers_correct_per_provider_insurance_claim() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);

    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let investment = client.get_invoice_investment(&invoice_id);
    let investment_id = investment.investment_id;

    // Add insurance
    let provider = Address::generate(&env);
    let coverage_percentage = 50u32;
    client.add_investment_insurance(&investment_id, &provider, &coverage_percentage);

    let records_before = client.query_investment_insurance(&investment_id);
    assert!(records_before.get(0).unwrap().active);

    // Move time past due date + grace period
    let grace_period = 7 * 24 * 60 * 60; // 7 days
    let default_time = due_date + grace_period + 1;
    env.ledger().set_timestamp(default_time);

    // Default the invoice
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    // Assert investment defaulted
    let post_investment = client.get_invoice_investment(&invoice_id);
    assert_eq!(post_investment.status, InvestmentStatus::Defaulted);

    // Assert coverage deactivated
    let records = client.query_investment_insurance(&investment_id);
    assert_eq!(records.len(), 1);
    assert!(!records.get(0).unwrap().active);

    // Assert active coverage percentage is zero
    assert_eq!(post_investment.total_active_coverage_percentage(), 0);

    // Assert exactly 1 claim event was fired
    assert_eq!(count_insurance_claimed_events(&env), 1);
    let payload = latest_insurance_claimed_payload(&env);

    assert_eq!(payload.investment_id, investment_id);
    assert_eq!(payload.invoice_id, invoice_id);
    assert_eq!(payload.provider, provider);
    // coverage_amount = 1000 * 50% = 500
    assert_eq!(payload.coverage_amount, 500);
}

#[test]
fn test_default_triggers_stacked_insurance_claims_correctly() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);

    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let investment = client.get_invoice_investment(&invoice_id);
    let investment_id = investment.investment_id;

    let provider1 = Address::generate(&env);
    let provider2 = Address::generate(&env);

    client.add_investment_insurance(&investment_id, &provider1, &30u32);
    client.add_investment_insurance(&investment_id, &provider2, &70u32);

    let records_before = client.query_investment_insurance(&investment_id);
    assert_eq!(records_before.len(), 2);

    // Move time past due date + grace period
    let grace_period = 7 * 24 * 60 * 60; // 7 days
    let default_time = due_date + grace_period + 1;
    env.ledger().set_timestamp(default_time);

    // Record count before default
    let count_before = count_insurance_claimed_events(&env);

    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    // Assert coverage deactivated exactly once for both
    let records = client.query_investment_insurance(&investment_id);
    assert!(!records.get(0).unwrap().active);
    assert!(!records.get(1).unwrap().active);

    let post_investment = client.get_invoice_investment(&invoice_id);
    assert_eq!(post_investment.total_active_coverage_percentage(), 0);

    let count_after = count_insurance_claimed_events(&env);
    assert_eq!(count_after - count_before, 2);

    // Ensure total claim amounts match what was set
    // Note: iterating backwards over events to check all payloads is harder with the helper,
    // but we know 2 claims were emitted for the providers
    let mut claims = Vec::new(&env);
    use soroban_sdk::xdr;
    let topic_str = "insurance_claimed";
    let topic_sym = Symbol::new(&env, topic_str);
    let topic_xdr = xdr::ScVal::try_from_val(&env, &topic_sym).expect("topic to ScVal");
    for e in env.events().all().events().iter() {
        if let xdr::ContractEventBody::V0(body) = &e.body {
            if body.topics.first() == Some(&topic_xdr) {
                let data_val = Val::try_from_val(&env, &body.data).expect("data ScVal to Val");
                let pl = InsuranceClaimed::try_from_val(&env, &data_val).unwrap();
                claims.push_back(pl);
            }
        }
    }

    assert_eq!(claims.len(), 2);
    let mut found_p1 = false;
    let mut found_p2 = false;

    for claim in claims.iter() {
        if claim.provider == provider1 {
            assert_eq!(claim.coverage_amount, 300);
            found_p1 = true;
        } else if claim.provider == provider2 {
            assert_eq!(claim.coverage_amount, 700);
            found_p2 = true;
        }
    }

    assert!(
        found_p1 && found_p2,
        "both providers should have claimed events"
    );
}

#[test]
fn test_default_uninsured_investment_claims_nothing() {
    let env = Env::default();
    let (client, admin, _) = setup(&env);

    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    // Do not add insurance
    let count_before = count_insurance_claimed_events(&env);

    let grace_period = 7 * 24 * 60 * 60; // 7 days
    let default_time = due_date + grace_period + 1;
    env.ledger().set_timestamp(default_time);

    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    let count_after = count_insurance_claimed_events(&env);
    assert_eq!(
        count_before, count_after,
        "no insurance claims should be emitted"
    );
}

#[test]
fn test_already_inactive_coverage_not_reclaimed() {
    let env = Env::default();
    let (client, admin, contract_id) = setup(&env);

    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = create_and_fund_invoice(
        &env, &client, &admin, &business, &investor, amount, due_date,
    );

    let investment = client.get_invoice_investment(&invoice_id);
    let investment_id = investment.investment_id;

    let provider = Address::generate(&env);
    client.add_investment_insurance(&investment_id, &provider, &100u32);

    // Manually deactivate it to test
    env.as_contract(&contract_id, || {
        let mut inv =
            crate::investment::InvestmentStorage::get_investment(&env, &investment_id).unwrap();
        let mut cov = inv.insurance.get(0).unwrap();
        cov.active = false;
        inv.insurance.set(0, cov);
        crate::investment::InvestmentStorage::update_investment(&env, &inv);
    });

    let grace_period = 7 * 24 * 60 * 60; // 7 days
    let default_time = due_date + grace_period + 1;
    env.ledger().set_timestamp(default_time);

    let count_before = count_insurance_claimed_events(&env);
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    let count_after = count_insurance_claimed_events(&env);

    assert_eq!(
        count_after, count_before,
        "inactive coverage should not trigger claim"
    );
}
