use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceMetadata, LineItemRecord};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec, IntoVal,
};

fn setup_contract(env: &Env) -> (QuickLendXContractClient, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.set_admin(&admin);
    (client, admin)
}

fn create_base_invoice(env: &Env, client: &QuickLendXContractClient) -> (Address, soroban_sdk::BytesN<32>) {
    let business = Address::generate(env);
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86400;
    
    // Total amount = 5000
    let id = client.store_invoice(
        &business,
        &5000,
        &currency,
        &due_date,
        &String::from_str(env, "Base invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    (business, id)
}

fn valid_metadata(env: &Env) -> InvoiceMetadata {
    let mut line_items = Vec::new(env);
    // 2 items: 2 qty * 1000 = 2000, 3 qty * 1000 = 3000 -> Total 5000
    line_items.push_back(LineItemRecord(String::from_str(env, "Item 1"), 2, 1000, 2000));
    line_items.push_back(LineItemRecord(String::from_str(env, "Item 2"), 3, 1000, 3000));

    InvoiceMetadata {
        customer_name: String::from_str(env, "Acme Corp"),
        customer_address: String::from_str(env, "123 Main St"),
        tax_id: String::from_str(env, "TAX-12345"),
        line_items,
        notes: String::from_str(env, "Thank you"),
    }
}

// ----------------------------------------------------------------------------
// UPDATE AUTHENTICATION
// ----------------------------------------------------------------------------

#[test]
fn test_update_metadata_success() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_business, invoice_id) = create_base_invoice(&env, &client);
    
    let meta = valid_metadata(&env);
    client.update_invoice_metadata(&invoice_id, &meta);

    // Retrieve invoice and check metadata
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.metadata_customer_name, Some(String::from_str(&env, "Acme Corp")));
    assert_eq!(invoice.metadata_tax_id, Some(String::from_str(&env, "TAX-12345")));
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_update_metadata_non_owner_rejected() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_business, invoice_id) = create_base_invoice(&env, &client);
    
    // We clear mock auths and only mock the wrong user
    let wrong_user = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &wrong_user,
        invoke: &soroban_sdk::testutils::MockAuthInvoke { // Mocking any invoke for wrong user
            contract: &client.address,
            fn_name: "update_invoice_metadata",
            args: (&invoice_id, valid_metadata(&env)).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.update_invoice_metadata(&invoice_id, &valid_metadata(&env));
}

// ----------------------------------------------------------------------------
// VALIDATION TESTS
// ----------------------------------------------------------------------------

#[test]
fn test_validation_empty_customer_name() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, invoice_id) = create_base_invoice(&env, &client);
    
    let mut meta = valid_metadata(&env);
    meta.customer_name = String::from_str(&env, ""); // Empty
    
    let res = client.try_update_invoice_metadata(&invoice_id, &meta);
    assert_eq!(res.unwrap_err().unwrap(), QuickLendXError::InvalidDescription);
}

#[test]
fn test_validation_empty_customer_address() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, invoice_id) = create_base_invoice(&env, &client);
    
    let mut meta = valid_metadata(&env);
    meta.customer_address = String::from_str(&env, ""); // Empty
    
    let res = client.try_update_invoice_metadata(&invoice_id, &meta);
    assert_eq!(res.unwrap_err().unwrap(), QuickLendXError::InvalidDescription);
}

#[test]
fn test_validation_empty_tax_id() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, invoice_id) = create_base_invoice(&env, &client);
    
    let mut meta = valid_metadata(&env);
    meta.tax_id = String::from_str(&env, ""); // Empty
    
    let res = client.try_update_invoice_metadata(&invoice_id, &meta);
    assert_eq!(res.unwrap_err().unwrap(), QuickLendXError::InvalidDescription);
}

#[test]
fn test_validation_empty_line_items() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, invoice_id) = create_base_invoice(&env, &client);
    
    let mut meta = valid_metadata(&env);
    meta.line_items = Vec::new(&env); // Empty
    
    let res = client.try_update_invoice_metadata(&invoice_id, &meta);
    assert_eq!(res.unwrap_err().unwrap(), QuickLendXError::InvalidDescription);
}

#[test]
fn test_validation_invalid_line_item_desc() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, invoice_id) = create_base_invoice(&env, &client);
    
    let mut meta = valid_metadata(&env);
    let mut bad_items = Vec::new(&env);
    bad_items.push_back(LineItemRecord(String::from_str(&env, ""), 5, 1000, 5000));
    meta.line_items = bad_items;
    
    let res = client.try_update_invoice_metadata(&invoice_id, &meta);
    assert_eq!(res.unwrap_err().unwrap(), QuickLendXError::InvalidDescription);
}

#[test]
fn test_validation_invalid_line_item_qty_price() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, invoice_id) = create_base_invoice(&env, &client);
    
    let mut meta = valid_metadata(&env);
    let mut bad_items = Vec::new(&env);
    // Qty cannot be 0
    bad_items.push_back(LineItemRecord(String::from_str(&env, "Item"), 0, 5000, 0));
    meta.line_items = bad_items;
    
    let res = client.try_update_invoice_metadata(&invoice_id, &meta);
    assert_eq!(res.unwrap_err().unwrap(), QuickLendXError::InvalidAmount);

    let mut meta2 = valid_metadata(&env);
    let mut bad_items2 = Vec::new(&env);
    // Price cannot be negative
    bad_items2.push_back(LineItemRecord(String::from_str(&env, "Item"), 1, -5000, -5000));
    meta2.line_items = bad_items2;
    
    let res2 = client.try_update_invoice_metadata(&invoice_id, &meta2);
    assert_eq!(res2.unwrap_err().unwrap(), QuickLendXError::InvalidAmount);
}

#[test]
fn test_validation_mismatched_computation() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, invoice_id) = create_base_invoice(&env, &client);
    
    let mut meta = valid_metadata(&env);
    let mut bad_items = Vec::new(&env);
    // 2 * 2000 != 5000 (total claimed)
    bad_items.push_back(LineItemRecord(String::from_str(&env, "Item"), 2, 2000, 5000));
    meta.line_items = bad_items;
    
    let res = client.try_update_invoice_metadata(&invoice_id, &meta);
    assert_eq!(res.unwrap_err().unwrap(), QuickLendXError::InvalidAmount);
}

#[test]
fn test_validation_mismatched_invoice_total() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, invoice_id) = create_base_invoice(&env, &client); // invoice total is 5000
    
    let mut meta = valid_metadata(&env);
    let mut items = Vec::new(&env);
    // total is 6000, which doesn't match invoice amount 5000
    items.push_back(LineItemRecord(String::from_str(&env, "Item"), 2, 3000, 6000));
    meta.line_items = items;
    
    let res = client.try_update_invoice_metadata(&invoice_id, &meta);
    assert_eq!(res.unwrap_err().unwrap(), QuickLendXError::InvoiceAmountInvalid);
}

// ----------------------------------------------------------------------------
// CLEAR METADATA
// ----------------------------------------------------------------------------

#[test]
fn test_clear_metadata_success() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, invoice_id) = create_base_invoice(&env, &client);
    
    client.update_invoice_metadata(&invoice_id, &valid_metadata(&env));
    assert!(client.get_invoice(&invoice_id).metadata_customer_name.is_some());

    client.clear_invoice_metadata(&invoice_id);
    assert!(client.get_invoice(&invoice_id).metadata_customer_name.is_none());
}

#[test]
fn test_clear_metadata_no_op() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, invoice_id) = create_base_invoice(&env, &client);
    
    // Invoice has no metadata yet, clearing should succeed without error
    client.clear_invoice_metadata(&invoice_id);
    assert!(client.get_invoice(&invoice_id).metadata_customer_name.is_none());
}

// ----------------------------------------------------------------------------
// INDEX UPDATES & RETRIEVAL
// ----------------------------------------------------------------------------

#[test]
fn test_get_invoices_by_customer_and_tax_id() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, inv1) = create_base_invoice(&env, &client);
    let (_, inv2) = create_base_invoice(&env, &client);
    
    // Both use the same customer & tax ID
    let meta = valid_metadata(&env);
    client.update_invoice_metadata(&inv1, &meta);
    client.update_invoice_metadata(&inv2, &meta);

    let customer_name = String::from_str(&env, "Acme Corp");
    let tax_id = String::from_str(&env, "TAX-12345");

    let by_customer = client.get_invoices_by_customer(&customer_name);
    assert_eq!(by_customer.len(), 2);
    assert!(by_customer.contains(&inv1));
    assert!(by_customer.contains(&inv2));

    let by_tax = client.get_invoices_by_tax_id(&tax_id);
    assert_eq!(by_tax.len(), 2);
}

#[test]
fn test_index_removed_on_metadata_update() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, inv1) = create_base_invoice(&env, &client);
    
    client.update_invoice_metadata(&inv1, &valid_metadata(&env));

    let old_cust = String::from_str(&env, "Acme Corp");
    assert_eq!(client.get_invoices_by_customer(&old_cust).len(), 1);

    // Update to new customer
    let mut new_meta = valid_metadata(&env);
    new_meta.customer_name = String::from_str(&env, "Globex");
    
    client.update_invoice_metadata(&inv1, &new_meta);

    // Old index should be gone
    assert_eq!(client.get_invoices_by_customer(&old_cust).len(), 0);
    // New index should exist
    let new_cust = String::from_str(&env, "Globex");
    assert_eq!(client.get_invoices_by_customer(&new_cust).len(), 1);
}

#[test]
fn test_index_removed_on_clear() {
    let env = Env::default();
    let (client, _) = setup_contract(&env);
    let (_, inv1) = create_base_invoice(&env, &client);
    
    client.update_invoice_metadata(&inv1, &valid_metadata(&env));

    let cust = String::from_str(&env, "Acme Corp");
    let tax = String::from_str(&env, "TAX-12345");
    
    assert_eq!(client.get_invoices_by_customer(&cust).len(), 1);
    assert_eq!(client.get_invoices_by_tax_id(&tax).len(), 1);

    client.clear_invoice_metadata(&inv1);

    // Indexes should be empty
    assert_eq!(client.get_invoices_by_customer(&cust).len(), 0);
    assert_eq!(client.get_invoices_by_tax_id(&tax).len(), 0);
}
