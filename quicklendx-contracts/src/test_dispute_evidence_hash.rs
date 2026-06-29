use super::*;
use crate::errors::QuickLendXError;
use crate::types::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

fn create_disputed_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let amount = 1000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let description = String::from_str(env, "Test invoice");

    client.set_admin(admin);
    client.submit_kyc_application(business, &String::from_str(env, "KYC data"));
    client.verify_business(admin, business);

    let invoice_id = client.upload_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &description,
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    let reason = String::from_str(env, "Payment issue");
    let evidence = String::from_str(env, "Payment evidence");
    client.create_dispute(&invoice_id, business, &reason, &evidence);

    invoice_id
}

#[test]
fn attach_evidence_hash_records_hash() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = create_disputed_invoice(&env, &client, &admin, &business);
    let hash = BytesN::from_array(&env, &[7u8; 32]);

    client.attach_evidence_hash(&invoice_id, &business, &hash);

    let dispute = client.get_dispute_details(&invoice_id).unwrap();
    assert_eq!(dispute.evidence_hash, Some(hash));
}

#[test]
fn attach_evidence_hash_rejects_second_hash() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = create_disputed_invoice(&env, &client, &admin, &business);
    let first_hash = BytesN::from_array(&env, &[7u8; 32]);
    let second_hash = BytesN::from_array(&env, &[8u8; 32]);

    client.attach_evidence_hash(&invoice_id, &business, &first_hash);
    let result = client.try_attach_evidence_hash(&invoice_id, &business, &second_hash);

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::InvalidDisputeEvidence
    );
}

#[test]
fn attach_evidence_hash_rejects_non_creator() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let other = Address::generate(&env);
    let invoice_id = create_disputed_invoice(&env, &client, &admin, &business);
    let hash = BytesN::from_array(&env, &[7u8; 32]);

    let result = client.try_attach_evidence_hash(&invoice_id, &other, &hash);

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::DisputeNotAuthorized
    );
}

#[test]
fn attach_evidence_hash_rejects_under_review_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = create_disputed_invoice(&env, &client, &admin, &business);
    let hash = BytesN::from_array(&env, &[7u8; 32]);

    client.put_dispute_under_review(&invoice_id, &admin);
    let result = client.try_attach_evidence_hash(&invoice_id, &business, &hash);

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);
}
