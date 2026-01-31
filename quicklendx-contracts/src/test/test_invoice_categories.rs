use super::*;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

#[test]
fn test_invoice_category_and_tags() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let mut tags1 = Vec::new(&env);
    tags1.push_back(String::from_str(&env, "urgent"));
    tags1.push_back(String::from_str(&env, "tech"));

    // Invoice 1: Services, [urgent, tech]
    let invoice1_id = client.store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &tags1,
    );

    let mut tags2 = Vec::new(&env);
    tags2.push_back(String::from_str(&env, "tech"));

    // Invoice 2: Products, [tech]
    let invoice2_id = client.store_invoice(
        &business,
        &2000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Products,
        &tags2,
    );

    let mut tags3 = Vec::new(&env);
    tags3.push_back(String::from_str(&env, "urgent"));

    // Invoice 3: Services, [urgent]
    let invoice3_id = client.store_invoice(
        &business,
        &3000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 3"),
        &InvoiceCategory::Services,
        &tags3,
    );

    // Test get_invoices_by_category
    let services = client.get_invoices_by_category(&InvoiceCategory::Services);
    assert_eq!(services.len(), 2);
    assert!(services.contains(&invoice1_id));
    assert!(services.contains(&invoice3_id));

    let products = client.get_invoices_by_category(&InvoiceCategory::Products);
    assert_eq!(products.len(), 1);
    assert!(products.contains(&invoice2_id));

    // Test get_invoices_by_tag
    let tech_invoices = client.get_invoices_by_tag(&String::from_str(&env, "tech"));
    assert_eq!(tech_invoices.len(), 2);
    assert!(tech_invoices.contains(&invoice1_id));
    assert!(tech_invoices.contains(&invoice2_id));

    let urgent_invoices = client.get_invoices_by_tag(&String::from_str(&env, "urgent"));
    assert_eq!(urgent_invoices.len(), 2);
    assert!(urgent_invoices.contains(&invoice1_id));
    assert!(urgent_invoices.contains(&invoice3_id));

    // Test add_invoice_tag
    client.add_invoice_tag(&invoice3_id, &String::from_str(&env, "tech"));

    let tech_invoices_updated = client.get_invoices_by_tag(&String::from_str(&env, "tech"));
    assert_eq!(tech_invoices_updated.len(), 3);
    assert!(tech_invoices_updated.contains(&invoice3_id));

    // Test remove_invoice_tag
    client.remove_invoice_tag(&invoice1_id, &String::from_str(&env, "urgent"));

    let urgent_invoices_updated = client.get_invoices_by_tag(&String::from_str(&env, "urgent"));
    assert_eq!(urgent_invoices_updated.len(), 1); // Only invoice3 left
    assert!(urgent_invoices_updated.contains(&invoice3_id));
    assert!(!urgent_invoices_updated.contains(&invoice1_id));
}
