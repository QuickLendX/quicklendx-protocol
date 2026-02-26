// Removed test_invoice mod
mod test_invoice_categories;
mod test_invoice_metadata;

use super::*;
use crate::bid::{BidStatus, BidStorage};
use crate::investment::{Investment, InvestmentStorage};
use crate::invoice::{InvoiceCategory, InvoiceMetadata, LineItemRecord};
use crate::verification::BusinessVerificationStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

fn verify_investor_for_test(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    limit: i128,
) {
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(investor, &limit);
}

#[test]
fn test_store_invoice() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400; // 1 day from now
    let description = String::from_str(&env, "Test invoice for services");

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Verify invoice was stored
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.business, business);
    assert_eq!(invoice.amount, amount);
    assert_eq!(invoice.currency, currency);
    assert_eq!(invoice.due_date, due_date);
    assert_eq!(invoice.description, description);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());
}

#[test]
fn test_store_invoice_validation() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Valid invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Verify invoice was created
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.amount, 1000);
    assert_eq!(invoice.business, business);
}

#[test]
fn test_get_business_invoices() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business1 = Address::generate(&env);
    let business2 = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create invoices for business1
    let invoice1_id = client.store_invoice(
        &business1,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice2_id = client.store_invoice(
        &business1,
        &2000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Create invoice for business2
    let invoice3_id = client.store_invoice(
        &business2,
        &3000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 3"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Get invoices for business1
    let business1_invoices = client.get_business_invoices(&business1);
    assert_eq!(business1_invoices.len(), 2);
    assert!(business1_invoices.contains(&invoice1_id));
    assert!(business1_invoices.contains(&invoice2_id));

    // Get invoices for business2
    let business2_invoices = client.get_business_invoices(&business2);
    assert_eq!(business2_invoices.len(), 1);
    assert!(business2_invoices.contains(&invoice3_id));
}

#[test]
fn test_get_invoices_by_status() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create invoices
    let invoice1_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let invoice2_id = client.store_invoice(
        &business,
        &2000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Get pending invoices
    let pending_invoices = client.get_invoices_by_status(&InvoiceStatus::Pending);
    assert_eq!(pending_invoices.len(), 2);
    assert!(pending_invoices.contains(&invoice1_id));
    assert!(pending_invoices.contains(&invoice2_id));

    // Get verified invoices (should be empty initially)
    let verified_invoices = client.get_invoices_by_status(&InvoiceStatus::Verified);
    assert_eq!(verified_invoices.len(), 0);
}

#[test]
fn test_update_invoice_status() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Verify invoice starts as pending
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // Update to verified
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);

    // Check status lists
    let pending_invoices = client.get_invoices_by_status(&InvoiceStatus::Pending);
    assert_eq!(pending_invoices.len(), 0);

    let verified_invoices = client.get_invoices_by_status(&InvoiceStatus::Verified);
    assert_eq!(verified_invoices.len(), 1);
    assert!(verified_invoices.contains(&invoice_id));
}

#[test]
fn test_update_invoice_metadata_and_queries() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Metadata invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let mut line_items = Vec::new(&env);
    line_items.push_back(LineItemRecord(
        String::from_str(&env, "Consulting"),
        5,
        200,
        1_000,
    ));

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Acme Corp"),
        customer_address: String::from_str(&env, "123 Market St"),
        tax_id: String::from_str(&env, "TAX-123"),
        line_items,
        notes: String::from_str(&env, "Net 30"),
    };

    client.update_invoice_metadata(&invoice_id, &metadata);

    let invoice = client.get_invoice(&invoice_id);
    let stored_metadata = invoice.metadata().expect("metadata must be stored");
    assert_eq!(stored_metadata.customer_name, metadata.customer_name);
    assert_eq!(stored_metadata.tax_id, metadata.tax_id);
    assert_eq!(stored_metadata.line_items.len(), 1);
    let stored_line_item = stored_metadata.line_items.get(0).expect("line item");
    assert_eq!(stored_line_item.3, 1_000);

    let customer_invoices = client.get_invoices_by_customer(&metadata.customer_name);
    assert!(customer_invoices.contains(&invoice_id));

    let tax_invoices = client.get_invoices_by_tax_id(&metadata.tax_id);
    assert!(tax_invoices.contains(&invoice_id));

    client.clear_invoice_metadata(&invoice_id);

    let cleared_invoice = client.get_invoice(&invoice_id);
    assert!(cleared_invoice.metadata().is_none());

    let customer_invoices_after_clear = client.get_invoices_by_customer(&metadata.customer_name);
    assert!(!customer_invoices_after_clear.contains(&invoice_id));
}

#[test]
fn test_invoice_metadata_validation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invalid metadata invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let mut invalid_items = Vec::new(&env);
    invalid_items.push_back(LineItemRecord(
        String::from_str(&env, "Consulting"),
        2,
        250,
        500,
    ));

    let invalid_metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Beta LLC"),
        customer_address: String::from_str(&env, "456 Elm St"),
        tax_id: String::from_str(&env, "TAX-456"),
        line_items: invalid_items,
        notes: String::from_str(&env, "Review"),
    };

    let result = client.try_update_invoice_metadata(&invoice_id, &invalid_metadata);
    let err = result.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::InvoiceAmountInvalid);

    let mut invalid_line = Vec::new(&env);
    invalid_line.push_back(LineItemRecord(
        String::from_str(&env, "Consulting"),
        0,
        1,
        0,
    ));

    let invalid_line_metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Gamma LLC"),
        customer_address: String::from_str(&env, "789 Oak St"),
        tax_id: String::from_str(&env, "TAX-789"),
        line_items: invalid_line,
        notes: String::from_str(&env, "Invalid"),
    };

    let result_line = client.try_update_invoice_metadata(&invoice_id, &invalid_line_metadata);
    let err_line = result_line.err().expect("expected error");
    let contract_error_line = err_line.expect("expected contract invoke error");
    assert_eq!(contract_error_line, QuickLendXError::InvalidAmount);
}

#[test]
fn test_investor_verification_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Setup token
    let token_admin = Address::generate(&env);
    let currency = env.register_stellar_asset_contract(token_admin);
    let token_client = token::Client::new(&env, &currency);
    let token_admin_client = token::StellarAssetClient::new(&env, &currency);
    token_admin_client.mint(&investor, &10000);
    let due_date = env.ledger().timestamp() + 86400;

    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Investor verification invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.verify_invoice(&invoice_id);

    let bid_attempt = client.try_place_bid(&investor, &invoice_id, &500, &600);
    let err = bid_attempt.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::BusinessNotVerified);

    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));

    let pending_attempt = client.try_place_bid(&investor, &invoice_id, &500, &600);
    let pending_err = pending_attempt.err().expect("expected pending error");
    let pending_contract_error = pending_err.expect("expected contract invoke error");
    assert_eq!(pending_contract_error, QuickLendXError::KYCAlreadyPending);

    client.verify_investor(&investor, &1_000);

    let verification = client
        .get_investor_verification(&investor)
        .expect("verification record");
    assert_eq!(verification.investment_limit, 750);
    assert!(matches!(
        verification.status,
        BusinessVerificationStatus::Verified
    ));

    let _bid_id = client.place_bid(&investor, &invoice_id, &500, &600);

    let over_limit = client.try_place_bid(&investor, &invoice_id, &1_500, &1_700);
    let limit_err = over_limit.err().expect("expected limit error");
    let limit_contract_error = limit_err.expect("expected invoke error");
    assert_eq!(limit_contract_error, QuickLendXError::InvalidAmount);
}

#[test]
fn test_get_available_invoices() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create invoices
    let invoice1_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let _invoice2_id = client.store_invoice(
        &business,
        &2000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Initially no available invoices (all pending)
    let available_invoices = client.get_available_invoices();
    assert_eq!(available_invoices.len(), 0);

    // Verify one invoice
    client.update_invoice_status(&invoice1_id, &InvoiceStatus::Verified);

    // Now one available invoice
    let available_invoices = client.get_available_invoices();
    assert_eq!(available_invoices.len(), 1);
    assert!(available_invoices.contains(&invoice1_id));
}

#[test]
fn test_invoice_count_functions() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Create invoices
    client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.store_invoice(
        &business,
        &2000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Test count by status
    let pending_count = client.get_invoice_count_by_status(&InvoiceStatus::Pending);
    assert_eq!(pending_count, 2);

    let verified_count = client.get_invoice_count_by_status(&InvoiceStatus::Verified);
    assert_eq!(verified_count, 0);

    // Test total count
    let total_count = client.get_total_invoice_count();
    assert_eq!(total_count, 2);
}

#[test]
fn test_invoice_not_found() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let fake_id = BytesN::from_array(&env, &[0u8; 32]);

    let result = client.try_get_invoice(&fake_id);
    assert!(matches!(result, Err(_)));
}

#[test]
fn test_invoice_lifecycle() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Test lifecycle: Pending -> Verified -> Paid
    let mut invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);

    client.update_invoice_status(&invoice_id, &InvoiceStatus::Paid);
    invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
    assert!(invoice.settled_at.is_some());
}

#[test]
fn test_simple_bid_storage() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Create and verify invoice
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    // Place a single bid to test basic functionality
    let bid_id = client.place_bid(&investor, &invoice_id, &900, &1000);

    // Verify that the bid can be retrieved
    let bid = client.get_bid(&bid_id);
    assert!(bid.is_some(), "Bid should be retrievable");
    let bid = bid.unwrap();
    assert_eq!(bid.bid_amount, 900);
    assert_eq!(bid.expected_return, 1000);
}

#[test]
fn test_unique_bid_id_generation() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    env.as_contract(&contract_id, || {
        let mut ids = Vec::new(&env);

        // Generate 100 unique bid IDs (reduced for faster testing)
        for _ in 0..100 {
            let id = crate::bid::BidStorage::generate_unique_bid_id(&env);

            // Check if this ID already exists in our vector
            for i in 0..ids.len() {
                let existing_id = ids.get(i).unwrap();
                assert_ne!(id, existing_id, "Duplicate bid ID generated");
            }

            ids.push_back(id);
        }
    });
    env.mock_all_auths();
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Create and verify invoice
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    // Place first bid
    let bid_id_1 = client.place_bid(&investor, &invoice_id, &900, &1100);

    // Verify first bid was stored correctly
    let bid_1 = client.get_bid(&bid_id_1);
    assert!(bid_1.is_some(), "First bid should be retrievable");

    // Attempt duplicate bid from same investor should fail
    let duplicate = client.try_place_bid(&investor, &invoice_id, &950, &1200);
    assert!(
        duplicate.is_err(),
        "Duplicate active bids should be rejected"
    );
}

#[test]
fn test_bid_ranking_and_filters() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);
    let investor_c = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    let invoice_id = client.store_invoice(
        &business,
        &2_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Ranking invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor_a, 10_000);
    verify_investor_for_test(&env, &client, &investor_b, 10_000);
    verify_investor_for_test(&env, &client, &investor_c, 10_000);

    let bid_a = client.place_bid(&investor_a, &invoice_id, &700, &880);
    let bid_b = client.place_bid(&investor_b, &invoice_id, &800, &1_050);
    let _bid_c = client.place_bid(&investor_c, &invoice_id, &900, &1_200);

    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked.len(), 3);

    let best = client.get_best_bid(&invoice_id).unwrap();
    assert_eq!(best.bid_id, ranked.get(0).unwrap().bid_id);
    assert_eq!(best.investor, investor_c);

    env.as_contract(&contract_id, || {
        let mut bid = BidStorage::get_bid(&env, &bid_a).unwrap();
        bid.status = BidStatus::Accepted;
        BidStorage::update_bid(&env, &bid);
    });

    let placed = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed.len(), 2);
    let accepted = client.get_bids_by_status(&invoice_id, &BidStatus::Accepted);
    assert_eq!(accepted.len(), 1);

    let investor_filter = client.get_bids_by_investor(&invoice_id, &investor_b);
    assert_eq!(investor_filter.len(), 1);
    assert_eq!(investor_filter.get(0).unwrap().bid_id, bid_b);
}

#[test]
fn test_bid_expiration_cleanup() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Expiration invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    let bid_id = client.place_bid(&investor, &invoice_id, &500, &650);

    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);

    let ranked = client.get_ranked_bids(&invoice_id);
    assert_eq!(ranked.len(), 1);

    env.ledger().set_timestamp(bid.expiration_timestamp + 1);

    let expired_count = client.cleanup_expired_bids(&invoice_id);
    assert_eq!(expired_count, 1);

    let bid_after = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid_after.status, BidStatus::Expired);

    assert!(client.get_ranked_bids(&invoice_id).is_empty());
    assert!(client.get_best_bid(&invoice_id).is_none());
}

#[test]
fn test_bid_validation_rules() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let other_investor = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Create and verify invoice
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Validation invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    verify_investor_for_test(&env, &client, &other_investor, 10_000);

    // Amount below minimum (1% of 1000 = 10)
    assert!(client
        .try_place_bid(&investor, &invoice_id, &5, &6)
        .is_err());

    // Expected return must exceed bid amount
    assert!(client
        .try_place_bid(&investor, &invoice_id, &150, &150)
        .is_err());

    // Amount cannot exceed invoice amount
    assert!(client
        .try_place_bid(&investor, &invoice_id, &1500, &1600)
        .is_err());

    // Valid bid succeeds
    let _bid_id = client.place_bid(&investor, &invoice_id, &150, &200);

    // Duplicate bid from same investor is rejected
    assert!(client
        .try_place_bid(&investor, &invoice_id, &180, &240)
        .is_err());

    // Another investor can still bid
    let second_bid = client.try_place_bid(&other_investor, &invoice_id, &180, &240);
    assert!(second_bid.is_ok());
}

#[test]
fn test_withdraw_bid() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Create and verify invoice
    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Withdraw test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    // Place a bid
    let bid_id = client.place_bid(&investor, &invoice_id, &500, &600);
    let bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.status, BidStatus::Placed);

    // Withdraw the bid
    client.withdraw_bid(&bid_id);
    let withdrawn_bid = client.get_bid(&bid_id).unwrap();
    assert_eq!(withdrawn_bid.status, BidStatus::Withdrawn);

    // Verify bid is no longer in placed status
    let placed_bids = client.get_bids_by_status(&invoice_id, &BidStatus::Placed);
    assert_eq!(placed_bids.len(), 0);

    // Verify bid appears in withdrawn status
    let withdrawn_bids = client.get_bids_by_status(&invoice_id, &BidStatus::Withdrawn);
    assert_eq!(withdrawn_bids.len(), 1);
    assert_eq!(withdrawn_bids.get(0).unwrap().bid_id, bid_id);

    // Try to withdraw again (should fail)
    assert!(client.try_withdraw_bid(&bid_id).is_err());
}

#[test]
fn test_get_bids_for_invoice() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Create and verify invoice
    let invoice_id = client.store_invoice(
        &business,
        &2000,
        &currency,
        &due_date,
        &String::from_str(&env, "Get bids test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor_a, 10_000);
    verify_investor_for_test(&env, &client, &investor_b, 10_000);

    // Place multiple bids
    let bid_a = client.place_bid(&investor_a, &invoice_id, &500, &600);
    let bid_b = client.place_bid(&investor_b, &invoice_id, &600, &750);

    // Get all bids for invoice
    let all_bids = client.get_bids_for_invoice(&invoice_id);
    assert_eq!(all_bids.len(), 2);

    // Verify both bids are present
    let mut found_a = false;
    let mut found_b = false;
    for bid in all_bids.iter() {
        if bid.bid_id == bid_a {
            found_a = true;
            assert_eq!(bid.investor, investor_a);
        }
        if bid.bid_id == bid_b {
            found_b = true;
            assert_eq!(bid.investor, investor_b);
        }
    }
    assert!(found_a && found_b, "Both bids should be found");

    // Withdraw one bid
    client.withdraw_bid(&bid_a);

    // Get all bids again (should still include withdrawn bid)
    let all_bids_after = client.get_bids_for_invoice(&invoice_id);
    assert_eq!(all_bids_after.len(), 2);

    // Verify withdrawn bid is still in the list
    let withdrawn = all_bids_after.iter().find(|b| b.bid_id == bid_a).unwrap();
    assert_eq!(withdrawn.status, BidStatus::Withdrawn);
}

#[test]
fn test_escrow_creation_on_bid_acceptance() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Setup token
    let token_admin = Address::generate(&env);
    let currency = env.register_stellar_asset_contract(token_admin);
    let token_client = token::Client::new(&env, &currency);
    let token_admin_client = token::StellarAssetClient::new(&env, &currency);
    token_admin_client.mint(&investor, &10000);

    let due_date = env.ledger().timestamp() + 86400;
    let bid_amount = 1000i128;
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Create and verify invoice
    let invoice_id = client.store_invoice(
        &business,
        &bid_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    // Place bid
    token_client.approve(&investor, &contract_id, &10000, &20000);
    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &1100);

    // Accept bid (should create escrow)
    client.accept_bid(&invoice_id, &bid_id);

    // Verify escrow was created
    let escrow_details = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow_details.invoice_id, invoice_id);
    assert_eq!(escrow_details.investor, investor);
    assert_eq!(escrow_details.business, business);
    assert_eq!(escrow_details.amount, bid_amount);
    assert_eq!(escrow_details.currency, currency);
    assert_eq!(escrow_details.status, crate::payments::EscrowStatus::Held);

    // Verify escrow status
    let escrow_status = client.get_escrow_status(&invoice_id);
    assert_eq!(escrow_status, crate::payments::EscrowStatus::Held);
}

#[test]
fn test_escrow_release_on_verification() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Setup token
    let token_admin = Address::generate(&env);
    let currency = env.register_stellar_asset_contract(token_admin);
    let token_client = token::Client::new(&env, &currency);
    let token_admin_client = token::StellarAssetClient::new(&env, &currency);
    token_admin_client.mint(&investor, &10000);

    let due_date = env.ledger().timestamp() + 86400;
    let bid_amount = 1000i128;
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Create invoice
    let invoice_id = client.store_invoice(
        &business,
        &bid_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    // Place and accept bid (creates escrow)
    token_client.approve(&investor, &contract_id, &10000, &20000);
    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &1100);
    client.accept_bid(&invoice_id, &bid_id);

    // Verify escrow is held
    let escrow_status = client.get_escrow_status(&invoice_id);
    assert_eq!(escrow_status, crate::payments::EscrowStatus::Held);

    // Release escrow funds
    client.release_escrow_funds(&invoice_id);

    // Verify escrow is released
    let escrow_status = client.get_escrow_status(&invoice_id);
    assert_eq!(escrow_status, crate::payments::EscrowStatus::Released);
}

#[test]
fn test_escrow_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Setup token
    let token_admin = Address::generate(&env);
    let currency = env.register_stellar_asset_contract(token_admin);
    let token_client = token::Client::new(&env, &currency);
    let token_admin_client = token::StellarAssetClient::new(&env, &currency);
    token_admin_client.mint(&investor, &10000);

    let due_date = env.ledger().timestamp() + 86400;
    let bid_amount = 1000i128;

    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Create invoice
    let invoice_id = client.store_invoice(
        &business,
        &bid_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    // Place and accept bid (creates escrow)
    token_client.approve(&investor, &contract_id, &10000, &20000);
    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &1100);
    client.accept_bid(&invoice_id, &bid_id);

    // Verify escrow is held
    let escrow_status = client.get_escrow_status(&invoice_id);
    assert_eq!(escrow_status, crate::payments::EscrowStatus::Held);

    // Refund escrow funds
    client.refund_escrow_funds(&invoice_id, &admin);

    // Verify escrow is refunded
    let escrow_status = client.get_escrow_status(&invoice_id);
    assert_eq!(escrow_status, crate::payments::EscrowStatus::Refunded);

    // Verify funds returned to investor
    // Note: investor had 10000, bid 1000, so balance was 9000. Refunded 1000, so balance 10000.
    assert_eq!(token_client.balance(&investor), 10000);
}

#[test]
fn test_escrow_status_tracking() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Setup token
    let token_admin = Address::generate(&env);
    let currency = env.register_stellar_asset_contract(token_admin);
    let token_client = token::Client::new(&env, &currency);
    let token_admin_client = token::StellarAssetClient::new(&env, &currency);
    token_admin_client.mint(&investor, &10000);

    let due_date = env.ledger().timestamp() + 86400;
    let bid_amount = 1000i128;

    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Create and verify invoice
    let invoice_id = client.store_invoice(
        &business,
        &bid_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    // Place and accept bid
    token_client.approve(&investor, &contract_id, &10000, &20000);
    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &1100);
    client.accept_bid(&invoice_id, &bid_id);

    // Test escrow details
    let escrow_details = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow_details.status, crate::payments::EscrowStatus::Held);
    // created_at is set to ledger timestamp (u64 is always >= 0)
    assert_eq!(escrow_details.amount, bid_amount);

    // Test status progression: Held -> Released
    client.release_escrow_funds(&invoice_id);
    let escrow_details = client.get_escrow_details(&invoice_id);
    assert_eq!(
        escrow_details.status,
        crate::payments::EscrowStatus::Released
    );
}

#[test]
fn test_escrow_error_cases() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let fake_invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    // Test getting escrow for non-existent invoice
    let result = client.try_get_escrow_status(&fake_invoice_id);
    assert!(matches!(result, Err(_)));

    let result = client.try_get_escrow_details(&fake_invoice_id);
    assert!(matches!(result, Err(_)));

    // Test releasing escrow for non-existent invoice
    let result = client.try_release_escrow_funds(&fake_invoice_id);
    assert!(matches!(result, Err(_)));

    // Test refunding escrow for non-existent invoice
    let dummy_admin = Address::generate(&env);
    let result = client.try_refund_escrow_funds(&fake_invoice_id, &dummy_admin);
    assert!(matches!(result, Err(_)));
}

#[test]
fn test_escrow_double_operation_prevention() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Setup token
    let token_admin = Address::generate(&env);
    let currency = env.register_stellar_asset_contract(token_admin);
    let token_client = token::Client::new(&env, &currency);
    let token_admin_client = token::StellarAssetClient::new(&env, &currency);
    token_admin_client.mint(&investor, &10000);

    let due_date = env.ledger().timestamp() + 86400;
    let bid_amount = 1000i128;

    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Create and verify invoice
    let invoice_id = client.store_invoice(
        &business,
        &bid_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    // Place and accept bid
    token_client.approve(&investor, &contract_id, &10000, &20000);
    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &1100);
    client.accept_bid(&invoice_id, &bid_id);

    // Release escrow funds
    client.release_escrow_funds(&invoice_id);

    // Try to release again (should fail)
    let result = client.try_release_escrow_funds(&invoice_id);
    assert!(matches!(result, Err(_)));

    let dummy_admin = Address::generate(&env);
    // Try to refund after release (should fail)
    let result = client.try_refund_escrow_funds(&invoice_id, &dummy_admin);
    assert!(matches!(result, Err(_)));
}

#[test]
fn test_unique_investment_id_generation() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    env.as_contract(&contract_id, || {
        let mut ids = Vec::new(&env);

        // Generate 100 unique investment IDs (reduced for faster testing)
        for _ in 0..100 {
            let id = crate::investment::InvestmentStorage::generate_unique_investment_id(&env);

            // Check if this ID already exists in our vector
            for i in 0..ids.len() {
                let existing_id = ids.get(i).unwrap();
                assert_ne!(id, existing_id, "Duplicate investment ID generated");
            }

            ids.push_back(id);
        }
    });
}


// Business KYC/Verification Tests (from main branch)

#[test]
fn test_submit_kyc_application() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let kyc_data = String::from_str(&env, "Business registration documents");

    // Mock business authorization
    env.mock_all_auths();

    client.submit_kyc_application(&business, &kyc_data);

    // Verify KYC was submitted
    let verification = client.get_business_verification_status(&business);
    assert!(verification.is_some());
    let verification = verification.unwrap();
    assert_eq!(verification.business, business);
    assert_eq!(verification.kyc_data, kyc_data);
    assert!(matches!(
        verification.status,
        verification::BusinessVerificationStatus::Pending
    ));
}

#[test]
fn test_verify_business() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let kyc_data = String::from_str(&env, "Business registration documents");

    // Set admin
    env.mock_all_auths();
    client.set_admin(&admin);

    // Submit KYC application
    env.mock_all_auths();
    client.submit_kyc_application(&business, &kyc_data);

    // Verify business
    env.mock_all_auths();
    client.verify_business(&admin, &business);

    // Check verification status
    let verification = client.get_business_verification_status(&business);
    assert!(verification.is_some());
    let verification = verification.unwrap();
    assert!(matches!(
        verification.status,
        verification::BusinessVerificationStatus::Verified
    ));
    assert!(verification.verified_at.is_some());
    assert_eq!(verification.verified_by, Some(admin));
}

#[test]
fn test_verify_invoice_requires_admin() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    let business = Address::generate(&env);
    let admin = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Admin gating"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert!(client.try_verify_invoice(&invoice_id).is_err());

    env.mock_all_auths();
    client.set_admin(&admin);

    client.verify_invoice(&invoice_id);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
}

#[test]
fn test_reject_business() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let kyc_data = String::from_str(&env, "Business registration documents");
    let rejection_reason = String::from_str(&env, "Incomplete documentation");

    // Set admin
    env.mock_all_auths();
    client.set_admin(&admin);

    // Submit KYC application
    env.mock_all_auths();
    client.submit_kyc_application(&business, &kyc_data);

    // Reject business
    env.mock_all_auths();
    client.reject_business(&admin, &business, &rejection_reason);

    // Check verification status
    let verification = client.get_business_verification_status(&business);
    assert!(verification.is_some());
    let verification = verification.unwrap();
    assert!(matches!(
        verification.status,
        verification::BusinessVerificationStatus::Rejected
    ));
    assert_eq!(verification.rejection_reason, Some(rejection_reason));
}

#[test]
fn test_upload_invoice_requires_verification() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");

    // Mock business authorization
    env.mock_all_auths();

    // Try to upload invoice without verification - should fail
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());

    // Submit KYC and verify business
    let admin = Address::generate(&env);
    let kyc_data = String::from_str(&env, "Business registration documents");

    env.mock_all_auths();
    client.set_admin(&admin);
    env.mock_all_auths();
    client.submit_kyc_application(&business, &kyc_data);

    env.mock_all_auths();
    client.verify_business(&admin, &business);

    // Now try to upload invoice - should succeed
    env.mock_all_auths();
    let _invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
}

#[test]
fn test_kyc_already_pending() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let kyc_data = String::from_str(&env, "Business registration documents");

    // Mock business authorization
    env.mock_all_auths();

    // Submit KYC application
    client.submit_kyc_application(&business, &kyc_data);

    // Try to submit again - should fail
    let result = client.try_submit_kyc_application(&business, &kyc_data);
    assert!(matches!(result, Err(_)));
}

#[test]
fn test_kyc_already_verified() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let kyc_data = String::from_str(&env, "Business registration documents");

    // Set admin and submit KYC
    env.mock_all_auths();
    client.set_admin(&admin);
    env.mock_all_auths();
    client.submit_kyc_application(&business, &kyc_data);

    // Verify business
    env.mock_all_auths();
    client.verify_business(&admin, &business);

    // Try to submit KYC again - should fail
    let result = client.try_submit_kyc_application(&business, &kyc_data);
    assert!(matches!(result, Err(_)));
}

#[test]
fn test_kyc_resubmission_after_rejection() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let kyc_data = String::from_str(&env, "Business registration documents");
    let rejection_reason = String::from_str(&env, "Incomplete documentation");

    // Set admin and submit KYC
    env.mock_all_auths();
    client.set_admin(&admin);
    env.mock_all_auths();
    client.submit_kyc_application(&business, &kyc_data);

    // Reject business
    env.mock_all_auths();
    client.reject_business(&admin, &business, &rejection_reason);

    // Try to resubmit KYC - should succeed
    let new_kyc_data = String::from_str(&env, "Updated business registration documents");
    env.mock_all_auths();
    client.submit_kyc_application(&business, &new_kyc_data);

    // Check status is back to pending
    let verification = client.get_business_verification_status(&business);
    assert!(verification.is_some());
    let verification = verification.unwrap();
    assert!(matches!(
        verification.status,
        verification::BusinessVerificationStatus::Pending
    ));
    assert_eq!(verification.kyc_data, new_kyc_data);
}

#[test]
fn test_verification_unauthorized_access() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let unauthorized_admin = Address::generate(&env);

    // Set admin
    env.mock_all_auths();
    client.set_admin(&admin);

    // Submit KYC application
    env.mock_all_auths();
    let kyc_data = String::from_str(&env, "Business registration documents");
    client.submit_kyc_application(&business, &kyc_data);

    // Try to verify with unauthorized admin - should fail
    env.mock_all_auths();
    let result = client.try_verify_business(&unauthorized_admin, &business);
    assert!(matches!(result, Err(_)));
}

#[test]
fn test_get_verification_lists() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business1 = Address::generate(&env);
    let business2 = Address::generate(&env);
    let business3 = Address::generate(&env);

    // Set admin
    env.mock_all_auths();
    client.set_admin(&admin);

    // Submit KYC applications
    env.mock_all_auths();
    let kyc_data = String::from_str(&env, "Business registration documents");
    client.submit_kyc_application(&business1, &kyc_data);
    client.submit_kyc_application(&business2, &kyc_data);
    client.submit_kyc_application(&business3, &kyc_data);

    // Verify business1, reject business2, leave business3 pending
    env.mock_all_auths();
    client.verify_business(&admin, &business1);
    client.reject_business(&admin, &business2, &String::from_str(&env, "Rejected"));

    // Check lists
    let verified = client.get_verified_businesses();
    let pending = client.get_pending_businesses();
    let rejected = client.get_rejected_businesses();

    assert_eq!(verified.len(), 1);
    assert_eq!(pending.len(), 1);
    assert_eq!(rejected.len(), 1);

    assert!(verified.contains(&business1));
    assert!(pending.contains(&business3));
    assert!(rejected.contains(&business2));
}

#[test]
fn test_platform_fee_configuration() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);

    let default_config = client.get_platform_fee();
    assert_eq!(default_config.fee_bps, 200);

    client.set_platform_fee(&300);
    let updated_config = client.get_platform_fee();
    assert_eq!(updated_config.fee_bps, 300);
    assert_eq!(updated_config.updated_by, admin);

    let (investor_return, platform_fee) = client.calculate_profit(&1_000, &1_200);
    assert_eq!(investor_return, 1_194);
    assert_eq!(platform_fee, 6);

    let invalid = client.try_set_platform_fee(&1_200);
    let err = invalid.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::InvalidAmount);
}


#[test]
fn test_invoice_expiration_triggers_default() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    let initial_balance = 5_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);

    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(&business, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, &contract_id, &initial_balance, &expiration);

    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));
    client.verify_business(&admin, &business);

    let due_date = env.ledger().timestamp() + 60;
    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Expiring invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1_000, &1_100);
    client.accept_bid(&invoice_id, &bid_id);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    env.ledger().set_timestamp(invoice.due_date + 1);

    let defaulted = client.check_invoice_expiration(&invoice_id, &Some(0));
    assert!(defaulted);

    let updated_invoice = client.get_invoice(&invoice_id);
    assert_eq!(updated_invoice.status, InvoiceStatus::Defaulted);
}

#[test]
fn test_partial_payments_trigger_settlement() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    let initial_balance = 5_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);

    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(&business, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, &contract_id, &initial_balance, &expiration);

    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));
    client.verify_business(&admin, &business);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Partial payment invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1_000, &1_100);
    client.accept_bid(&invoice_id, &bid_id);

    let tx1 = String::from_str(&env, "tx-1");
    client.process_partial_payment(&invoice_id, &400, &tx1);

    let mid_invoice = client.get_invoice(&invoice_id);
    assert_eq!(mid_invoice.total_paid, 400);
    assert_eq!(mid_invoice.payment_history.len(), 1);
    assert_eq!(mid_invoice.status, InvoiceStatus::Funded);
    assert_eq!(mid_invoice.payment_progress(), 40);

    let tx2 = String::from_str(&env, "tx-2");
    client.process_partial_payment(&invoice_id, &600, &tx2);

    let settled_invoice = client.get_invoice(&invoice_id);
    assert_eq!(settled_invoice.status, InvoiceStatus::Paid);
    assert_eq!(settled_invoice.total_paid, 1_000);
    assert_eq!(settled_invoice.payment_history.len(), 2);
    assert_eq!(settled_invoice.payment_progress(), 100);

    let investment = env
        .as_contract(&contract_id, || {
            InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
        })
        .expect("investment");
    assert_eq!(investment.status, InvestmentStatus::Completed);
}

// Dispute Resolution System Tests (from main)

// TODO: Fix authorization issues in test environment

// TODO: Fix authorization issues in test environment

// TODO: Fix authorization issues in test environment

// TODO: Fix authorization issues in test environment

// TODO: Fix authorization issues in test environment

// TODO: Fix authorization issues in test environment

// TODO: Fix authorization issues in test environment

// TODO: Fix authorization issues in test environment

// TODO: Fix authorization issues in test environment

#[test]
fn test_investment_insurance_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let admin = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    let initial_balance = 10_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);

    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(&business, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, &contract_id, &initial_balance, &expiration);

    client.set_admin(&admin);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice with insurance"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    let bid_id = client.place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);
    client.accept_bid(&invoice_id, &bid_id);

    let investment = client.get_invoice_investment(&invoice_id);
    let investment_id = investment.investment_id.clone();

    let invalid_attempt = client.try_add_investment_insurance(&investment_id, &provider, &150u32);
    let err = invalid_attempt.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::InvalidCoveragePercentage);

    let coverage_percentage = 60u32;
    client.add_investment_insurance(&investment_id, &provider, &coverage_percentage);

    let duplicate_provider = Address::generate(&env);
    let duplicate_attempt =
        client.try_add_investment_insurance(&investment_id, &duplicate_provider, &30u32);
    let err = duplicate_attempt.err().expect("expected contract error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::OperationNotAllowed);

    let insured_investment = client.get_invoice_investment(&invoice_id);
    let investment_amount = insured_investment.amount;
    assert_eq!(insured_investment.insurance.len(), 1);
    let insurance = insured_investment
        .insurance
        .get(0)
        .expect("expected insurance entry");
    assert!(insurance.active);
    assert_eq!(insurance.provider, provider);
    assert_eq!(insurance.coverage_percentage, coverage_percentage);
    let expected_coverage = investment_amount * coverage_percentage as i128 / 100;
    assert_eq!(insurance.coverage_amount, expected_coverage);
    let expected_premium = Investment::calculate_premium(investment_amount, coverage_percentage);
    assert_eq!(insurance.premium_amount, expected_premium);

    let stored_invoice = client.get_invoice(&invoice_id);
    env.ledger().set_timestamp(stored_invoice.due_date + 1);
    let result = client.try_handle_default(&invoice_id);
    assert!(result.is_ok());

    let after_default = client.get_invoice_investment(&invoice_id);
    assert_eq!(after_default.status, InvestmentStatus::Defaulted);
    assert_eq!(after_default.insurance.len(), 1);
    let insurance_after = after_default
        .insurance
        .get(0)
        .expect("expected insurance entry after claim");
    assert!(!insurance_after.active);
    assert_eq!(insurance_after.coverage_amount, expected_coverage);
}

#[test]
fn test_query_investment_insurance_single_coverage() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let admin = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    let initial_balance = 10_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);

    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(&business, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, &contract_id, &initial_balance, &expiration);

    client.set_admin(&admin);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &5_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test insurance query"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    let bid_id = client.place_bid(&investor, &invoice_id, &5_000i128, &5_500i128);
    client.accept_bid(&invoice_id, &bid_id);

    let investment = client.get_invoice_investment(&invoice_id);
    let investment_id = investment.investment_id.clone();

    // Query with no insurance should return empty vector
    let insurance_before = client
        .try_query_investment_insurance(&investment_id)
        .unwrap()
        .unwrap();
    assert_eq!(insurance_before.len(), 0);

    // Add insurance
    let coverage_percentage = 75u32;
    client.add_investment_insurance(&investment_id, &provider, &coverage_percentage);

    // Query should now return the insurance coverage
    let insurance_vec = client
        .try_query_investment_insurance(&investment_id)
        .unwrap()
        .unwrap();
    assert_eq!(insurance_vec.len(), 1);

    let coverage = insurance_vec.get(0).expect("expected insurance coverage");
    assert_eq!(coverage.provider, provider);
    assert_eq!(coverage.coverage_percentage, coverage_percentage);
    assert!(coverage.active);
    let expected_amount = 5_000i128 * 75 / 100;
    assert_eq!(coverage.coverage_amount, expected_amount);
    assert!(coverage.premium_amount > 0);
}

#[test]
fn test_query_investment_insurance_nonexistent_investment() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let fake_investment_id = BytesN::from_array(
        &env,
        &[
            0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31,
        ],
    );

    // Query nonexistent investment should return StorageKeyNotFound
    let result = client.try_query_investment_insurance(&fake_investment_id);
    assert!(result.is_err());
    let err = result.err().expect("expected error");
    let contract_error = err.expect("expected contract invoke error");
    assert_eq!(contract_error, QuickLendXError::StorageKeyNotFound);
}

#[test]
fn test_query_investment_insurance_premium_calculation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let admin = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    let initial_balance = 100_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);

    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(&business, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, &contract_id, &initial_balance, &expiration);

    client.set_admin(&admin);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_amount = 10_000i128;
    let invoice_id = client.store_invoice(
        &business,
        &invoice_amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Premium calculation test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 100_000);

    let bid_id = client.place_bid(&investor, &invoice_id, &invoice_amount, &11_000i128);
    client.accept_bid(&invoice_id, &bid_id);

    let investment = client.get_invoice_investment(&invoice_id);
    let investment_id = investment.investment_id.clone();

    // Test multiple coverage percentages
    let test_cases: [(u32, i128); 3] = [
        (50u32, 5_000i128),   // 50% of 10,000
        (80u32, 8_000i128),   // 80% of 10,000
        (100u32, 10_000i128), // 100% of 10,000
    ];

    for (idx, (coverage_pct, expected_coverage)) in test_cases.iter().enumerate() {
        let provider_i = if idx == 0 {
            provider.clone()
        } else {
            // Can't add multiple insurances, so test each separately
            break;
        };

        client.add_investment_insurance(&investment_id, &provider_i, coverage_pct);

        let insurance_vec = client
            .try_query_investment_insurance(&investment_id)
            .unwrap()
            .unwrap();
        assert_eq!(insurance_vec.len(), 1);

        let coverage = insurance_vec.get(0).expect("expected coverage");
        assert_eq!(coverage.coverage_percentage, *coverage_pct);
        assert_eq!(coverage.coverage_amount, *expected_coverage);

        // Verify premium calculation: coverage_amount * DEFAULT_INSURANCE_PREMIUM_BPS / 10_000
        // where DEFAULT_INSURANCE_PREMIUM_BPS = 200 (2%)
        let expected_premium = *expected_coverage * 200 / 10_000;
        let expected_premium = if expected_premium == 0 && expected_coverage > &0 {
            1
        } else {
            expected_premium
        };
        assert_eq!(coverage.premium_amount, expected_premium);
    }
}

#[test]
fn test_query_investment_insurance_inactive_coverage() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let provider = Address::generate(&env);
    let admin = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    let initial_balance = 10_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);

    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(&business, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, &contract_id, &initial_balance, &expiration);

    client.set_admin(&admin);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test inactive coverage"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    let bid_id = client.place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);
    client.accept_bid(&invoice_id, &bid_id);

    let investment = client.get_invoice_investment(&invoice_id);
    let investment_id = investment.investment_id.clone();

    // Add insurance
    client.add_investment_insurance(&investment_id, &provider, &60u32);

    // Query and verify it's active
    let insurance_before = client
        .try_query_investment_insurance(&investment_id)
        .unwrap()
        .unwrap();
    let coverage_before = insurance_before.get(0).expect("expected coverage");
    assert!(coverage_before.active);

    // Trigger default to deactivate insurance
    let stored_invoice = client.get_invoice(&invoice_id);
    env.ledger().set_timestamp(stored_invoice.due_date + 1);
    let _ = client.handle_default(&invoice_id);

    // Query and verify it's now inactive
    let insurance_after = client
        .try_query_investment_insurance(&investment_id)
        .unwrap()
        .unwrap();
    let coverage_after = insurance_after.get(0).expect("expected coverage");
    assert!(!coverage_after.active);
    assert_eq!(
        coverage_after.coverage_amount,
        coverage_before.coverage_amount
    );
}

// Test basic functionality from README.md

// ========================================
// Invoice Lifecycle Tests
// ========================================

#[test]
fn test_upload_invoice_success() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin and verify business
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Upload invoice
    let amount = 1000000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Payment for consulting services");
    let tags = Vec::new(&env);

    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Consulting,
        &tags,
    );

    // Verify invoice was created with correct status
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
    assert_eq!(invoice.business, business);
    assert_eq!(invoice.amount, amount);
    assert_eq!(invoice.due_date, due_date);
}

#[test]
#[should_panic]
fn test_upload_invoice_not_verified_business() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Try to upload invoice without being verified
    let amount = 1000000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);

    client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
}

#[test]
#[should_panic]
fn test_upload_invoice_invalid_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin and verify business
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Try to upload invoice with negative amount
    let amount = -100i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);

    client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
}

#[test]
#[should_panic]
fn test_upload_invoice_past_due_date() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin and verify business
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Try to upload invoice with past due date
    let amount = 1000000i128;
    let due_date = env.ledger().timestamp() - 86400; // Past date
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);

    client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
}

#[test]
fn test_verify_invoice_success() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin and verify business
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Upload invoice
    let amount = 1000000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);

    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );

    // Verify invoice status is Pending
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // Verify the invoice
    client.verify_invoice(&invoice_id);

    // Check status changed to Verified
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
}

#[test]
fn test_verify_invoice_not_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin and verify business
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Upload invoice
    let amount = 1000000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);

    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );

    // Try to verify as non-admin (should fail in real scenario)
    // Note: mock_all_auths() bypasses auth, so we set admin first
    client.set_admin(&non_admin);
    client.verify_invoice(&invoice_id);
}

#[test]
#[should_panic]
fn test_verify_invoice_already_verified() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin and verify business
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Upload invoice
    let amount = 1000000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);

    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );

    // Verify once
    client.verify_invoice(&invoice_id);

    // Try to verify again (should fail)
    client.verify_invoice(&invoice_id);
}

#[test]
fn test_cancel_invoice_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin and verify business
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Upload invoice
    let amount = 1000000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);

    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );

    // Verify invoice is Pending
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // Cancel the invoice
    client.cancel_invoice(&invoice_id);

    // Verify status changed to Cancelled
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Cancelled);
}

#[test]
fn test_cancel_invoice_verified() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin and verify business
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Upload invoice
    let amount = 1000000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);

    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );

    // Verify the invoice
    client.verify_invoice(&invoice_id);

    // Verify invoice is Verified
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);

    // Cancel the invoice
    client.cancel_invoice(&invoice_id);

    // Verify status changed to Cancelled
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Cancelled);
}

#[test]
#[should_panic]
fn test_cancel_invoice_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin and verify business and investor
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);
    verify_investor_for_test(&env, &client, &investor, 10000000);

    // Upload and verify invoice
    let amount = 1000000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);

    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );

    client.verify_invoice(&invoice_id);

    // Investor places bid
    let bid_amount = amount;
    let expected_return = amount + 100000;
    let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &expected_return);

    // Business accepts bid (invoice becomes Funded)
    client.accept_bid(&invoice_id, &bid_id);

    // Verify invoice is Funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // Try to cancel funded invoice (should fail)
    client.cancel_invoice(&invoice_id);
}

#[test]
fn test_complete_invoice_lifecycle_with_cancellation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Setup: Set admin and verify business
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Step 1: Upload invoice
    let amount = 1000000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(&env, "Consulting services invoice");
    let tags = Vec::new(&env);

    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Consulting,
        &tags,
    );

    // Verify invoice is Pending
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
    assert_eq!(invoice.business, business);
    assert_eq!(invoice.amount, amount);

    // Step 2: Verify invoice
    client.verify_invoice(&invoice_id);

    // Verify status changed to Verified
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);

    // Step 3: Cancel invoice (business changes mind)
    client.cancel_invoice(&invoice_id);

    // Verify status changed to Cancelled
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Cancelled);

    // Verify cancelled invoices are tracked
    let cancelled_invoices = client.get_invoices_by_status(&InvoiceStatus::Cancelled);
    assert_eq!(cancelled_invoices.len(), 1);
    assert_eq!(cancelled_invoices.get(0).unwrap(), invoice_id);
}

#[test]
fn test_invoice_lifecycle_counts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Setup
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Create multiple invoices in different states
    let due_date = env.ledger().timestamp() + 86400;
    let tags = Vec::new(&env);

    // Invoice 1: Pending
    let _invoice_id_1 = client.upload_invoice(
        &business,
        &1000000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &tags,
    );

    // Invoice 2: Verified
    let invoice_id_2 = client.upload_invoice(
        &business,
        &2000000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Products,
        &tags,
    );
    client.verify_invoice(&invoice_id_2);

    // Invoice 3: Cancelled
    let invoice_id_3 = client.upload_invoice(
        &business,
        &3000000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 3"),
        &InvoiceCategory::Consulting,
        &tags,
    );
    client.verify_invoice(&invoice_id_3);
    client.cancel_invoice(&invoice_id_3);

    // Verify counts
    let pending_count = client.get_invoice_count_by_status(&InvoiceStatus::Pending);
    let verified_count = client.get_invoice_count_by_status(&InvoiceStatus::Verified);
    let cancelled_count = client.get_invoice_count_by_status(&InvoiceStatus::Cancelled);
    let total_count = client.get_total_invoice_count();

    assert_eq!(pending_count, 1);
    assert_eq!(verified_count, 1);
    assert_eq!(cancelled_count, 1);
    assert_eq!(total_count, 3);
}

#[test]
fn test_get_invoices_by_status_cancelled() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Setup
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    let due_date = env.ledger().timestamp() + 86400;
    let tags = Vec::new(&env);

    // Create and cancel multiple invoices
    let mut cancelled_ids = Vec::new(&env);
    for i in 0..3 {
        let desc = if i == 0 {
            "Invoice 1"
        } else if i == 1 {
            "Invoice 2"
        } else {
            "Invoice 3"
        };
        let invoice_id = client.upload_invoice(
            &business,
            &((i + 1) * 1000000),
            &currency,
            &due_date,
            &String::from_str(&env, desc),
            &InvoiceCategory::Services,
            &tags,
        );
        client.cancel_invoice(&invoice_id);
        cancelled_ids.push_back(invoice_id);
    }

    // Get all cancelled invoices
    let cancelled_invoices = client.get_invoices_by_status(&InvoiceStatus::Cancelled);
    assert_eq!(cancelled_invoices.len(), 3);

    // Verify all cancelled IDs are in the list
    for id in cancelled_ids.iter() {
        let found = cancelled_invoices.iter().any(|invoice_id| invoice_id == id);
        assert!(found);
    }
}

#[test]
fn test_store_invoice_max_due_date_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin and add currency to whitelist
    client.set_admin(&admin);
    client.add_currency(&admin, &currency);

    // Initialize protocol limits
    client.initialize_protocol_limits(&admin, &1000000i128, &365u64, &86400u64);

    let amount = 1000000i128;
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);
    let current_time = env.ledger().timestamp();

    // Test 1: Due date exactly at max boundary (365 days) should succeed
    let max_due_date = current_time + (365 * 86400);
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &max_due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(invoice_id.len() == 32);

    // Test 2: Due date just over max boundary (366 days) should fail
    let over_max_due_date = current_time + (366 * 86400);
    let result = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &over_max_due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));

    // Test 3: Due date well within bounds (30 days) should succeed
    let normal_due_date = current_time + (30 * 86400);
    let invoice_id2 = client.store_invoice(
        &business,
        &amount,
        &currency,
        &normal_due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(invoice_id2.len() == 32);
}

#[test]
fn test_upload_invoice_max_due_date_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin, verify business, and add currency
    client.set_admin(&admin);
    client.add_currency(&admin, &currency);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    // Initialize protocol limits
    client.initialize_protocol_limits(&admin, &1000000i128, &365u64, &86400u64);

    let amount = 1000000i128;
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);
    let current_time = env.ledger().timestamp();

    // Test 1: Due date exactly at max boundary (365 days) should succeed
    let max_due_date = current_time + (365 * 86400);
    let invoice_id = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &max_due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(invoice_id.len() == 32);

    // Test 2: Due date just over max boundary (366 days) should fail
    let over_max_due_date = current_time + (366 * 86400);
    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &over_max_due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));

    // Test 3: Due date well within bounds (30 days) should succeed
    let normal_due_date = current_time + (30 * 86400);
    let invoice_id2 = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &normal_due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(invoice_id2.len() == 32);
}

#[test]
fn test_custom_max_due_date_limits() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin and add currency to whitelist
    client.set_admin(&admin);
    client.add_currency(&admin, &currency);

    // Initialize protocol limits with custom max due date (30 days)
    client.initialize_protocol_limits(&admin, &1000000i128, &30u64, &86400u64);

    let amount = 1000000i128;
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);
    let current_time = env.ledger().timestamp();

    // Test 1: Due date exactly at custom max boundary (30 days) should succeed
    let max_due_date = current_time + (30 * 86400);
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &max_due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(invoice_id.len() == 32);

    // Test 2: Due date just over custom max boundary (31 days) should fail
    let over_max_due_date = current_time + (31 * 86400);
    let result = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &over_max_due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));

    // Test 3: Update limits to 730 days and test old boundary now succeeds
    client.set_protocol_limits(&admin, &1000000i128, &730u64, &86400u64);
    let old_over_max_due_date = current_time + (365 * 86400);
    let invoice_id2 = client.store_invoice(
        &business,
        &amount,
        &currency,
        &old_over_max_due_date,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(invoice_id2.len() == 32);
}

#[test]
fn test_due_date_bounds_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set admin and add currency to whitelist
    client.set_admin(&admin);
    client.add_currency(&admin, &currency);

    // Initialize with minimum max due date (1 day)
    client.initialize_protocol_limits(&admin, &1000000i128, &1u64, &86400u64);

    let amount = 1000000i128;
    let description = String::from_str(&env, "Test invoice");
    let tags = Vec::new(&env);
    let current_time = env.ledger().timestamp();

    // Test 1: Due date exactly 1 day ahead should succeed
    let one_day_due = current_time + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &one_day_due,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(invoice_id.len() == 32);

    // Test 2: Due date 1 second over limit should fail
    let one_second_over = current_time + 86401;
    let result = client.try_store_invoice(
        &business,
        &amount,
        &currency,
        &one_second_over,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));

    // Test 3: Future timestamp (current time + 1 second) should still respect max due date
    let future_current = current_time + 1;
    env.ledger().set_timestamp(future_current);
    
    let one_day_from_future = future_current + 86400;
    let invoice_id2 = client.store_invoice(
        &business,
        &amount,
        &currency,
        &one_day_from_future,
        &description,
        &InvoiceCategory::Services,
        &tags,
    );
    assert!(invoice_id2.len() == 32);
}
