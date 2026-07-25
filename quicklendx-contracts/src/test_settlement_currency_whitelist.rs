//! Negative regression tests: settlement is blocked when the per-invoice
//! settlement currency whitelist does not include `invoice.currency`.
//!
//! # Security gap being closed (defence-in-depth)
//!
//! `settle_invoice_internal()` previously had no check on the settlement
//! currency — it used `invoice.currency` unconditionally.  This meant that
//! if `invoice.currency` were corrupted in stored state (or if a future
//! refactor introduced a caller-supplied settlement currency), an invoice
//! could be settled in an unexpected token.
//!
//! ## Threat model
//!
//! An attacker who can manipulate stored invoice state (or a future code
//! path that accepts a caller-specified settlement currency) could cause an
//! invoice funded in Token A to be settled in Token B, potentially at a
//! different valuation, enabling:
//!
//! 1. **Value extraction** — settle a high-value invoice with a low-value
//!    token, defrauding the investor of the expected return.
//! 2. **Compliance bypass** — settle in a token that was deliberately
//!    excluded (e.g. a sanctioned or frozen token).
//! 3. **Accounting drift** — create an inconsistency between the funded
//!    currency and the settlement currency that off-chain reconciliation
//!    cannot resolve.
//!
//! ## Fix
//!
//! `settle_invoice_internal()` now checks a per-invoice settlement currency
//! whitelist stored at invoice creation time.  If the whitelist is non-empty
//! and does not contain `invoice.currency`, settlement is rejected with
//! `SettlementCurrencyNotAllowed`.
//!
//! ## Test matrix
//!
//! | Test                                                           | Whitelist contains invoice currency | Expected                     |
//! |----------------------------------------------------------------|--------------------------------------|------------------------------|
//! | `test_settlement_blocked_when_whitelist_does_not_match`        | No                                   | `SettlementCurrencyNotAllowed` |
//! | `test_settlement_succeeds_with_default_whitelist`              | Yes (default)                        | `Ok(())`                     |

use super::*;

use crate::errors::QuickLendXError;
use crate::types::InvoiceCategory;
use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

/// Boot a full contract instance and return a `Funded` invoice.
fn setup_funded_invoice(
    env: &Env,
) -> (
    QuickLendXContractClient,
    BytesN<32>,
    Address,
    Address,
    Address,
) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let business = Address::generate(env);
    let investor = Address::generate(env);

    client.set_admin(&admin);
    client.initialize_fee_system(&admin);

    // Create a real Stellar Asset Contract so token transfers succeed.
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(env, &currency);
    let sac = token::StellarAssetClient::new(env, &currency);

    let balance: i128 = 500_000;
    sac.mint(&business, &balance);
    sac.mint(&investor, &balance);

    let expiry = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &balance, &expiry);
    token_client.approve(&investor, &contract_id, &balance, &expiry);

    // KYC — both parties must be verified.
    client.submit_kyc_application(&business, &String::from_str(env, "business-kyc"));
    client.verify_business(&admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(env, "investor-kyc"));
    client.verify_investor(&investor, &balance);

    // Create invoice → verify → bid → accept (funds escrowed, invoice = Funded).
    let amount: i128 = 100_000;
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Settlement currency whitelist test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &amount);
    client.accept_bid(&invoice_id, &bid_id);

    (client, invoice_id, business, investor, contract_id)
}

/// Settlement MUST be blocked when the per-invoice settlement currency
/// whitelist has been replaced with a list that does NOT include the
/// invoice's own currency.
///
/// Before the fix: there was no check, so settlement would succeed.
/// After the fix: `settle_invoice_internal` verifies the whitelist and
/// returns `Err(SettlementCurrencyNotAllowed)`.
#[test]
fn test_settlement_blocked_when_whitelist_does_not_match() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, invoice_id, _business, _investor, contract_id) = setup_funded_invoice(&env);

    // Verify pre-condition: invoice is Funded and ready to settle.
    let inv = client.get_invoice(&invoice_id);
    assert_eq!(
        inv.status,
        crate::types::InvoiceStatus::Funded,
        "pre-condition: invoice must be Funded"
    );

    // ── Override settlement currencies whitelist ──────────────────────────
    // Create a different dummy token and set it as the ONLY allowed
    // settlement currency.  The invoice was created with `currency`, so
    // this whitelist does NOT include it.
    let other_token = Address::generate(&env);
    let mut bad_whitelist: Vec<Address> = Vec::new(&env);
    bad_whitelist.push_back(other_token);
    crate::settlement::store_settlement_currencies(&env, &invoice_id, &bad_whitelist);

    // Attempt full settlement — MUST FAIL.
    let result = client.try_settle_invoice(&invoice_id, &100_000i128);

    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::SettlementCurrencyNotAllowed,
        "settle_invoice must return SettlementCurrencyNotAllowed when the \
         whitelist does not include invoice.currency"
    );

    // Invoice must still be Funded — no funds moved, no terminal state.
    let inv_after = client.get_invoice(&invoice_id);
    assert_eq!(
        inv_after.status,
        crate::types::InvoiceStatus::Funded,
        "invoice must remain Funded after failed settlement"
    );
}

/// Settlement MUST succeed when the default whitelist (set at invoice
/// creation) contains the invoice's own currency.
#[test]
fn test_settlement_succeeds_with_default_whitelist() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, invoice_id, _business, _investor, _contract_id) = setup_funded_invoice(&env);

    // Verify pre-condition: invoice is Funded.
    let inv = client.get_invoice(&invoice_id);
    assert_eq!(
        inv.status,
        crate::types::InvoiceStatus::Funded,
        "pre-condition: invoice must be Funded"
    );

    // Full settlement with default whitelist — MUST SUCCEED.
    let result = client.try_settle_invoice(&invoice_id, &100_000i128);

    assert!(
        result.is_ok(),
        "settle_invoice must succeed with default whitelist: {:?}",
        result.err()
    );

    // Invoice must now be Paid.
    let inv_after = client.get_invoice(&invoice_id);
    assert_eq!(
        inv_after.status,
        crate::types::InvoiceStatus::Paid,
        "invoice must transition to Paid after successful settlement"
    );
}
