//! Bid submission, auction-selection, and withdrawal authorization boundaries
//! - issue #2440 (QE-2026-08).
//!
//! These tests pin the authorization contract of the three entry points that
//! mutate bid state on behalf of a participant:
//!
//! | Entry point    | Required signer        | Allowed only while                            |
//! |----------------|------------------------|-----------------------------------------------|
//! | `place_bid`    | the bid's investor     | the invoice is a live, unfrozen auction       |
//! | `accept_bid`   | the invoice's business | the auction is open and the bid is compatible |
//! | `withdraw_bid` | the bid's investor     | the bid is still `Placed`                     |
//!
//! The signer is always the identity recorded in trusted contract state
//! (`bid.investor` for submission and withdrawal, `invoice.business` for
//! selection); no entry point takes an "actor" argument a caller could forge.
//! Every denial case therefore asserts *no mutation* as well: the per-invoice
//! bid index, the bid record, and the auction's selectable winner are read back
//! after the rejected call, proving a refused call leaves no partial state.
//!
//! The `verify_bid_match` group at the end covers the shared compatibility
//! helper directly: it is the single place that decides whether a bid may still
//! fund an invoice, including the invoice face-value ceiling.
//!
//! Fixtures are seeded through storage rather than the KYC/upload entry points
//! so each test isolates the boundary under test.

#![cfg(test)]

use crate::bid::{verify_bid_match, BidStatus, BidStorage};
use crate::errors::QuickLendXError;
use crate::storage::InvoiceStorage;
use crate::types::{Bid, BusinessFreezeReason, Invoice, InvoiceCategory};
use crate::verification::{
    BusinessVerificationStatus, InvestorRiskLevel, InvestorTier, InvestorVerification,
    InvestorVerificationStorage,
};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal, String, Vec,
};

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

const INVOICE_AMOUNT: i128 = 1_000;
const BID_AMOUNT: i128 = 900;
const EXPECTED_RETURN: i128 = 950;
const INVESTOR_LIMIT: i128 = 10_000;

/// Deterministic 32-byte id / salt from a single seed byte.
fn seeded_id(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    BytesN::from_array(env, &bytes)
}

/// Contract plus the three identities the boundaries distinguish between:
/// an admin (fixture only), a business, and a KYC-verified investor.
fn setup() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.ledger().set_timestamp(1_700_000_000);
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // A verified investor with headroom is the only identity allowed to bid.
    env.as_contract(&contract_id, || {
        InvestorVerificationStorage::store(
            &env,
            &InvestorVerification {
                investor: investor.clone(),
                status: BusinessVerificationStatus::Verified,
                verified_at: Some(env.ledger().timestamp()),
                verified_by: Some(admin.clone()),
                kyc_data: String::from_str(&env, "QE-2026-08 investor"),
                investment_limit: INVESTOR_LIMIT,
                submitted_at: env.ledger().timestamp(),
                tier: InvestorTier::Basic,
                risk_level: InvestorRiskLevel::Medium,
                risk_score: 10,
                total_invested: 0,
                total_returns: 0,
                successful_investments: 0,
                defaulted_investments: 0,
                last_activity: env.ledger().timestamp(),
                rejection_reason: None,
                compliance_notes: None,
            },
        );
    });

    (env, client, contract_id, business, investor)
}

/// Seed a `Verified`, unfunded invoice owned by `business`.
fn seed_invoice(env: &Env, contract_id: &Address, business: &Address) -> BytesN<32> {
    env.as_contract(contract_id, || {
        let mut invoice = Invoice::new(
            env,
            business.clone(),
            INVOICE_AMOUNT,
            Address::generate(env),
            env.ledger().timestamp() + 86_400,
            String::from_str(env, "QE-2026-08 invoice"),
            InvoiceCategory::Services,
            Vec::new(env),
            None,
            None,
            None,
        )
        .expect("invoice fixture must build");
        invoice.verify(env, business.clone());
        let invoice_id = invoice.id.clone();
        InvoiceStorage::store_invoice(env, &invoice);
        invoice_id
    })
}

/// Seed a `Placed` bid and index it under its invoice (the auction ranking and
/// winner selection read the auction through that index).
fn seed_bid(
    env: &Env,
    contract_id: &Address,
    investor: &Address,
    invoice_id: &BytesN<32>,
    bid_amount: i128,
    seed: u8,
) -> BytesN<32> {
    env.as_contract(contract_id, || {
        let bid = Bid {
            bid_id: seeded_id(env, seed),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount,
            expected_return: bid_amount + 50,
            timestamp: env.ledger().timestamp(),
            status: BidStatus::Placed,
            expiration_timestamp: env.ledger().timestamp() + 86_400,
        };
        BidStorage::store_bid(env, &bid);
        BidStorage::add_bid_to_invoice(env, invoice_id, &bid.bid_id);
        bid.bid_id
    })
}

fn freeze(env: &Env, contract_id: &Address, invoice_id: &BytesN<32>) {
    env.as_contract(contract_id, || {
        InvoiceStorage::set_frozen(
            env,
            invoice_id,
            true,
            Some(BusinessFreezeReason::AdminAction),
        );
    });
}

fn bid_status(client: &QuickLendXContractClient, bid_id: &BytesN<32>) -> BidStatus {
    client.get_bid(bid_id).expect("bid must exist").status
}

fn indexed_bids(client: &QuickLendXContractClient, invoice_id: &BytesN<32>) -> u32 {
    client.get_bids_for_invoice(invoice_id).len()
}

/// Mock exactly one signer for exactly one invocation.
fn only_auth(
    env: &Env,
    contract_id: &Address,
    signer: &Address,
    fn_name: &str,
    args: soroban_sdk::Vec<soroban_sdk::Val>,
) {
    env.mock_auths(&[MockAuth {
        address: signer,
        invoke: &MockAuthInvoke {
            contract: contract_id,
            fn_name,
            args,
            sub_invokes: &[],
        },
    }]);
}

// ===========================================================================
// 1. SUBMISSION - only the named investor, and only into a live auction
// ===========================================================================

/// Nobody signs: the submission is rejected and no bid reaches the ledger.
#[test]
fn test_place_bid_without_a_signature_is_rejected() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);

    env.mock_auths(&[]);
    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &BID_AMOUNT,
        &EXPECTED_RETURN,
        &seeded_id(&env, 0x77),
    );

    assert!(
        result.is_err(),
        "a bid must not be accepted without the investor's authorization"
    );
    assert_eq!(indexed_bids(&client, &invoice_id), 0u32);
}

/// A third party signs while the call names the investor: forged identity.
#[test]
fn test_place_bid_with_a_forged_identity_is_rejected() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    let attacker = Address::generate(&env);
    let nonce = seeded_id(&env, 0x78);

    only_auth(
        &env,
        &contract_id,
        &attacker,
        "place_bid",
        (
            investor.clone(),
            invoice_id.clone(),
            BID_AMOUNT,
            EXPECTED_RETURN,
            nonce.clone(),
        )
            .into_val(&env),
    );

    let result =
        client.try_place_bid(&investor, &invoice_id, &BID_AMOUNT, &EXPECTED_RETURN, &nonce);

    assert!(
        result.is_err(),
        "a caller must not submit a bid on behalf of another investor"
    );
    assert_eq!(indexed_bids(&client, &invoice_id), 0u32);
}

/// The investor signs: the bid is stored *and* indexed under the invoice.
///
/// Indexing is part of the boundary - ranking and winner selection read the
/// auction through the per-invoice index, so a stored-but-unindexed bid would
/// be invisible to the auction it belongs to.
#[test]
fn test_place_bid_stores_and_indexes_the_bid_for_the_auction() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);

    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &BID_AMOUNT,
        &EXPECTED_RETURN,
        &seeded_id(&env, 0x01),
    );

    let bid = client.get_bid(&bid_id).expect("bid must be stored");
    assert_eq!(bid.investor, investor);
    assert_eq!(bid.bid_amount, BID_AMOUNT);
    assert_eq!(bid.status, BidStatus::Placed);

    assert_eq!(indexed_bids(&client, &invoice_id), 1u32);
    assert_eq!(client.get_bids_for_invoice(&invoice_id).get(0).unwrap(), bid_id);
    let best = client
        .get_best_bid(&invoice_id)
        .expect("the indexed bid must be selectable");
    assert_eq!(best.bid_id, bid_id);
}

/// A bid above the invoice face value must never enter an auction.
#[test]
fn test_place_bid_above_the_invoice_face_value_is_rejected() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);

    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &1_500i128,
        &2_000i128,
        &seeded_id(&env, 0x02),
    );

    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceAmountInvalid)));
    assert_eq!(indexed_bids(&client, &invoice_id), 0u32);
}

/// A non-positive bid must never enter an auction.
#[test]
fn test_place_bid_with_a_non_positive_amount_is_rejected() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);

    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &0i128,
        &EXPECTED_RETURN,
        &seeded_id(&env, 0x03),
    );

    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
    assert_eq!(indexed_bids(&client, &invoice_id), 0u32);
}

/// A frozen invoice is not an auction: bids are refused before any write.
#[test]
fn test_place_bid_on_a_frozen_invoice_is_rejected() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    freeze(&env, &contract_id, &invoice_id);

    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &BID_AMOUNT,
        &EXPECTED_RETURN,
        &seeded_id(&env, 0x04),
    );

    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceFrozen)));
    assert_eq!(indexed_bids(&client, &invoice_id), 0u32);
}

/// The invoice owner cannot bid on its own invoice.
#[test]
fn test_business_cannot_bid_on_its_own_invoice() {
    let (env, client, contract_id, business, _investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);

    let result = client.try_place_bid(
        &business,
        &invoice_id,
        &BID_AMOUNT,
        &EXPECTED_RETURN,
        &seeded_id(&env, 0x05),
    );

    assert_eq!(result, Err(Ok(QuickLendXError::Unauthorized)));
    assert_eq!(indexed_bids(&client, &invoice_id), 0u32);
}

/// A bid for an unknown invoice is a typed error, not a phantom bid.
#[test]
fn test_place_bid_for_an_unknown_invoice_is_rejected() {
    let (env, client, _contract_id, _business, investor) = setup();
    let unknown_invoice = seeded_id(&env, 0xAB);

    let result = client.try_place_bid(
        &investor,
        &unknown_invoice,
        &BID_AMOUNT,
        &EXPECTED_RETURN,
        &seeded_id(&env, 0x06),
    );

    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceNotFound)));
    assert_eq!(indexed_bids(&client, &unknown_invoice), 0u32);
}

// ===========================================================================
// 2. SELECTION - only the invoice's business can decide a winner
// ===========================================================================

/// Nobody signs: the auction is not decided and nothing is funded.
#[test]
fn test_accept_bid_without_a_signature_is_rejected() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    let bid_id = seed_bid(&env, &contract_id, &investor, &invoice_id, BID_AMOUNT, 0x10);

    env.mock_auths(&[]);
    let result = client.try_accept_bid(&invoice_id, &bid_id);

    assert!(
        result.is_err(),
        "winner selection must require the invoice owner's authorization"
    );
    assert_eq!(bid_status(&client, &bid_id), BidStatus::Placed);
    assert!(client.get_best_bid(&invoice_id).is_some());
}

/// A third party signs: still not the invoice owner, so selection is refused.
#[test]
fn test_accept_bid_by_a_third_party_is_rejected() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    let bid_id = seed_bid(&env, &contract_id, &investor, &invoice_id, BID_AMOUNT, 0x11);
    let attacker = Address::generate(&env);

    only_auth(
        &env,
        &contract_id,
        &attacker,
        "accept_bid",
        (invoice_id.clone(), bid_id.clone()).into_val(&env),
    );

    let result = client.try_accept_bid(&invoice_id, &bid_id);

    assert!(
        result.is_err(),
        "only the invoice's business may select a winner"
    );
    assert_eq!(bid_status(&client, &bid_id), BidStatus::Placed);
    assert!(client.get_best_bid(&invoice_id).is_some());
}

/// Allowed path: the invoice's business signs and the winner is accepted.
#[test]
fn test_the_invoice_business_can_select_the_winning_bid() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    let bid_id = seed_bid(&env, &contract_id, &investor, &invoice_id, BID_AMOUNT, 0x12);

    client.accept_bid(&invoice_id, &bid_id);

    assert_eq!(bid_status(&client, &bid_id), BidStatus::Accepted);
    // The accepted bid is retained for audit and settlement, but it is no
    // longer a selectable winner.
    assert_eq!(indexed_bids(&client, &invoice_id), 1u32);
    assert!(client.get_best_bid(&invoice_id).is_none());
}

/// A winner can be selected at most once per invoice.
#[test]
fn test_accepting_a_winner_twice_is_rejected() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    let bid_id = seed_bid(&env, &contract_id, &investor, &invoice_id, BID_AMOUNT, 0x13);

    client.accept_bid(&invoice_id, &bid_id);
    let second = client.try_accept_bid(&invoice_id, &bid_id);

    assert_eq!(second, Err(Ok(QuickLendXError::InvoiceAlreadyFunded)));
    assert_eq!(bid_status(&client, &bid_id), BidStatus::Accepted);
}

/// A bid belonging to another invoice can never win this auction.
#[test]
fn test_a_bid_from_another_invoice_cannot_win_this_auction() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_a = seed_invoice(&env, &contract_id, &business);
    let invoice_b = seed_invoice(&env, &contract_id, &business);
    let bid_id = seed_bid(&env, &contract_id, &investor, &invoice_a, BID_AMOUNT, 0x14);

    let result = client.try_accept_bid(&invoice_b, &bid_id);

    assert_eq!(result, Err(Ok(QuickLendXError::Unauthorized)));
    // Neither the bid nor either auction may move.
    assert_eq!(bid_status(&client, &bid_id), BidStatus::Placed);
    assert_eq!(indexed_bids(&client, &invoice_b), 0u32);
    assert!(client.get_best_bid(&invoice_b).is_none());
    assert!(client.get_best_bid(&invoice_a).is_some());
}

/// A withdrawn bid is stale: it can never be selected afterwards.
#[test]
fn test_a_withdrawn_bid_cannot_be_selected() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    let bid_id = seed_bid(&env, &contract_id, &investor, &invoice_id, BID_AMOUNT, 0x15);

    client.withdraw_bid(&bid_id);
    let result = client.try_accept_bid(&invoice_id, &bid_id);

    assert_eq!(result, Err(Ok(QuickLendXError::InvalidStatus)));
    assert_eq!(bid_status(&client, &bid_id), BidStatus::Withdrawn);
    assert!(client.get_best_bid(&invoice_id).is_none());
}

/// An unknown bid id is a typed error, not a host panic.
#[test]
fn test_selecting_an_unknown_bid_is_a_typed_error() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    let _bid_id = seed_bid(&env, &contract_id, &investor, &invoice_id, BID_AMOUNT, 0x16);

    let result = client.try_accept_bid(&invoice_id, &seeded_id(&env, 0xEE));

    assert_eq!(result, Err(Ok(QuickLendXError::StorageKeyNotFound)));
    assert_eq!(indexed_bids(&client, &invoice_id), 1u32);
}

/// A bid recorded above the invoice face value - for example written before the
/// submission boundary enforced the ceiling - cannot be accepted either.
///
/// The ceiling is re-checked at the acceptance boundary from trusted state, so a
/// stale record cannot over-fund an invoice.
#[test]
fn test_a_recorded_bid_above_the_face_value_cannot_be_accepted() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    let oversized = seed_bid(&env, &contract_id, &investor, &invoice_id, 1_500i128, 0x17);

    let result = client.try_accept_bid(&invoice_id, &oversized);

    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceAmountInvalid)));
    assert_eq!(bid_status(&client, &oversized), BidStatus::Placed);
    assert!(client.get_best_bid(&invoice_id).is_some());
}

// ===========================================================================
// 3. WITHDRAWAL - only the bid's investor, and only while the bid is live
// ===========================================================================

/// Nobody signs: the bid stays exactly where it was.
#[test]
fn test_withdraw_bid_without_a_signature_is_rejected() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    let bid_id = seed_bid(&env, &contract_id, &investor, &invoice_id, BID_AMOUNT, 0x20);

    env.mock_auths(&[]);
    let result = client.try_withdraw_bid(&bid_id);

    assert!(
        result.is_err(),
        "withdrawal must require the bid owner's authorization"
    );
    assert_eq!(bid_status(&client, &bid_id), BidStatus::Placed);
    assert!(client.get_best_bid(&invoice_id).is_some());
}

/// A third party (here: the invoice's business) cannot withdraw the bid.
#[test]
fn test_a_third_party_cannot_withdraw_someone_elses_bid() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    let bid_id = seed_bid(&env, &contract_id, &investor, &invoice_id, BID_AMOUNT, 0x21);

    only_auth(
        &env,
        &contract_id,
        &business,
        "withdraw_bid",
        (bid_id.clone(),).into_val(&env),
    );

    let result = client.try_withdraw_bid(&bid_id);

    assert!(
        result.is_err(),
        "the invoice's business must not withdraw an investor's bid"
    );
    assert_eq!(bid_status(&client, &bid_id), BidStatus::Placed);
    assert!(client.get_best_bid(&invoice_id).is_some());
}

/// Allowed path: the bid owner withdraws their own live bid.
#[test]
fn test_the_bid_owner_can_withdraw_own_bid() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    let bid_id = seed_bid(&env, &contract_id, &investor, &invoice_id, BID_AMOUNT, 0x22);

    client.withdraw_bid(&bid_id);

    assert_eq!(bid_status(&client, &bid_id), BidStatus::Withdrawn);
    // The withdrawn bid is retained for audit, but is no longer a winner.
    assert_eq!(indexed_bids(&client, &invoice_id), 1u32);
    assert!(client.get_best_bid(&invoice_id).is_none());
}

/// A bid that was already accepted is stale and cannot be withdrawn.
#[test]
fn test_withdrawal_after_selection_is_rejected_as_stale() {
    let (env, client, contract_id, business, investor) = setup();
    let invoice_id = seed_invoice(&env, &contract_id, &business);
    let bid_id = seed_bid(&env, &contract_id, &investor, &invoice_id, BID_AMOUNT, 0x23);

    client.accept_bid(&invoice_id, &bid_id);
    let result = client.try_withdraw_bid(&bid_id);

    assert_eq!(result, Err(Ok(QuickLendXError::BidStale)));
    // No-mutation assertion: the accepted bid stays accepted.
    assert_eq!(bid_status(&client, &bid_id), BidStatus::Accepted);
}

// ===========================================================================
// 4. SHARED MATCH HELPER - the acceptance-side compatibility rules
// ===========================================================================

fn match_fixture() -> (Env, Invoice, Address) {
    let env = Env::default();
    env.ledger().set_timestamp(1_700_000_000);
    let contract_id = env.register(QuickLendXContract, ());
    let business = Address::generate(&env);
    // Built inside a contract context: invoice id allocation touches storage.
    let invoice = env.as_contract(&contract_id, || {
        Invoice::new(
            &env,
            business.clone(),
            INVOICE_AMOUNT,
            Address::generate(&env),
            env.ledger().timestamp() + 86_400,
            String::from_str(&env, "QE-2026-08 match fixture"),
            InvoiceCategory::Services,
            Vec::new(&env),
            None,
            None,
            None,
        )
        .expect("invoice fixture must build")
    });
    (env, invoice, business)
}

fn matched_bid(env: &Env, invoice: &Invoice, investor: &Address, bid_amount: i128) -> Bid {
    Bid {
        bid_id: seeded_id(env, 0x30),
        invoice_id: invoice.id.clone(),
        investor: investor.clone(),
        bid_amount,
        expected_return: bid_amount + 50,
        timestamp: env.ledger().timestamp(),
        status: BidStatus::Placed,
        expiration_timestamp: env.ledger().timestamp() + 86_400,
    }
}

/// A live, in-range bid for this invoice matches.
#[test]
fn test_verify_bid_match_accepts_a_compliant_bid() {
    let (env, invoice, _business) = match_fixture();
    let investor = Address::generate(&env);
    let bid = matched_bid(&env, &invoice, &investor, BID_AMOUNT);

    assert_eq!(verify_bid_match(&env, &bid, &invoice), Ok(()));
}

/// The invoice face value is a hard ceiling at the acceptance boundary too.
#[test]
fn test_verify_bid_match_rejects_a_bid_above_the_face_value() {
    let (env, invoice, _business) = match_fixture();
    let investor = Address::generate(&env);
    let bid = matched_bid(&env, &invoice, &investor, INVOICE_AMOUNT + 1);

    assert_eq!(
        verify_bid_match(&env, &bid, &invoice),
        Err(QuickLendXError::InvoiceAmountInvalid)
    );
}

/// A bid for a different invoice never matches, however valid it looks.
#[test]
fn test_verify_bid_match_rejects_a_bid_for_another_invoice() {
    let (env, invoice, _business) = match_fixture();
    let investor = Address::generate(&env);
    let mut misplaced = matched_bid(&env, &invoice, &investor, BID_AMOUNT);
    misplaced.invoice_id = seeded_id(&env, 0x31);

    assert_eq!(
        verify_bid_match(&env, &misplaced, &invoice),
        Err(QuickLendXError::Unauthorized)
    );
}

/// Only a `Placed` bid can fund an invoice; terminal states never match.
#[test]
fn test_verify_bid_match_rejects_a_bid_that_is_not_placed() {
    let (env, invoice, _business) = match_fixture();
    let investor = Address::generate(&env);

    for status in [
        BidStatus::Accepted,
        BidStatus::Withdrawn,
        BidStatus::Cancelled,
        BidStatus::Expired,
    ] {
        let mut bid = matched_bid(&env, &invoice, &investor, BID_AMOUNT);
        bid.status = status;
        assert_eq!(
            verify_bid_match(&env, &bid, &invoice),
            Err(QuickLendXError::InvalidStatus)
        );
    }
}

/// A non-positive bid never matches.
#[test]
fn test_verify_bid_match_rejects_a_non_positive_bid() {
    let (env, invoice, _business) = match_fixture();
    let investor = Address::generate(&env);
    let mut bid = matched_bid(&env, &invoice, &investor, BID_AMOUNT);
    bid.bid_amount = 0;

    assert_eq!(
        verify_bid_match(&env, &bid, &invoice),
        Err(QuickLendXError::InvalidAmount)
    );
}
