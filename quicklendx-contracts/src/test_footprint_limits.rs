//! Soroban transaction footprint limit assertions for heavy contract entrypoints.
//!
//! Soroban enforces per-transaction caps on ledger entries read/written, bytes
//! read/written, CPU instructions, and memory. This module executes the protocol's
//! heaviest entrypoints at their maximum configured bounds using deterministic
//! fixtures and asserts that every measured resource stays strictly under the
//! constants declared in [`crate::operational_limits::FOOTPRINT_LIMITS`].
//!
//! Covered entrypoints:
//! - `accept_bid` / `accept_bid_and_fund` at `MAX_BIDS_PER_INVOICE` (50) bids
//! - `settle_invoice` on a funded invoice
//! - `cleanup_expired_bids` at `MAX_BIDS_PER_INVOICE` (50) expired bids
//! - `process_partial_payment` at max configured payment history bounds
//! - Paged queries (`get_payment_records`, `get_dispute_timeline`,
//!   `get_bids_for_invoice`) at `MAX_QUERY_LIMIT` (50)

use crate::bid::{BidStorage, MAX_BIDS_PER_INVOICE};
use crate::operational_limits::{
    FootprintLimit, FOOTPRINT_ACCEPT_BID_AND_FUND, FOOTPRINT_CLEANUP_EXPIRED_BIDS,
    FOOTPRINT_PAGED_QUERIES, FOOTPRINT_PROCESS_PARTIAL_PAYMENT, FOOTPRINT_SETTLE_INVOICE,
};
use crate::test_utils::assert_non_mainnet;
use crate::types::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient, MAX_QUERY_LIMIT};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

const BASE_TIMESTAMP: u64 = 1_700_000_000;
const SECONDS_PER_DAY: u64 = 86_400;

/// Measured Soroban transaction footprint for a single invocation.
#[derive(Clone, Copy, Debug)]
struct MeasuredFootprint {
    read_entries: u32,
    write_entries: u32,
    read_bytes: u32,
    write_bytes: u32,
    instructions: u64,
    mem_bytes: u64,
}

/// Capture the footprint of the most recent contract invocation via
/// `env.cost_estimate().resources()` and `env.budget()`.
fn capture_footprint(env: &Env) -> MeasuredFootprint {
    let resources = env.cost_estimate().resources();
    // Use the maximum of cost_estimate and budget counters so direct
    // `env.as_contract` calls also have non-zero CPU/memory accounting.
    let budget_cpu = env.budget().cpu_instruction_cost();
    let budget_mem = env.budget().memory_bytes_cost();
    MeasuredFootprint {
        read_entries: resources.read_entries,
        write_entries: resources.write_entries,
        read_bytes: resources.disk_read_bytes,
        write_bytes: resources.write_bytes,
        instructions: (resources.instructions as u64).max(budget_cpu),
        mem_bytes: (resources.mem_bytes as u64).max(budget_mem),
    }
}

/// Assert that `measured` stays within every bound of `limit`.
fn assert_within_footprint_limit(label: &str, measured: MeasuredFootprint, limit: FootprintLimit) {
    assert!(
        measured.read_entries <= limit.max_read_entries,
        "{label}: read_entries {} exceeded limit {} (entrypoint={})",
        measured.read_entries,
        limit.max_read_entries,
        limit.entrypoint
    );
    assert!(
        measured.write_entries <= limit.max_write_entries,
        "{label}: write_entries {} exceeded limit {} (entrypoint={})",
        measured.write_entries,
        limit.max_write_entries,
        limit.entrypoint
    );
    assert!(
        measured.read_bytes <= limit.max_read_bytes,
        "{label}: read_bytes {} exceeded limit {} (entrypoint={})",
        measured.read_bytes,
        limit.max_read_bytes,
        limit.entrypoint
    );
    assert!(
        measured.write_bytes <= limit.max_write_bytes,
        "{label}: write_bytes {} exceeded limit {} (entrypoint={})",
        measured.write_bytes,
        limit.max_write_bytes,
        limit.entrypoint
    );
    assert!(
        measured.instructions <= limit.max_instructions,
        "{label}: instructions {} exceeded limit {} (entrypoint={})",
        measured.instructions,
        limit.max_instructions,
        limit.entrypoint
    );
    assert!(
        measured.mem_bytes <= limit.max_mem_bytes,
        "{label}: mem_bytes {} exceeded limit {} (entrypoint={})",
        measured.mem_bytes,
        limit.max_mem_bytes,
        limit.entrypoint
    );
}

/// Deterministic test environment and seeded actors.
struct FootprintFixture {
    env: Env,
    contract_id: Address,
    client: QuickLendXContractClient<'static>,
    admin: Address,
    business: Address,
    investor: Address,
    currency: Address,
}

impl FootprintFixture {
    /// Deterministically seed a fresh test environment with verified business,
    /// verified investor, and funded SAC token balances.
    fn new() -> Self {
        assert_non_mainnet("testnet").expect("footprint tests must never target mainnet");

        let env = Env::default();
        env.budget().reset_unlimited();
        env.mock_all_auths();
        env.ledger().set_timestamp(BASE_TIMESTAMP);

        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);

        let business = Address::generate(&env);
        client.submit_kyc_application(&business, &String::from_str(&env, "footprint-business-kyc"));
        client.verify_business(&admin, &business);

        let investor = Address::generate(&env);
        client.submit_investor_kyc(&investor, &String::from_str(&env, "footprint-investor-kyc"));
        client.verify_investor(&investor, &1_000_000_000_000i128);

        let token_admin = Address::generate(&env);
        let currency = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let sac = token::StellarAssetClient::new(&env, &currency);
        let tok = token::Client::new(&env, &currency);
        let balance = 10_000_000_000i128;
        let exp = env.ledger().sequence() + 1_000_000;

        sac.mint(&business, &balance);
        sac.mint(&investor, &balance);
        tok.approve(&business, &contract_id, &balance, &exp);
        tok.approve(&investor, &contract_id, &balance, &exp);

        Self {
            env,
            contract_id,
            client,
            admin,
            business,
            investor,
            currency,
        }
    }

    fn create_verified_invoice(&self, amount: i128) -> BytesN<32> {
        let due_date = self.env.ledger().timestamp() + 30 * SECONDS_PER_DAY;
        let invoice_id = self.client.store_invoice(
            &self.business,
            &amount,
            &self.currency,
            &due_date,
            &String::from_str(&self.env, "Max-bound footprint test invoice"),
            &InvoiceCategory::Services,
            &Vec::new(&self.env),
            &None,
        );
        self.client.verify_invoice(&invoice_id);
        invoice_id
    }

    /// Seed `count` deterministic bids on `invoice_id` using distinct verified
    /// investors so per-investor active-bid caps are respected naturally.
    fn seed_bids(&self, invoice_id: &BytesN<32>, count: u32, bid_amount: i128) -> BytesN<32> {
        let sac = token::StellarAssetClient::new(&self.env, &self.currency);
        let tok = token::Client::new(&self.env, &self.currency);
        let exp = self.env.ledger().sequence() + 1_000_000;

        let mut first_bid_id: Option<BytesN<32>> = None;
        for i in 0..count {
            let bidder = if i == 0 {
                self.investor.clone()
            } else {
                let inv = Address::generate(&self.env);
                self.client
                    .submit_investor_kyc(&inv, &String::from_str(&self.env, "bidder-kyc"));
                self.client.verify_investor(&inv, &1_000_000_000i128);
                sac.mint(&inv, &10_000_000i128);
                tok.approve(&inv, &self.contract_id, &10_000_000i128, &exp);
                inv
            };

            let mut salt_bytes = [0u8; 32];
            salt_bytes[0..4].copy_from_slice(&i.to_be_bytes());
            let salt = BytesN::from_array(&self.env, &salt_bytes);
            let bid_id = self.client.place_bid(
                &bidder,
                invoice_id,
                &bid_amount,
                &(bid_amount + 50_000),
                &salt,
            );
            if first_bid_id.is_none() {
                first_bid_id = Some(bid_id);
            }
        }
        first_bid_id.expect("count must be >= 1")
    }

    fn create_funded_invoice(&self, invoice_amount: i128, bid_amount: i128) -> BytesN<32> {
        let invoice_id = self.create_verified_invoice(invoice_amount);
        let salt = BytesN::from_array(&self.env, &[1u8; 32]);
        let bid_id = self.client.place_bid(
            &self.investor,
            &invoice_id,
            &bid_amount,
            &invoice_amount,
            &salt,
        );
        self.client.accept_bid(&invoice_id, &bid_id);
        invoice_id
    }
}

/// Format a deterministic 64-character lowercase hex transaction ID for nonce `n`.
fn deterministic_tx_id(env: &Env, n: u32) -> String {
    let hex_digits = b"0123456789abcdef";
    let mut buf = [b'0'; 64];
    for shift in 0..8 {
        let nibble = ((n >> (28 - shift * 4)) & 0xf) as usize;
        buf[56 + shift] = hex_digits[nibble];
    }
    let s = core::str::from_utf8(&buf).expect("valid ASCII hex");
    String::from_str(env, s)
}

#[test]
fn test_footprint_accept_bid_and_fund_at_max_bids() {
    let fixture = FootprintFixture::new();
    let invoice_id = fixture.create_verified_invoice(1_000_000);
    let first_bid_id = fixture.seed_bids(&invoice_id, MAX_BIDS_PER_INVOICE, 900_000);

    fixture.env.budget().reset_default();
    fixture.client.accept_bid(&invoice_id, &first_bid_id);
    let measured = capture_footprint(&fixture.env);

    assert_within_footprint_limit(
        "accept_bid_and_fund(50 bids)",
        measured,
        FOOTPRINT_ACCEPT_BID_AND_FUND,
    );
}

#[test]
fn test_footprint_settle_invoice() {
    let fixture = FootprintFixture::new();
    let invoice_amount = 1_000_000i128;
    let invoice_id = fixture.create_funded_invoice(invoice_amount, 900_000);
    let snap = fixture
        .client
        .get_investment(&invoice_id)
        .expect("funded invoice must have investment");

    fixture.env.budget().reset_default();
    fixture
        .client
        .settle_invoice(&invoice_id, &invoice_amount, &snap);
    let measured = capture_footprint(&fixture.env);

    assert_within_footprint_limit("settle_invoice", measured, FOOTPRINT_SETTLE_INVOICE);
}

#[test]
fn test_footprint_cleanup_expired_bids_at_max_bids() {
    let fixture = FootprintFixture::new();
    let invoice_id = fixture.create_verified_invoice(1_000_000);
    let _ = fixture.seed_bids(&invoice_id, MAX_BIDS_PER_INVOICE, 900_000);

    // Advance ledger past bid expiration (24h default in place_bid).
    fixture
        .env
        .ledger()
        .set_timestamp(BASE_TIMESTAMP + 2 * SECONDS_PER_DAY);

    fixture.env.budget().reset_default();
    let cleaned = fixture
        .env
        .as_contract(&fixture.contract_id, || {
            BidStorage::cleanup_expired_bids(&fixture.env, &invoice_id)
        });
    let measured = capture_footprint(&fixture.env);

    assert_eq!(cleaned, MAX_BIDS_PER_INVOICE);
    assert_within_footprint_limit(
        "cleanup_expired_bids(50 expired bids)",
        measured,
        FOOTPRINT_CLEANUP_EXPIRED_BIDS,
    );
}

#[test]
fn test_footprint_process_partial_payment_at_max_page_history() {
    let fixture = FootprintFixture::new();
    let invoice_amount = 1_000_000i128;
    let invoice_id = fixture.create_funded_invoice(invoice_amount, 900_000);

    // Seed MAX_QUERY_LIMIT (50) partial payments first so the next partial
    // payment runs against a populated payment-history ledger state.
    for i in 0..MAX_QUERY_LIMIT {
        let tx_id = deterministic_tx_id(&fixture.env, i + 1);
        fixture
            .client
            .process_partial_payment(&invoice_id, &1_000i128, &tx_id);
    }

    let next_tx_id = deterministic_tx_id(&fixture.env, MAX_QUERY_LIMIT + 1);
    fixture.env.budget().reset_default();
    fixture
        .client
        .process_partial_payment(&invoice_id, &1_000i128, &next_tx_id);
    let measured = capture_footprint(&fixture.env);

    assert_within_footprint_limit(
        "process_partial_payment(after 50 payments)",
        measured,
        FOOTPRINT_PROCESS_PARTIAL_PAYMENT,
    );
}

#[test]
fn test_footprint_paged_queries_at_max_query_limit() {
    let fixture = FootprintFixture::new();

    // 1. get_payment_records at MAX_QUERY_LIMIT (50)
    let invoice_id = fixture.create_funded_invoice(1_000_000, 900_000);
    for i in 0..MAX_QUERY_LIMIT {
        let tx_id = deterministic_tx_id(&fixture.env, i + 100);
        fixture
            .client
            .process_partial_payment(&invoice_id, &1_000i128, &tx_id);
    }

    fixture.env.budget().reset_default();
    let records = fixture
        .client
        .get_payment_records(&invoice_id, &0u32, &MAX_QUERY_LIMIT);
    let measured_payments = capture_footprint(&fixture.env);
    assert_eq!(records.len(), MAX_QUERY_LIMIT);
    assert_within_footprint_limit(
        "get_payment_records(limit=50)",
        measured_payments,
        FOOTPRINT_PAGED_QUERIES,
    );

    // 2. get_bids_for_invoice at MAX_BIDS_PER_INVOICE (50)
    let bid_invoice_id = fixture.create_verified_invoice(1_000_000);
    let _ = fixture.seed_bids(&bid_invoice_id, MAX_BIDS_PER_INVOICE, 900_000);

    fixture.env.budget().reset_default();
    let bids = fixture.client.get_bids_for_invoice(&bid_invoice_id);
    let measured_bids = capture_footprint(&fixture.env);
    assert_eq!(bids.len(), MAX_BIDS_PER_INVOICE);
    assert_within_footprint_limit(
        "get_bids_for_invoice(50 bids)",
        measured_bids,
        FOOTPRINT_PAGED_QUERIES,
    );

    // 3. get_dispute_timeline at MAX_QUERY_LIMIT (50)
    fixture.client.create_dispute(
        &invoice_id,
        &fixture.business,
        &String::from_str(&fixture.env, "footprint dispute reason"),
        &String::from_str(&fixture.env, "footprint dispute evidence"),
    );
    fixture.env.budget().reset_default();
    let _timeline = fixture
        .client
        .get_dispute_timeline(&invoice_id, &0u32, &MAX_QUERY_LIMIT);
    let measured_timeline = capture_footprint(&fixture.env);
    assert_within_footprint_limit(
        "get_dispute_timeline(limit=50)",
        measured_timeline,
        FOOTPRINT_PAGED_QUERIES,
    );
}
