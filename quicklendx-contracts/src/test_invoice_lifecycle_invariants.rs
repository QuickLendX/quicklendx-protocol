//! State-transition invariants for the invoice lifecycle admin entry point.
//!
//! QE-2026-08 / issue #2433. `update_invoice_status` is the last weakly-guarded
//! place an invoice status can be changed without going through the domain
//! flows (`accept_bid`, payment settlement, `do_mark_invoice_defaulted`). These
//! tests pin the legal transition matrix and prove that rejected, stale,
//! repeated, skipped, and out-of-order transitions leave no partial or
//! unauthorized state (invoice fields and the per-status index stay intact).
//!
//! Legal matrix enforced:
//!   Pending -> Verified -> Funded -> Paid
//! Everything else (backwards, skipped, repeated, terminal) must return
//! `InvalidStatus` with zero side effects. `Defaulted` may only be entered via
//! the centralized `do_mark_invoice_defaulted` path, not through the raw matrix.

use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

const INV_AMOUNT: i128 = 1_000;

fn setup(env: &Env) -> (QuickLendXContractClient<'_>, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    env.ledger().set_timestamp(1);
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.set_admin(&admin);
    (client, admin, contract_id)
}

fn kyc_business(env: &Env, client: &QuickLendXContractClient, admin: &Address, biz: &Address) {
    client.submit_kyc_application(biz, &String::from_str(env, "lifecycle KYC"));
    client.verify_business(admin, biz);
}

/// Register a real Stellar Asset Contract and seed the contract account so
/// token-instance / decimal lookups used by the currency precision guard work.
fn mint_currency(env: &Env, contract_id: &Address, business: &Address) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let bal = 10_000_000i128;
    sac.mint(business, &bal);
    // Ensure the contract has a token instance entry so balance/decimals
    // lookups don't fail with "missing value".
    sac.mint(contract_id, &1i128);
    currency
}

fn upload_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    biz: &Address,
    currency: &Address,
) -> BytesN<32> {
    let due = env.ledger().timestamp() + 86_400;
    client.upload_invoice(
        biz,
        &INV_AMOUNT,
        currency,
        &due,
        &String::from_str(env, "lifecycle invariant"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
        &None,
        &None,
    )
}

/// The full legal matrix plus every illegal class the issue calls out (stale,
/// repeated, skipped, backward, and terminal-immutable transitions), with the
/// per-status index checked after every step so no partial state slips through.
#[test]
fn update_invoice_status_enforces_full_lifecycle_matrix() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz);
    kyc_business(&env, &client, &admin, &biz);
    let id = upload_invoice(&env, &client, &biz, &currency);

    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Pending);

    // Skipped: Pending -> Funded (no verification).
    assert!(matches!(
        client.try_update_invoice_status(&id, &InvoiceStatus::Funded),
        Err(Ok(QuickLendXError::InvalidStatus))
    ));

    // Skipped: Pending -> Paid (settlement before funding).
    assert!(matches!(
        client.try_update_invoice_status(&id, &InvoiceStatus::Paid),
        Err(Ok(QuickLendXError::InvalidStatus))
    ));

    // Nothing moved: still Pending, indexed under Pending only.
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Pending);
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Pending),
        1
    );
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Verified),
        0
    );
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Funded),
        0
    );
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Paid), 0);

    // Legal: Pending -> Verified.
    client.update_invoice_status(&id, &InvoiceStatus::Verified);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Verified);
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Pending),
        0
    );
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Verified),
        1
    );

    // Repeated: Verified -> Verified must be rejected without index drift.
    assert!(matches!(
        client.try_update_invoice_status(&id, &InvoiceStatus::Verified),
        Err(Ok(QuickLendXError::InvalidStatus))
    ));
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Verified),
        1
    );
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Pending),
        0
    );

    // Backward: Verified -> Pending must be rejected.
    assert!(matches!(
        client.try_update_invoice_status(&id, &InvoiceStatus::Pending),
        Err(Ok(QuickLendXError::InvalidStatus))
    ));
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Verified);

    // Legal: Verified -> Funded.
    client.update_invoice_status(&id, &InvoiceStatus::Funded);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Funded);
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Verified),
        0
    );
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Funded),
        1
    );

    // Backward: Funded -> Verified rejected.
    assert!(matches!(
        client.try_update_invoice_status(&id, &InvoiceStatus::Verified),
        Err(Ok(QuickLendXError::InvalidStatus))
    ));
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Funded);

    // Legal: Funded -> Paid (terminal).
    client.update_invoice_status(&id, &InvoiceStatus::Paid);
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Paid);
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Paid), 1);
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Funded),
        0
    );

    // Terminal immutability: Paid may not be re-opened in any direction.
    assert!(matches!(
        client.try_update_invoice_status(&id, &InvoiceStatus::Funded),
        Err(Ok(QuickLendXError::InvalidStatus))
    ));
    assert!(matches!(
        client.try_update_invoice_status(&id, &InvoiceStatus::Verified),
        Err(Ok(QuickLendXError::InvalidStatus))
    ));
    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Paid);
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Paid), 1);
    assert_eq!(
        client.get_invoice_count_by_status(&InvoiceStatus::Funded),
        0
    );
}

/// The financial-safety core: a rejected transition must not mutate asset-level
/// fields. On the old behaviour, `Pending -> Paid` would have run
/// `mark_as_paid` (setting `total_paid`/`settled_at`) and `Paid -> Funded`
/// would have re-opened funding (setting `funded_amount`/`investor`) on a
/// settled invoice, mispricing exposure. Both must now be atomic no-ops.
#[test]
fn rejected_transition_leaves_no_partial_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, cid) = setup(&env);
    let biz = Address::generate(&env);
    let currency = mint_currency(&env, &cid, &biz);
    kyc_business(&env, &client, &admin, &biz);
    let id = upload_invoice(&env, &client, &biz, &currency);

    // Settlement-before-funding must not set any payment fields.
    assert!(matches!(
        client.try_update_invoice_status(&id, &InvoiceStatus::Paid),
        Err(Ok(QuickLendXError::InvalidStatus))
    ));
    let invoice = client.get_invoice(&id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
    assert_eq!(invoice.total_paid, 0);
    assert!(invoice.settled_at.is_none());
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());

    // A settled invoice cannot be re-opened to Funded (would reactivate
    // exposure after settlement and could misprice the obligation).
    client.update_invoice_status(&id, &InvoiceStatus::Verified);
    client.update_invoice_status(&id, &InvoiceStatus::Funded);
    client.update_invoice_status(&id, &InvoiceStatus::Paid);
    let paid_invoice = client.get_invoice(&id);
    assert_eq!(paid_invoice.status, InvoiceStatus::Paid);
    assert_eq!(paid_invoice.total_paid, INV_AMOUNT);
    assert!(paid_invoice.settled_at.is_some());

    assert!(matches!(
        client.try_update_invoice_status(&id, &InvoiceStatus::Funded),
        Err(Ok(QuickLendXError::InvalidStatus))
    ));
    let after = client.get_invoice(&id);
    assert_eq!(after.status, InvoiceStatus::Paid);
    assert_eq!(after.total_paid, INV_AMOUNT);
    assert_eq!(after.funded_amount, INV_AMOUNT);
    assert!(after.settled_at.is_some());
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Paid), 1);
}
