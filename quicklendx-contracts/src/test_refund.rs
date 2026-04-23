/// Test suite for escrow refund flow
///
/// Test Coverage:
/// 1. Authorization: Only admin or business owner can trigger a refund
/// 2. State Validation: Only funded invoices can be refunded
/// 3. Token Transfer: Funds are returned to the correct investor
/// 4. State Transitions: Invoice, Bid, Investment, and Escrow statuses are correctly updated
/// 5. Security: Unauthorized callers cannot trigger refunds
use super::*;
use crate::invoice::InvoiceCategory;
use crate::payments::EscrowStatus;
#[cfg(test)]
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

// ============================================================================
// Helper Functions (Reused from test_escrow.rs pattern)
// ============================================================================

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn setup_token(
    env: &Env,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);

    let initial_balance = 100_000i128;
    sac_client.mint(business, &initial_balance);
    sac_client.mint(investor, &initial_balance);

    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(business, contract_id, &initial_balance, &expiration);
    token_client.approve(investor, contract_id, &initial_balance, &expiration);

    currency
}

fn setup_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn setup_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

fn create_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> (BytesN<32>, Address, Address, i128, Address) {
    let business = setup_verified_business(env, client, admin);
    let investor = setup_verified_investor(env, client, 50_000);
    let currency = setup_token(env, &business, &investor, &client.address);

    let amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 1000));
    client.accept_bid(&invoice_id, &bid_id);

    (invoice_id, business, investor, amount, currency)
}

// ============================================================================
// Test Cases
// ============================================================================

#[test]
fn test_business_can_trigger_refund() {
    let (env, client, admin) = setup();
    let (invoice_id, business, investor, amount, currency) =
        create_funded_invoice(&env, &client, &admin);
    let token_client = token::Client::new(&env, &currency);

    let investor_balance_before = token_client.balance(&investor);

    // Business owner triggers refund
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(
        result.is_ok(),
        "Business owner should be able to trigger refund"
    );

    // Verify investor received funds back
    let investor_balance_after = token_client.balance(&investor);
    assert_eq!(investor_balance_after - investor_balance_before, amount);

    // Verify state transitions
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Refunded);

    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);

    env.as_contract(&client.address, || {
        let investment =
            crate::investment::InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
                .unwrap();
        assert_eq!(investment.status, InvestmentStatus::Refunded);
    });

    let bid = client.get_bids_for_invoice(&invoice_id).get(0).unwrap();
    assert_eq!(bid.status, crate::bid::BidStatus::Cancelled);
}

#[test]
fn test_admin_can_trigger_refund() {
    let (env, client, admin) = setup();
    let (invoice_id, _, investor, amount, currency) = create_funded_invoice(&env, &client, &admin);
    let token_client = token::Client::new(&env, &currency);

    let investor_balance_before = token_client.balance(&investor);

    // Admin triggers refund
    let result = client.try_refund_escrow_funds(&invoice_id, &admin);
    assert!(result.is_ok(), "Admin should be able to trigger refund");

    // Verify investor received funds back
    let investor_balance_after = token_client.balance(&investor);
    assert_eq!(investor_balance_after - investor_balance_before, amount);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Refunded);
}

#[test]
fn test_unauthorized_caller_cannot_trigger_refund() {
    let (env, client, admin) = setup();
    let (invoice_id, _, _, _, _) = create_funded_invoice(&env, &client, &admin);
    let stranger = Address::generate(&env);

    // Stranger tries to trigger refund
    let result = client.try_refund_escrow_funds(&invoice_id, &stranger);
    assert!(
        result.is_err(),
        "Stranger should not be able to trigger refund"
    );

    // Verify invoice is still Funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
}

#[test]
fn test_cannot_refund_unfunded_invoice() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);

    let amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Test Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    // Invoice is Verified but not Funded
    let result = client.try_refund_escrow_funds(&invoice_id, &admin);
    assert!(result.is_err(), "Cannot refund an unfunded invoice");
}

#[test]
fn test_cannot_refund_twice() {
    let (env, client, admin) = setup();
    let (invoice_id, business, _, _, _) = create_funded_invoice(&env, &client, &admin);

    // First refund
    client.refund_escrow_funds(&invoice_id, &business);

    // Second refund attempt
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(result.is_err(), "Cannot refund an already refunded invoice");
}

#[test]
fn test_cannot_refund_nonexistent_invoice() {
    let (env, client, admin) = setup();

    // Generate a random invoice ID that doesn't exist
    let nonexistent_invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    // Attempt to refund
    let result = client.try_refund_escrow_funds(&nonexistent_invoice_id, &admin);

    // Verify it returns an error
    assert!(result.is_err(), "Cannot refund a nonexistent invoice");
}

#[test]
fn test_cannot_refund_missing_escrow() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);

    let amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 86400;

    // Create and verify an invoice
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Test Missing Escrow"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    // Forcibly update status to Funded, skipping the bid process (no escrow record created)
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Funded);

    // Attempt to refund should fail because there is no corresponding escrow record
    let result = client.try_refund_escrow_funds(&invoice_id, &admin);
    assert!(
        result.is_err(),
        "Cannot refund an invoice if the escrow record is missing"
    );
}

#[test]
fn test_refund_updates_internal_states_correctly() {
    let (env, client, admin) = setup();
    let (invoice_id, business, _investor, _amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    // Pre-refund state verification
    let pre_refund_invoice = client.get_invoice(&invoice_id);
    assert_eq!(pre_refund_invoice.status, InvoiceStatus::Funded);

    // Status list tracking count check before refund
    let pre_refunded_count = client.get_invoice_count_by_status(&InvoiceStatus::Refunded);

    // Perform the refund
    client.refund_escrow_funds(&invoice_id, &business);

    // 1. Invoice Status should update to Refunded
    let post_refund_invoice = client.get_invoice(&invoice_id);
    assert_eq!(post_refund_invoice.status, InvoiceStatus::Refunded);

    // 2. Invoice Status tracking lists should be updated correctly
    let post_refunded_count = client.get_invoice_count_by_status(&InvoiceStatus::Refunded);

    assert_eq!(post_refunded_count, pre_refunded_count + 1);

    // 3. Bid status should update to Cancelled
    let bids = client.get_bids_for_invoice(&invoice_id);
    assert_eq!(bids.len(), 1);
    assert_eq!(
        bids.get(0).unwrap().status,
        crate::bid::BidStatus::Cancelled
    );

    // 4. Investment status should update to Refunded
    env.as_contract(&client.address, || {
        let investment =
            crate::investment::InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
                .unwrap();
        assert_eq!(investment.status, InvestmentStatus::Refunded);
    });
}

// ============================================================================
// Token Transfer Failure Tests – Refund Path
//
// These tests document and verify the contract's behavior when the underlying
// Stellar token transfer fails during a refund. In every failure case:
//   - The escrow status remains `Held` (retryable).
//   - Invoice, bid, and investment states are left unchanged.
//   - The correct error variant is returned.
// ============================================================================

/// `refund_escrow_funds` fails with `InsufficientFunds` when the contract's
/// token balance has been drained externally (invariant violation scenario).
///
/// # Security note
/// The balance check in `transfer_funds` runs before the token call, so the
/// escrow status is never updated to `Refunded` and the operation is retryable.
#[test]
fn test_refund_fails_when_contract_has_insufficient_balance() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let (invoice_id, business, investor, amount, currency) =
        create_funded_invoice(&env, &client, &admin);

    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    // Drain the contract's balance to simulate an invariant violation.
    // We do this by burning the contract's tokens directly via the SAC admin.
    let contract_balance = token_client.balance(&contract_id);
    // Burn all contract tokens (SAC burn requires the holder to auth; use mock_all_auths).
    sac_client.burn(&contract_id, &contract_balance);

    assert_eq!(
        token_client.balance(&contract_id),
        0,
        "Contract balance should be zero after burn"
    );

    let investor_balance_before = token_client.balance(&investor);

    // Refund should fail because the contract has no balance to return.
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(
        result.is_err(),
        "refund_escrow_funds must fail when contract has no balance"
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::InsufficientFunds,
        "Expected InsufficientFunds error"
    );

    // No funds moved to investor.
    assert_eq!(
        token_client.balance(&investor),
        investor_balance_before,
        "Investor balance must not change on failed refund"
    );

    // Escrow status must remain Held (retryable).
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(
        escrow.status,
        EscrowStatus::Held,
        "Escrow must remain Held after failed refund"
    );

    // Invoice must remain Funded.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Funded,
        "Invoice must remain Funded after failed refund"
    );
}

/// After a failed refund (due to drained contract balance), the refund succeeds
/// once the contract balance is restored.
///
/// This verifies that the escrow `Held` state is truly retryable.
#[test]
fn test_refund_succeeds_after_balance_restored() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();

    let (invoice_id, business, investor, amount, currency) =
        create_funded_invoice(&env, &client, &admin);

    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    // Drain contract balance.
    let contract_balance = token_client.balance(&contract_id);
    sac_client.burn(&contract_id, &contract_balance);

    // First refund attempt fails.
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::InsufficientFunds
    );

    // Restore contract balance by minting directly to the contract address.
    sac_client.mint(&contract_id, &amount);

    let investor_balance_before = token_client.balance(&investor);

    // Second refund attempt succeeds.
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(
        result.is_ok(),
        "refund should succeed after balance restored"
    );

    // Investor received funds.
    assert_eq!(
        token_client.balance(&investor),
        investor_balance_before + amount
    );

    // Escrow is now Refunded.
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);

    // Invoice is now Refunded.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Refunded);
}

// ============================================================================
// Finality and cross-state ordering tests
//
// These tests verify that refund handling is final and cannot double-account
// with defaults, settlements, or repeated refunds. See
// `docs/contracts/defaults.md` for the full cross-state ordering rules.
// ============================================================================

/// Defaulted invoices cannot be refunded. Once the default transition has
/// occurred, the invoice status is `Defaulted` (not `Funded`), so
/// `refund_escrow_funds` rejects the call before any funds move. The escrow
/// record must remain `Held` so it can be handled through the separate
/// insurance-claim path.
#[test]
fn test_cannot_refund_defaulted_invoice() {
    let (env, client, admin) = setup();
    let (invoice_id, business, _investor, _amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    // Move time past due_date + grace period and mark the invoice defaulted.
    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);
    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );

    // Refund must now be rejected with InvalidStatus.
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);

    // Invoice still Defaulted.
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );

    // Escrow must remain Held so the insurance/claim flow can still resolve
    // it — the default path never refunds automatically.
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(
        escrow.status,
        EscrowStatus::Held,
        "escrow must remain Held after a rejected refund on a defaulted invoice"
    );
}

/// Paid (settled) invoices cannot be refunded. Finality rule: once settlement
/// moves the invoice to `Paid`, refund is rejected before any funds move, so
/// the investor cannot be paid out twice (once via settlement profit,
/// once via escrow refund).
#[test]
fn test_cannot_refund_paid_invoice() {
    let (env, client, admin) = setup();
    let (invoice_id, business, _investor, amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    // Settle the invoice. `setup_token` already mints 100_000 to the business,
    // which is enough to cover the 10_000 settlement plus platform fee.
    client.settle_invoice(&invoice_id, &amount);
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Paid);

    // Refund after settlement must be rejected.
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);

    // Invoice still Paid.
    assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Paid);
}

/// A second refund attempt after a successful first refund must not move
/// any tokens and must not mutate the already-Refunded state. This is the
/// canonical "no double refund" guarantee: the investor's on-chain balance
/// must increase by exactly `amount`, never by `2 * amount`.
#[test]
fn test_double_refund_does_not_double_transfer_funds() {
    let (env, client, admin) = setup();
    let (invoice_id, business, investor, amount, currency) =
        create_funded_invoice(&env, &client, &admin);
    let token_client = token::Client::new(&env, &currency);

    let investor_balance_before = token_client.balance(&investor);

    // First refund succeeds.
    client.refund_escrow_funds(&invoice_id, &business);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Refunded
    );
    assert_eq!(
        token_client.balance(&investor),
        investor_balance_before + amount,
        "first refund must credit the investor exactly once"
    );

    // Second refund attempt must be rejected with InvalidStatus (invoice
    // is no longer Funded).
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);

    // Investor balance must not have doubled.
    assert_eq!(
        token_client.balance(&investor),
        investor_balance_before + amount,
        "investor balance must not change across a rejected second refund"
    );

    // Escrow record must still be Refunded (not re-processed).
    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);
}

/// Refunded invoices cannot be settled. Finality rule: once the invoice is
/// `Refunded`, `settle_invoice` rejects the call with `InvalidStatus` before
/// any funds move or any payment record is stored.
#[test]
fn test_cannot_settle_refunded_invoice() {
    let (env, client, admin) = setup();
    let (invoice_id, business, _investor, amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    client.refund_escrow_funds(&invoice_id, &business);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Refunded
    );

    // Settle attempt on a refunded invoice must fail.
    let result = client.try_settle_invoice(&invoice_id, &amount);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);

    // Invoice still Refunded.
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Refunded
    );
}

/// Refunded invoices cannot be defaulted, even after the grace period has
/// expired. This is the refund-side counterpart of
/// `test_default::test_cannot_default_refunded_invoice`; keeping it in both
/// test files gives clean independent regression signals per module.
#[test]
fn test_cannot_default_refunded_invoice_crossmodule() {
    let (env, client, admin) = setup();
    let (invoice_id, business, _investor, _amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    // Move the invoice to Refunded.
    client.refund_escrow_funds(&invoice_id, &business);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Refunded
    );

    // Move time past due_date + grace period to isolate the status check
    // from the grace-period check.
    let invoice = client.get_invoice(&invoice_id);
    let grace_period = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);

    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::InvoiceNotAvailableForFunding
    );

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Refunded
    );
}
