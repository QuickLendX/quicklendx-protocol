use super::*;
use crate::defaults::{
    resolve_grace_period, scan_funded_invoice_expirations, DEFAULT_GRACE_PERIOD,
    DEFAULT_OVERDUE_SCAN_BATCH_LIMIT,
};
const MAX_GRACE_PERIOD: u64 = 30 * 24 * 60 * 60;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// --- Helpers copied from test_overdue_expiration.rs / test_default_finality_matrix.rs ---

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
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
    _admin: &Address,
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
    let bid_id = client.place_bid(investor, &invoice_id, &amount, &(amount + 100), &BytesN::from_array(&env, &[0u8; 32]));
    client.accept_bid(&invoice_id, &bid_id);
    invoice_id
}

// --- Test Cases ---

#[test]
fn test_resolve_grace_period_clamping() {
    let (env, client, admin) = setup();

    env.as_contract(&client.address, || {
        // 1. None: should use DEFAULT_GRACE_PERIOD (7 days)
        let resolved_none = resolve_grace_period(&env, None).unwrap();
        assert_eq!(resolved_none, DEFAULT_GRACE_PERIOD);

        // 2. Custom valid value (e.g. 5 days)
        let valid_custom = 5 * 24 * 60 * 60;
        let resolved_custom = resolve_grace_period(&env, Some(valid_custom)).unwrap();
        assert_eq!(resolved_custom, valid_custom);

        // 3. Custom zero value (allowed for immediate defaults)
        let resolved_zero = resolve_grace_period(&env, Some(0)).unwrap();
        assert_eq!(resolved_zero, 0);

        // 4. Custom invalid value exceeding MAX_GRACE_PERIOD (30 days)
        let invalid_custom = MAX_GRACE_PERIOD + 1;
        let resolved_invalid = resolve_grace_period(&env, Some(invalid_custom));
        assert!(matches!(
            resolved_invalid,
            Err(QuickLendXError::InvalidTimestamp)
        ));
    });
}

#[test]
fn test_grace_period_boundary_on_both_sides() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 10000);

    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id =
        create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due_date);

    let grace_period = 3 * 24 * 60 * 60; // 3 days
    let grace_deadline = due_date + grace_period;

    // 1. Exactly at grace deadline -> not defaultable
    env.ledger().set_timestamp(grace_deadline);

    // Try mark defaulted - should return OperationNotAllowed
    let res = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(matches!(res, Err(Ok(QuickLendXError::OperationNotAllowed))));

    // Scan funded invoice expirations at exactly grace deadline -> overdue count is 1 (since current_timestamp > due_date)
    // but the status remains Funded (not defaulted because current_timestamp <= grace_deadline)
    env.as_contract(&client.address, || {
        let scan_res = scan_funded_invoice_expirations(&env, grace_period, Some(10)).unwrap();
        assert_eq!(scan_res.overdue_count, 1);
        assert_eq!(scan_res.scanned_count, 1);
    });

    let status = client.get_invoice(&invoice_id).status;
    assert_eq!(status, InvoiceStatus::Funded);

    // Reset default scan cursor back to 0 for next check
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&soroban_sdk::symbol_short!("ovd_scan"), &0u32);
    });

    // 2. Exactly one second past grace deadline -> defaultable
    env.ledger().set_timestamp(grace_deadline + 1);

    // Scan funded invoice expirations one second past -> should trigger default handling (check_and_handle_expiration)
    env.as_contract(&client.address, || {
        let scan_res = scan_funded_invoice_expirations(&env, grace_period, Some(10)).unwrap();
        // The invoice is still counted as overdue in this scan step
        assert_eq!(scan_res.overdue_count, 1);
    });

    // Now it should be transitioned to Defaulted
    let status_after = client.get_invoice(&invoice_id).status;
    assert_eq!(status_after, InvoiceStatus::Defaulted);
}

#[test]
fn test_scan_funded_invoice_expirations_cursor_paging_boundary() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let investor = create_verified_investor(&env, &client, &admin, 50000);

    let base_time = env.ledger().timestamp();
    let due1 = base_time + 100;
    let due2 = base_time + 100;
    let due3 = base_time + 100;
    let due4 = base_time + 100;

    let inv1 = create_and_fund_invoice(&env, &client, &admin, &business, &investor, 1000, due1);
    let inv2 = create_and_fund_invoice(&env, &client, &admin, &business, &investor, 2000, due2);
    let inv3 = create_and_fund_invoice(&env, &client, &admin, &business, &investor, 3000, due3);
    let inv4 = create_and_fund_invoice(&env, &client, &admin, &business, &investor, 4000, due4);

    let grace_period = 10u64;
    let grace_deadline = due2 + grace_period;

    // Set time exactly at grace_deadline (so no invoice is defaultable yet, but all are overdue)
    env.ledger().set_timestamp(grace_deadline);

    // Scan page 1 with batch limit of 2 (processes inv1 and inv2)
    env.as_contract(&client.address, || {
        let res1 = scan_funded_invoice_expirations(&env, grace_period, Some(2)).unwrap();
        assert_eq!(res1.scanned_count, 2);
        assert_eq!(res1.overdue_count, 2);
        assert_eq!(res1.next_cursor, 2);
    });

    // Verify next cursor is indeed 2
    assert_eq!(client.get_overdue_scan_cursor(), 2);

    // Scan page 2 with batch limit of 2 (processes inv3 and inv4)
    env.as_contract(&client.address, || {
        let res2 = scan_funded_invoice_expirations(&env, grace_period, Some(2)).unwrap();
        assert_eq!(res2.scanned_count, 2);
        assert_eq!(res2.overdue_count, 2);
        // Since we processed all 4, cursor wraps to 0
        assert_eq!(res2.next_cursor, 0);
    });

    assert_eq!(client.get_overdue_scan_cursor(), 0);

    // Finding/Observation: When invoices are defaulted and removed from the Funded list during scanning,
    // the dynamic change in the underlying Funded list length causes cursor skipping because the cursor index
    // is applied to a mutated list. If the boundary invoice is the last on a page and defaults, it gets removed,
    // shifting subsequent items left, which results in skipping the next item during the next batch scan.
}
