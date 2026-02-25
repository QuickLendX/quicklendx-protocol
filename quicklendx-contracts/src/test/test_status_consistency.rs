use super::*;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::Address as _,
    token, Address, BytesN, Env, String, Vec,
};

fn setup_env_and_client() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

fn create_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    amount: i128,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86400;
    client.store_invoice(
        business,
        &amount,
        currency,
        &due_date,
        &String::from_str(env, "Status consistency test"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

/// Assert that status list lengths match count_by_status and no orphaned IDs exist.
fn assert_status_consistency(
    env: &Env,
    client: &QuickLendXContractClient,
    expected_counts: &[(InvoiceStatus, u32)],
) {
    let mut sum = 0u32;
    for (status, expected) in expected_counts {
        let list = client.get_invoices_by_status(status);
        let count = client.get_invoice_count_by_status(status);
        assert_eq!(
            list.len() as u32, *expected,
            "status list length mismatch for {:?}",
            status
        );
        assert_eq!(
            count, *expected,
            "count_by_status mismatch for {:?}",
            status
        );
        assert_eq!(
            list.len() as u32, count,
            "list length != count for {:?}",
            status
        );
        // Verify no orphaned IDs
        for id in list.iter() {
            let invoice = client.get_invoice(&id);
            assert_eq!(
                invoice.status, *status,
                "orphaned ID in {:?} list",
                status
            );
        }
        sum += count;
    }
    let total = client.get_total_invoice_count();
    assert_eq!(total, sum, "total count mismatch");
}

#[test]
fn test_status_list_after_verify() {
    let (env, client) = setup_env_and_client();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let id = create_invoice(&env, &client, &business, &currency, 1000);

    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Pending, 1),
        (InvoiceStatus::Verified, 0),
    ]);

    client.update_invoice_status(&id, &InvoiceStatus::Verified);

    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Pending, 0),
        (InvoiceStatus::Verified, 1),
    ]);
}

#[test]
fn test_status_list_after_cancel() {
    let (env, client) = setup_env_and_client();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let id = create_invoice(&env, &client, &business, &currency, 1000);
    client.update_invoice_status(&id, &InvoiceStatus::Verified);

    client.cancel_invoice(&id);

    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Pending, 0),
        (InvoiceStatus::Verified, 0),
        (InvoiceStatus::Cancelled, 1),
    ]);
}

#[test]
fn test_status_list_after_update_invoice_status_funded() {
    let (env, client) = setup_env_and_client();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let id = create_invoice(&env, &client, &business, &currency, 1000);
    client.update_invoice_status(&id, &InvoiceStatus::Verified);
    client.update_invoice_status(&id, &InvoiceStatus::Funded);

    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Pending, 0),
        (InvoiceStatus::Verified, 0),
        (InvoiceStatus::Funded, 1),
    ]);
}

#[test]
fn test_status_list_through_full_lifecycle() {
    let (env, client) = setup_env_and_client();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let id = create_invoice(&env, &client, &business, &currency, 1000);

    // Pending -> Verified
    client.update_invoice_status(&id, &InvoiceStatus::Verified);
    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Pending, 0),
        (InvoiceStatus::Verified, 1),
    ]);

    // Verified -> Funded
    client.update_invoice_status(&id, &InvoiceStatus::Funded);
    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Verified, 0),
        (InvoiceStatus::Funded, 1),
    ]);

    // Funded -> Paid
    client.update_invoice_status(&id, &InvoiceStatus::Paid);
    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Funded, 0),
        (InvoiceStatus::Paid, 1),
    ]);
}

#[test]
fn test_status_list_no_duplicates_on_repeated_add() {
    let (env, client) = setup_env_and_client();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let id = create_invoice(&env, &client, &business, &currency, 1000);

    // Manually verify the pending list has exactly 1 entry
    let pending = client.get_invoices_by_status(&InvoiceStatus::Pending);
    assert_eq!(pending.len(), 1);

    // Update to verified and back should not create duplicates
    client.update_invoice_status(&id, &InvoiceStatus::Verified);
    let verified = client.get_invoices_by_status(&InvoiceStatus::Verified);
    assert_eq!(verified.len(), 1);
}

#[test]
fn test_status_list_multiple_invoices_mixed_transitions() {
    let (env, client) = setup_env_and_client();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let id1 = create_invoice(&env, &client, &business, &currency, 1000);
    let id2 = create_invoice(&env, &client, &business, &currency, 2000);
    let id3 = create_invoice(&env, &client, &business, &currency, 3000);

    // All start as Pending
    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Pending, 3),
        (InvoiceStatus::Verified, 0),
        (InvoiceStatus::Funded, 0),
        (InvoiceStatus::Cancelled, 0),
    ]);

    // Verify id1
    client.update_invoice_status(&id1, &InvoiceStatus::Verified);
    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Pending, 2),
        (InvoiceStatus::Verified, 1),
    ]);

    // Cancel id2
    client.cancel_invoice(&id2);
    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Pending, 1),
        (InvoiceStatus::Verified, 1),
        (InvoiceStatus::Cancelled, 1),
    ]);

    // Fund id1
    client.update_invoice_status(&id1, &InvoiceStatus::Funded);
    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Pending, 1),
        (InvoiceStatus::Verified, 0),
        (InvoiceStatus::Funded, 1),
        (InvoiceStatus::Cancelled, 1),
    ]);

    // Default id1
    client.update_invoice_status(&id1, &InvoiceStatus::Defaulted);
    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Pending, 1),
        (InvoiceStatus::Funded, 0),
        (InvoiceStatus::Defaulted, 1),
        (InvoiceStatus::Cancelled, 1),
    ]);

    let total = client.get_total_invoice_count();
    assert_eq!(total, 3);
}

#[test]
fn test_accept_bid_updates_status_list() {
    let (env, client) = setup_env_and_client();

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let admin = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let currency = env.register_stellar_asset_contract(token_admin);
    let token_client = token::Client::new(&env, &currency);
    let token_admin_client = token::StellarAssetClient::new(&env, &currency);
    token_admin_client.mint(&investor, &10000);

    client.set_admin(&admin);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Bid acceptance test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);

    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Pending, 0),
        (InvoiceStatus::Verified, 1),
        (InvoiceStatus::Funded, 0),
    ]);

    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor, &10_000);

    token_client.approve(&investor, &client.address, &10000, &20000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000, &1100);

    client.accept_bid(&invoice_id, &bid_id);

    // After bid acceptance: Verified -> Funded
    assert_status_consistency(&env, &client, &[
        (InvoiceStatus::Pending, 0),
        (InvoiceStatus::Verified, 0),
        (InvoiceStatus::Funded, 1),
    ]);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
}

#[test]
fn test_count_matches_list_length_all_statuses() {
    let (env, client) = setup_env_and_client();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Create several invoices and move to various states
    let id1 = create_invoice(&env, &client, &business, &currency, 1000);
    let id2 = create_invoice(&env, &client, &business, &currency, 2000);
    let id3 = create_invoice(&env, &client, &business, &currency, 3000);
    let id4 = create_invoice(&env, &client, &business, &currency, 4000);

    client.update_invoice_status(&id1, &InvoiceStatus::Verified);
    client.update_invoice_status(&id2, &InvoiceStatus::Verified);
    client.update_invoice_status(&id2, &InvoiceStatus::Funded);
    client.update_invoice_status(&id3, &InvoiceStatus::Verified);
    client.update_invoice_status(&id3, &InvoiceStatus::Paid);
    client.cancel_invoice(&id4);

    let all_statuses = [
        InvoiceStatus::Pending,
        InvoiceStatus::Verified,
        InvoiceStatus::Funded,
        InvoiceStatus::Paid,
        InvoiceStatus::Defaulted,
        InvoiceStatus::Cancelled,
        InvoiceStatus::Refunded,
    ];

    let mut total = 0u32;
    for status in all_statuses.iter() {
        let list = client.get_invoices_by_status(status);
        let count = client.get_invoice_count_by_status(status);
        assert_eq!(list.len() as u32, count);
        total += count;
    }

    assert_eq!(client.get_total_invoice_count(), total);
}
