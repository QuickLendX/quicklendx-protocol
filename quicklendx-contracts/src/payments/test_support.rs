//! Shared fixtures for the inline `payments` unit tests.

use soroban_sdk::{token, Address, BytesN, Env};

pub(super) fn contract_env() -> (Env, Address) {
    use crate::QuickLendXContract;
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

/// Store a minimal Pending/Verified invoice record so that
/// `validate_and_prepare_escrow`'s invoice lookup succeeds.
pub(super) fn store_test_invoice(
    env: &Env,
    contract_id: &Address,
    invoice_id: &BytesN<32>,
    business: &Address,
    currency: &Address,
    amount: i128,
) {
    use crate::invoice::{InvoiceCategory, InvoiceStatus};
    let invoice = crate::types::Invoice {
        id: invoice_id.clone(),
        business: business.clone(),
        amount,
        currency: currency.clone(),
        due_date: env.ledger().timestamp() + 86400,
        status: InvoiceStatus::Verified,
        created_at: env.ledger().timestamp(),
        description: soroban_sdk::String::from_str(env, ""),
        metadata_customer_address: None,
        metadata_notes: None,
        metadata_line_items: soroban_sdk::Vec::new(env),
        category: InvoiceCategory::Services,
        tags: soroban_sdk::Vec::new(env),
        investor: None,
        funded_amount: 0,
        funded_at: None,
        settled_at: None,
        average_rating: None,
        total_ratings: 0,
        ratings: soroban_sdk::Vec::new(env),
        dispute_status: crate::types::DisputeStatus::None,
        dispute: crate::types::Dispute {
            created_by: business.clone(),
            created_at: 0,
            reason: soroban_sdk::String::from_str(env, ""),
            evidence: soroban_sdk::String::from_str(env, ""),
            resolution: soroban_sdk::String::from_str(env, ""),
            resolved_by: business.clone(),
            resolved_at: 0,
            resolution_outcome: crate::types::DisputeResolution::None,
            votes: soroban_sdk::Vec::new(env),
        },
        total_paid: 0,
        origination_fee_bps: None,
        late_payment_penalty_bps: None,
        early_payment_discount_bps: None,
        metadata_customer_name: None,
        metadata_tax_id: None,
        payment_history: soroban_sdk::Vec::new(env),
    };
    env.as_contract(contract_id, || {
        crate::storage::InvoiceStorage::store_invoice(env, &invoice);
    });
}

pub(super) fn mint_and_approve(
    env: &Env,
    contract_id: &Address,
    token_admin: &Address,
    holder: &Address,
    balance: i128,
    allowance: i128,
) -> Address {
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(holder, &balance);
    let expiry = env.ledger().sequence() + 10_000;
    tok.approve(holder, contract_id, &allowance, &expiry);
    currency
}
