use super::*;
use crate::errors::QuickLendXError;
use crate::storage::BidStorage;
use crate::types::{Bid, BidStatus};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Bytes, BytesN, Env, String, Vec};

struct TestContext {
    env: Env,
    client: QuickLendXContractClient<'static>,
    admin: Address,
    business: Address,
    investor: Address,
    currency: Address,
    contract_id: Address,
}

impl TestContext {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        client.initialize_admin(&admin);

        let token_admin = Address::generate(&env);
        let currency = env.register_stellar_asset_contract_v2(token_admin).address();
        let sac = token::StellarAssetClient::new(&env, &currency);
        sac.mint(&business, &100_000i128);
        sac.mint(&investor, &100_000i128);
        let tok = token::Client::new(&env, &currency);
        let exp = env.ledger().sequence() + 50_000;
        tok.approve(&business, &contract_id, &400_000i128, &exp);
        tok.approve(&investor, &contract_id, &400_000i128, &exp);

        client.add_currency(&admin, &currency);

        Self { env, client, admin, business, investor, currency, contract_id }
    }

    fn setup_kyc(&self) {
        self.client.submit_kyc_application(&self.business, &Bytes::from_slice(&self.env, b"KYC"));
        self.client.verify_business(&self.admin, &self.business);
        self.client.submit_investor_kyc(&self.investor, &Bytes::from_slice(&self.env, b"KYC"));
        self.client.verify_investor(&self.admin, &self.investor, &200_000i128);
    }

    fn create_verified_invoice(&self) -> BytesN<32> {
        let due_date = self.env.ledger().timestamp() + 86_400;
        let invoice_id = self.client.upload_invoice(
            &self.business,
            &1_000i128,
            &self.currency,
            &due_date,
            &String::from_str(&self.env, "Test"),
            &InvoiceCategory::Services,
            &Vec::new(&self.env),
        &None);
        self.client.verify_invoice(&invoice_id);
        invoice_id
    }

    fn place_bid(&self, invoice_id: &BytesN<32>) -> BytesN<32> {
        self.client.place_bid(
            &self.investor,
            invoice_id,
            &1_000i128,
            &1_000i128,
            &BytesN::from_array(&self.env, &[0u8; 32]),
        )
    }

    fn bid_status(&self, bid_id: &BytesN<32>) -> BidStatus {
        self.client.get_bid(bid_id).unwrap().status
    }
}

#[test]
fn test_transition_placed_to_accepted() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    let bid_id = ctx.place_bid(&invoice_id, &BytesN::from_array(&env, &[0u8; 32]));
    assert_eq!(ctx.bid_status(&bid_id), BidStatus::Placed);
    ctx.client.accept_bid(&invoice_id, &bid_id);
    assert_eq!(ctx.bid_status(&bid_id), BidStatus::Accepted);
}

#[test]
fn test_transition_placed_to_withdrawn() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    let bid_id = ctx.place_bid(&invoice_id, &BytesN::from_array(&env, &[0u8; 32]));
    assert_eq!(ctx.bid_status(&bid_id), BidStatus::Placed);
    ctx.client.withdraw_bid(&bid_id);
    assert_eq!(ctx.bid_status(&bid_id), BidStatus::Withdrawn);
}

#[test]
fn test_transition_placed_to_cancelled() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    let bid_id = ctx.place_bid(&invoice_id, &BytesN::from_array(&env, &[0u8; 32]));
    assert_eq!(ctx.bid_status(&bid_id), BidStatus::Placed);
    assert!(ctx.client.cancel_bid(&bid_id).is_ok());
    assert_eq!(ctx.bid_status(&bid_id), BidStatus::Cancelled);
}

#[test]
fn test_transition_placed_to_expired() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    let bid_id = ctx.place_bid(&invoice_id, &BytesN::from_array(&env, &[0u8; 32]));
    assert_eq!(ctx.bid_status(&bid_id), BidStatus::Placed);
    ctx.env.ledger().set_timestamp(ctx.env.ledger().timestamp() + 86_400 * 40);
    let cleaned = ctx.client.cleanup_expired_bids(&invoice_id);
    assert!(cleaned > 0);
    assert_eq!(ctx.bid_status(&bid_id), BidStatus::Expired);
}

#[test]
fn test_cannot_accept_non_placed_bid() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    let bid_id = ctx.place_bid(&invoice_id, &BytesN::from_array(&env, &[0u8; 32]));
    assert!(ctx.client.cancel_bid(&bid_id).is_ok());
    let result = ctx.client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_err());
}

#[test]
fn test_cannot_withdraw_non_placed_bid() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    let bid_id = ctx.place_bid(&invoice_id, &BytesN::from_array(&env, &[0u8; 32]));
    assert!(ctx.client.cancel_bid(&bid_id).is_ok());
    let result = ctx.client.try_withdraw_bid(&bid_id);
    assert!(result.is_err());
}

#[test]
fn test_cannot_cancel_non_placed_bid() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    let bid_id = ctx.place_bid(&invoice_id, &BytesN::from_array(&env, &[0u8; 32]));
    ctx.client.withdraw_bid(&bid_id);
    let cancelled = ctx.client.cancel_bid(&bid_id);
    assert_eq!(cancelled, Err(QuickLendXError::BidStale));
}

#[test]
fn test_terminal_bid_states_are_immutable() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    let bid_id = ctx.place_bid(&invoice_id, &BytesN::from_array(&env, &[0u8; 32]));
    ctx.client.accept_bid(&invoice_id, &bid_id);
    let result = ctx.client.try_withdraw_bid(&bid_id);
    assert!(result.is_err());
    let cancelled = ctx.client.cancel_bid(&bid_id);
    assert_eq!(cancelled, Err(QuickLendXError::BidStale));
}

#[test]
fn test_transitions_guard_consistency() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();

    let transitions: Vec<(BidStatus, bool)> = vec![
        (BidStatus::Accepted, true),
        (BidStatus::Withdrawn, true),
        (BidStatus::Cancelled, true),
        (BidStatus::Expired, true),
    ];

    for (to, should_succeed) in transitions {
        let bid_id = ctx.place_bid(&invoice_id, &BytesN::from_array(&env, &[0u8; 32]));
        assert_eq!(ctx.bid_status(&bid_id), BidStatus::Placed);

        let result = match to {
            BidStatus::Accepted => {
                ctx.client.try_accept_bid(&invoice_id, &bid_id)
            }
            BidStatus::Withdrawn => {
                ctx.client.try_withdraw_bid(&bid_id)
            }
            BidStatus::Cancelled => {
                let res = ctx.client.cancel_bid(&bid_id);
                if should_succeed { Ok(res.is_ok()) } else { Err(()) }
            }
            BidStatus::Expired => {
                ctx.env.ledger().set_timestamp(ctx.env.ledger().timestamp() + 86_400 * 40);
                let cleaned = ctx.client.cleanup_expired_bids(&invoice_id);
                if should_succeed { Ok(cleaned > 0) } else { Err(()) }
            }
        };

        assert_eq!(result.is_ok(), should_succeed,
            "Placed -> {:?} should {} but got {:?}",
            to, if should_succeed { "succeed" } else { "fail" }, result);
    }
}
