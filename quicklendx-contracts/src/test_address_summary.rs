//! Address-summary regression tests against real contract storage.
//!
//! Seed invoices, bids and funded positions through the contract client. Direct
//! storage updates are limited to historical status fixtures: these tests cover
//! aggregation, rather than re-testing every settlement/default lifecycle.

use crate::address_summary::{
    summarize_address, AddressSummary, BidStrategy, BusinessStrategy, InvestorStrategy,
    SummaryStrategy,
};
use crate::investment::InvestmentStorage;
use crate::storage::{BidStorage, InvoiceStorage};
use crate::types::{BidStatus, InvestmentStatus, InvoiceCategory, InvoiceStatus};
use crate::{QuickLendXContract, QuickLendXContractClient, MAX_QUERY_LIMIT};
use soroban_sdk::{
    testutils::{cost_estimate::NetworkInvocationResourceLimits, Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

struct Fixture {
    env: Env,
    contract: Address,
    admin: Address,
    currency: Address,
}

impl Fixture {
    fn new() -> Self {
        let env = Env::default();
        // Fixture construction is not part of query metering. Funding many
        // positions also exceeds transaction footprints in existing lifecycle
        // code; restore resource limits before testing the read-only queries.
        env.cost_estimate().disable_resource_limits();
        env.cost_estimate().budget().reset_unlimited();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);
        let contract = env.register(QuickLendXContract, ());
        let admin = Address::generate(&env);
        let currency = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let client = QuickLendXContractClient::new(&env, &contract);
        client.set_admin(&admin);
        client.initialize_protocol_limits(&admin);
        Self {
            env,
            contract,
            admin,
            currency,
        }
    }

    fn client(&self) -> QuickLendXContractClient<'_> {
        QuickLendXContractClient::new(&self.env, &self.contract)
    }

    fn business(&self) -> Address {
        let address = Address::generate(&self.env);
        self.verify_business(&address);
        address
    }

    fn verify_business(&self, address: &Address) {
        let client = self.client();
        client.submit_kyc_application(address, &String::from_str(&self.env, "summary business"));
        client.verify_business(&self.admin, address);
    }

    fn investor(&self) -> Address {
        let address = Address::generate(&self.env);
        let client = self.client();
        client.submit_investor_kyc(&address, &String::from_str(&self.env, "summary investor"));
        client.verify_investor(&address, &100_000_000);
        token::StellarAssetClient::new(&self.env, &self.currency).mint(&address, &100_000_000);
        token::Client::new(&self.env, &self.currency).approve(
            &address,
            &self.contract,
            &100_000_000,
            &100_000,
        );
        address
    }

    fn invoice(&self, business: &Address, amount: i128) -> BytesN<32> {
        // Keep fixture creation outside mutation and payment rate-limit windows.
        self.env.ledger().set_sequence_number(
            self.env.ledger().sequence() + crate::protocol_limits::RATE_LIMIT_WINDOW_SEQUENCES,
        );
        self.env.ledger().set_timestamp(
            self.env.ledger().timestamp() + crate::payments::PAYMENT_RATE_LIMIT_WINDOW_SECS,
        );
        self.client().store_invoice(
            business,
            &amount,
            &self.currency,
            &86_400,
            &String::from_str(&self.env, "address summary fixture"),
            &InvoiceCategory::Services,
            &Vec::new(&self.env),
            &None,
        )
    }

    fn bid(
        &self,
        business: &Address,
        investor: &Address,
        amount: i128,
    ) -> (BytesN<32>, BytesN<32>) {
        let invoice = self.invoice(business, amount);
        let client = self.client();
        client.verify_invoice(&invoice);
        let bid = client.place_bid(
            investor,
            &invoice,
            &amount,
            &(amount + 100),
            &BytesN::from_array(&self.env, &[0; 32]),
        );
        (invoice, bid)
    }

    fn investment(
        &self,
        business: &Address,
        investor: &Address,
        amount: i128,
        status: InvestmentStatus,
    ) {
        let (invoice, bid) = self.bid(business, investor, amount);
        self.client().accept_bid(&invoice, &bid);
        self.env.as_contract(&self.contract, || {
            let mut investment =
                InvestmentStorage::get_investment_by_invoice(&self.env, &invoice).unwrap();
            assert_eq!(investment.amount, amount);
            investment.status = status;
            InvestmentStorage::update_investment(&self.env, &investment);
        });
    }

    fn limit_query(&self) {
        self.env.cost_estimate().budget().reset_default();
        self.env
            .cost_estimate()
            .enforce_resource_limits(NetworkInvocationResourceLimits::mainnet());
    }

    fn summary(&self, address: &Address) -> AddressSummary {
        self.limit_query();
        self.env.as_contract(&self.contract, || {
            summarize_address(&self.env, address).unwrap()
        })
    }
}

#[test]
fn test_address_summary_empty_and_registered_without_records() {
    let f = Fixture::new();
    let empty = AddressSummary {
        is_investor: false,
        is_business: false,
        is_bidder: false,
        investor_active_principal: 0,
        investor_completed_count: 0,
        investor_completed_returns: 0,
        investor_defaulted_count: 0,
        investor_refunded_count: 0,
        investor_total_positions: 0,
        business_pending_invoices: 0,
        business_verified_invoices: 0,
        business_funded_invoices: 0,
        business_paid_invoices: 0,
        business_defaulted_invoices: 0,
        business_cancelled_invoices: 0,
        business_refunded_invoices: 0,
        bid_placed_count: 0,
        bid_accepted_count: 0,
        bid_withdrawn_count: 0,
        bid_expired_count: 0,
        bid_cancelled_count: 0,
        bid_total_records: 0,
    };
    assert_eq!(AddressSummary::empty(), empty);
    for address in [Address::generate(&f.env), f.business(), f.investor()] {
        assert_eq!(f.summary(&address), empty);
        f.env.as_contract(&f.contract, || {
            assert_eq!(
                InvestorStrategy::summarize(&f.env, &address).unwrap(),
                empty
            );
            assert_eq!(
                BusinessStrategy::summarize(&f.env, &address).unwrap(),
                empty
            );
            assert_eq!(BidStrategy::summarize(&f.env, &address).unwrap(), empty);
        });
    }
}

#[test]
fn test_address_summary_investor_status_buckets_and_storage_counts() {
    let f = Fixture::new();
    let business = f.business();
    let investor = f.investor();
    // Unequal bucket counts catch accidentally swapped status mappings. The
    // completed return is principal (not bid.expected_return), by API contract.
    for (count, amount, status) in [
        (2, 1_000, InvestmentStatus::Active),
        (3, 2_000, InvestmentStatus::Completed),
        (1, 3_000, InvestmentStatus::Defaulted),
        (4, 4_000, InvestmentStatus::Refunded),
        (5, 5_000, InvestmentStatus::Withdrawn),
    ] {
        for _ in 0..count {
            f.investment(&business, &investor, amount, status.clone());
        }
    }
    let other_investor = f.investor();
    f.investment(&business, &other_investor, 9_000, InvestmentStatus::Active);
    let mut expected = AddressSummary::empty();
    expected.is_investor = true;
    expected.investor_active_principal = 2_000;
    expected.investor_completed_count = 3;
    expected.investor_completed_returns = 6_000;
    expected.investor_defaulted_count = 1;
    expected.investor_refunded_count = 4;
    expected.investor_total_positions = 15;
    f.env.as_contract(&f.contract, || {
        assert_eq!(
            InvestorStrategy::summarize(&f.env, &investor).unwrap(),
            expected
        );
        assert_eq!(
            expected.investor_total_positions,
            InvestmentStorage::get_investments_by_investor(&f.env, &investor).len()
        );
    });
    expected.is_bidder = true;
    expected.bid_accepted_count = 15;
    expected.bid_total_records = 15;
    assert_eq!(f.summary(&investor), expected);
}

#[test]
fn test_address_summary_business_all_statuses_and_address_isolation() {
    let f = Fixture::new();
    let business = f.business();
    for (index, status) in [
        InvoiceStatus::Pending,
        InvoiceStatus::Verified,
        InvoiceStatus::Funded,
        InvoiceStatus::Paid,
        InvoiceStatus::Defaulted,
        InvoiceStatus::Cancelled,
        InvoiceStatus::Refunded,
    ]
    .iter()
    .enumerate()
    {
        for _ in 0..=index {
            let invoice = f.invoice(&business, 1_000);
            f.client().update_invoice_status(&invoice, status);
        }
    }
    let other = f.business();
    f.invoice(&other, 9_000);
    let mut expected = AddressSummary::empty();
    expected.is_business = true;
    expected.business_pending_invoices = 1;
    expected.business_verified_invoices = 2;
    expected.business_funded_invoices = 3;
    expected.business_paid_invoices = 4;
    expected.business_defaulted_invoices = 5;
    expected.business_cancelled_invoices = 6;
    expected.business_refunded_invoices = 7;
    assert_eq!(f.summary(&business), expected);
    f.env.as_contract(&f.contract, || {
        assert_eq!(
            BusinessStrategy::summarize(&f.env, &business).unwrap(),
            expected
        );
        let total = expected.business_pending_invoices
            + expected.business_verified_invoices
            + expected.business_funded_invoices
            + expected.business_paid_invoices
            + expected.business_defaulted_invoices
            + expected.business_cancelled_invoices
            + expected.business_refunded_invoices;
        assert_eq!(
            total,
            InvoiceStorage::get_business_invoices(&f.env, &business).len()
        );
    });
    assert_eq!(f.summary(&other).business_pending_invoices, 1);
}

#[test]
fn test_address_summary_bids_only_all_statuses_and_storage_counts() {
    let f = Fixture::new();
    let business = f.business();
    let bidder = f.investor();
    for (index, status) in [
        BidStatus::Placed,
        BidStatus::Accepted,
        BidStatus::Withdrawn,
        BidStatus::Expired,
        BidStatus::Cancelled,
    ]
    .iter()
    .enumerate()
    {
        for _ in 0..=index {
            let (_, bid_id) = f.bid(&business, &bidder, 1_000);
            // Historical bid fixtures deliberately have no investment records.
            f.env.as_contract(&f.contract, || {
                let mut bid = BidStorage::get_bid(&f.env, &bid_id).unwrap();
                bid.status = status.clone();
                BidStorage::update_bid(&f.env, &bid);
            });
        }
    }
    let other = f.investor();
    f.bid(&business, &other, 9_000);
    let mut expected = AddressSummary::empty();
    expected.is_bidder = true;
    expected.bid_placed_count = 1;
    expected.bid_accepted_count = 2;
    expected.bid_withdrawn_count = 3;
    expected.bid_expired_count = 4;
    expected.bid_cancelled_count = 5;
    expected.bid_total_records = 15;
    assert_eq!(f.summary(&bidder), expected);
    f.env.as_contract(&f.contract, || {
        assert_eq!(BidStrategy::summarize(&f.env, &bidder).unwrap(), expected);
        assert_eq!(
            expected.bid_total_records,
            BidStorage::get_all_bids_by_investor(&f.env, &bidder).len()
        );
        assert_eq!(
            expected.bid_total_records,
            expected.bid_placed_count
                + expected.bid_accepted_count
                + expected.bid_withdrawn_count
                + expected.bid_expired_count
                + expected.bid_cancelled_count
        );
        assert!(InvestmentStorage::get_investments_by_investor(&f.env, &bidder).is_empty());
    });
    assert_eq!(f.summary(&other).bid_total_records, 1);
}

#[test]
fn test_address_summary_combines_all_three_roles_without_overwriting_fields() {
    let f = Fixture::new();
    let participant = f.investor();
    f.verify_business(&participant);
    let other_business = f.business();
    let own_invoice = f.invoice(&participant, 7_000);
    f.client()
        .update_invoice_status(&own_invoice, &InvoiceStatus::Paid);
    f.investment(
        &other_business,
        &participant,
        1_000,
        InvestmentStatus::Active,
    );
    f.investment(
        &other_business,
        &participant,
        2_000,
        InvestmentStatus::Completed,
    );
    f.bid(&other_business, &participant, 3_000);
    let mut expected = AddressSummary::empty();
    expected.is_investor = true;
    expected.is_business = true;
    expected.is_bidder = true;
    expected.investor_active_principal = 1_000;
    expected.investor_completed_count = 1;
    expected.investor_completed_returns = 2_000;
    expected.investor_total_positions = 2;
    expected.business_paid_invoices = 1;
    expected.bid_placed_count = 1;
    expected.bid_accepted_count = 2;
    expected.bid_total_records = 3;
    assert_eq!(f.summary(&participant), expected);
    assert_eq!(f.summary(&participant), expected); // Stable across repeated reads.
    f.env.as_contract(&f.contract, || {
        assert_eq!(
            InvestmentStorage::get_investments_by_investor(&f.env, &participant).len(),
            2
        );
        assert_eq!(
            InvoiceStorage::get_business_invoices(&f.env, &participant).len(),
            1
        );
        assert_eq!(
            BidStorage::get_all_bids_by_investor(&f.env, &participant).len(),
            3
        );
    });
}

#[test]
fn test_address_summary_large_investor_caps_positions_within_default_budget() {
    let f = Fixture::new();
    let business = f.business();
    let investor = f.investor();
    for _ in 0..MAX_QUERY_LIMIT {
        f.investment(&business, &investor, 1_000, InvestmentStatus::Active);
    }
    // A distinct tail record detects both off-by-one scans and truncation bugs.
    f.investment(&business, &investor, 9_000, InvestmentStatus::Completed);
    f.env.as_contract(&f.contract, || {
        assert_eq!(
            InvestmentStorage::get_investments_by_investor(&f.env, &investor).len(),
            MAX_QUERY_LIMIT + 1
        );
    });
    // Only the investor strategy promises a bounded scan. The combined
    // summary also traverses all bid records, so it has no global record cap.
    f.limit_query();
    let summary = f.env.as_contract(&f.contract, || {
        InvestorStrategy::summarize(&f.env, &investor).unwrap()
    });
    let mut expected = AddressSummary::empty();
    expected.is_investor = true;
    expected.investor_active_principal = i128::from(MAX_QUERY_LIMIT) * 1_000;
    expected.investor_total_positions = MAX_QUERY_LIMIT;
    assert_eq!(summary, expected);
}
