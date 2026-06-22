#![cfg(all(test, feature = "fuzz-tests"))]

//! Property-based fuzz harness for [`settlement::process_partial_payment`].
//!
//! Validates nonce/transaction_id replay protection, cumulative payment capping,
//! monotonic payment-count growth, and deduplication stability under reordering.
//!
//! Run:
//! ```bash
//! # Fast local smoke (10 cases, ~1–2 min)
//! cargo test --features fuzz-tests test_fuzz_partial_payment_smoke
//!
//! # Full acceptance / CI (50,000 cases)
//! PROPTEST_CASES=50000 cargo test --features fuzz-tests test_fuzz_partial_payment
//! ```
//!
//! See [`docs/partial-payment-fuzz.md`](../docs/partial-payment-fuzz.md).

use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::settlement::get_payment_count;
use crate::{QuickLendXContract, QuickLendXContractClient};
use proptest::prelude::*;
use proptest::test_runner::FileFailurePersistence;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec as SorobanVec,
};
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String as RustString;
use alloc::vec::Vec;

const MIN_INVOICE_AMOUNT: i128 = 100;
const MAX_INVOICE_AMOUNT: i128 = 100_000;
const MAX_ACTIONS: usize = 40;
/// Mirrors `settlement::MAX_PAYMENT_COUNT`.
const MAX_PAYMENT_COUNT: u32 = 1_000;
/// Fixed case count for `test_fuzz_partial_payment_smoke` (local dev / quick CI).
const SMOKE_CASES: u32 = 10;

fn partial_payment_proptest_config() -> ProptestConfig {
    ProptestConfig::with_failure_persistence(FileFailurePersistence::WithSource(
        "partial_payment",
    ))
}

fn partial_payment_proptest_config_smoke() -> ProptestConfig {
    ProptestConfig {
        cases: SMOKE_CASES,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource(
            "partial_payment",
        ))),
        ..ProptestConfig::default()
    }
}

fn partial_payment_strategy() -> impl Strategy<Value = (i128, Vec<PaymentAction>)> {
    (MIN_INVOICE_AMOUNT..=MAX_INVOICE_AMOUNT).prop_flat_map(|invoice_amount| {
        (
            Just(invoice_amount),
            prop::collection::vec(action_strategy(invoice_amount), 1..MAX_ACTIONS),
        )
    })
}

fn tx_id_for_index(env: &Env, tx_index: u32) -> String {
    String::from_str(env, &alloc::format!("fuzz-tx-{tx_index}"))
}

fn setup_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    contract_id: &Address,
    invoice_amount: i128,
) -> (BytesN<32>, Address) {
    let admin = Address::generate(env);
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let initial_balance = invoice_amount.saturating_mul(10).max(50_000);
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, contract_id, &initial_balance, &expiration);

    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(env, "business-kyc"));
    client.verify_business(&admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(env, "investor-kyc"));
    client.verify_investor(&investor, &initial_balance);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &invoice_amount,
        &currency,
        &due_date,
        &String::from_str(env, "fuzz partial payment invoice"),
        &InvoiceCategory::Services,
        &SorobanVec::new(env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &invoice_amount,
        &(invoice_amount + 100),
    );
    client.accept_bid(&invoice_id, &bid_id);
    (invoice_id, business)
}

/// A single step applied to `process_partial_payment` during fuzzing.
///
/// # Variants
/// - [`ValidPayment`](PaymentAction::ValidPayment): first-seen or fresh `(transaction_id, amount)`.
/// - [`ReplaySame`](PaymentAction::ReplaySame): resubmit an earlier `transaction_id` with the
///   same amount — must be idempotent (no double-credit).
/// - [`ReplayDifferentAmount`](PaymentAction::ReplayDifferentAmount): resubmit with a different
///   amount — nonce replay must still block any additional credit.
#[derive(Clone, Debug)]
enum PaymentAction {
    /// Apply a payment using `tx_index` as the stable transaction_id/nonce key.
    ValidPayment { amount: i128, tx_index: u32 },
    /// Replay `tx_index` reusing the amount from its first successful application.
    ReplaySame { tx_index: u32 },
    /// Replay `tx_index` but request `alt_amount` — must not increase `total_paid`.
    ReplayDifferentAmount { tx_index: u32, alt_amount: i128 },
}

/// Reference model for partial-payment accounting independent of execution order
/// for replay steps that occur **after** the referenced `transaction_id` was first seen.
///
/// The oracle tracks:
/// - cumulative `total_paid` capped at `invoice_amount`
/// - monotonically non-decreasing `payment_count`
/// - a set of seen non-empty transaction identifiers (nonce replay table)
#[derive(Clone, Debug)]
struct PartialPaymentOracle {
    invoice_amount: i128,
    total_paid: i128,
    payment_count: u32,
    seen_nonces: BTreeSet<RustString>,
    finalized: bool,
    /// First applied amount per tx_index (oracle string key).
    first_amount_by_tx: BTreeMap<u32, i128>,
}

impl PartialPaymentOracle {
    fn new(invoice_amount: i128) -> Self {
        Self {
            invoice_amount,
            total_paid: 0,
            payment_count: 0,
            seen_nonces: BTreeSet::new(),
            finalized: false,
            first_amount_by_tx: BTreeMap::new(),
        }
    }

    fn nonce_key(tx_index: u32) -> RustString {
        format!("fuzz-tx-{tx_index}")
    }

    fn remaining(&self) -> i128 {
        (self.invoice_amount - self.total_paid).max(0)
    }

    /// Simulate `record_payment` / `process_partial_payment` semantics.
    ///
    /// Order mirrors production: amount validation → payable status → nonce replay
    /// idempotency → payment-count cap → remaining-due check → apply with cap.
    fn apply(&mut self, amount: i128, tx_index: u32) -> Result<(), QuickLendXError> {
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }
        if self.finalized {
            return Err(QuickLendXError::InvalidStatus);
        }

        let nonce = Self::nonce_key(tx_index);
        if self.seen_nonces.contains(&nonce) {
            return Ok(());
        }

        if self.payment_count >= MAX_PAYMENT_COUNT {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        let remaining = self.remaining();
        if remaining <= 0 {
            return Err(QuickLendXError::InvalidStatus);
        }

        let applied = amount.min(remaining);
        if applied <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        self.total_paid = self
            .total_paid
            .checked_add(applied)
            .ok_or(QuickLendXError::InvalidAmount)?;
        if self.total_paid > self.invoice_amount {
            return Err(QuickLendXError::InvalidAmount);
        }

        self.seen_nonces.insert(nonce);
        self.first_amount_by_tx.entry(tx_index).or_insert(applied);
        self.payment_count = self.payment_count.saturating_add(1);

        if self.total_paid >= self.invoice_amount {
            self.finalized = true;
        }
        Ok(())
    }

    fn amount_for_action(&self, action: &PaymentAction) -> i128 {
        match action {
            PaymentAction::ValidPayment { amount, .. } => *amount,
            PaymentAction::ReplaySame { tx_index } => self
                .first_amount_by_tx
                .get(tx_index)
                .copied()
                .unwrap_or(1),
            PaymentAction::ReplayDifferentAmount { alt_amount, .. } => *alt_amount,
        }
    }

    fn tx_index_for_action(action: &PaymentAction) -> u32 {
        match action {
            PaymentAction::ValidPayment { tx_index, .. }
            | PaymentAction::ReplaySame { tx_index }
            | PaymentAction::ReplayDifferentAmount { tx_index, .. } => *tx_index,
        }
    }
}

fn execute_action(
    env: &Env,
    client: &QuickLendXContractClient,
    invoice_id: &BytesN<32>,
    action: &PaymentAction,
    oracle: &PartialPaymentOracle,
) -> Result<(), QuickLendXError> {
    let tx_index = PartialPaymentOracle::tx_index_for_action(action);
    let amount = oracle.amount_for_action(action);
    let tx_id = tx_id_for_index(env, tx_index);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    match client.try_process_partial_payment(invoice_id, &amount, &tx_id) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(conversion_err)) => panic!(
            "unexpected conversion error for action {:?}: {:?}",
            action, conversion_err
        ),
        Err(invoke_err) => Err(invoke_err.unwrap()),
    }
}

fn run_sequence(
    env: &Env,
    client: &QuickLendXContractClient,
    contract_id: &Address,
    invoice_id: &BytesN<32>,
    invoice_amount: i128,
    actions: &[PaymentAction],
) -> PartialPaymentOracle {
    let mut oracle = PartialPaymentOracle::new(invoice_amount);
    let mut prev_count = 0u32;
    let mut prev_paid = 0i128;

    for action in actions {
        let tx_index = PartialPaymentOracle::tx_index_for_action(action);
        let amount = match action {
            PaymentAction::ValidPayment { amount, .. } => *amount,
            PaymentAction::ReplaySame { .. } => oracle
                .first_amount_by_tx
                .get(&tx_index)
                .copied()
                .unwrap_or(1),
            PaymentAction::ReplayDifferentAmount { alt_amount, .. } => *alt_amount,
        };

        let nonce_key = PartialPaymentOracle::nonce_key(tx_index);
        let nonce_was_seen = oracle.seen_nonces.contains(&nonce_key);
        let before_paid = client.get_invoice(invoice_id).total_paid;
        let before_count = env
            .as_contract(contract_id, || get_payment_count(env, invoice_id).unwrap());

        let contract_result = execute_action(env, client, invoice_id, action, &oracle);
        let oracle_result = oracle.apply(amount, tx_index);

        match (&contract_result, &oracle_result) {
            (Ok(()), Ok(())) => {}
            (Err(e1), Err(e2)) => {
                assert_eq!(
                    *e1, *e2,
                    "error mismatch for action {:?}: contract={:?} oracle={:?}",
                    action, e1, e2
                );
            }
            (c, o) => panic!(
                "contract/oracle success mismatch for {:?}: contract={:?} oracle={:?}",
                action, c, o
            ),
        }

        let after_paid = client.get_invoice(invoice_id).total_paid;
        let after_count = env
            .as_contract(contract_id, || get_payment_count(env, invoice_id).unwrap());

        if nonce_was_seen {
            assert_eq!(
                after_paid, before_paid,
                "replay changed total_paid for {:?}",
                action
            );
            assert_eq!(
                after_count, before_count,
                "replay changed payment_count for {:?}",
                action
            );
        }

        assert_step_invariants_deterministic(
            env,
            client,
            contract_id,
            invoice_id,
            invoice_amount,
            &oracle,
            prev_count,
            prev_paid,
        );

        prev_count = after_count;
        prev_paid = after_paid;
    }
    oracle
}

fn assert_step_invariants_deterministic(
    env: &Env,
    client: &QuickLendXContractClient,
    contract_id: &Address,
    invoice_id: &BytesN<32>,
    invoice_amount: i128,
    oracle: &PartialPaymentOracle,
    prev_count: u32,
    prev_paid: i128,
) {
    let invoice = client.get_invoice(invoice_id);
    assert!(
        invoice.total_paid <= invoice.amount,
        "cumulative cap violated: total_paid={} amount={}",
        invoice.total_paid,
        invoice.amount
    );
    assert!(
        invoice.total_paid <= invoice_amount,
        "total_paid {} exceeded invoice_amount {}",
        invoice.total_paid,
        invoice_amount
    );
    assert!(
        invoice.total_paid >= prev_paid,
        "total_paid regressed: {} -> {}",
        prev_paid,
        invoice.total_paid
    );

    let count = env
        .as_contract(contract_id, || get_payment_count(env, invoice_id).unwrap());
    assert!(
        count >= prev_count,
        "payment_count decreased: {} -> {}",
        prev_count,
        count
    );
    assert_eq!(
        invoice.total_paid, oracle.total_paid,
        "oracle mismatch on total_paid: contract={} oracle={}",
        invoice.total_paid, oracle.total_paid
    );
    assert_eq!(
        count, oracle.payment_count,
        "oracle mismatch on payment_count: contract={} oracle={}",
        count, oracle.payment_count
    );
}

fn assert_step_invariants(
    env: &Env,
    client: &QuickLendXContractClient,
    contract_id: &Address,
    invoice_id: &BytesN<32>,
    invoice_amount: i128,
    oracle: &PartialPaymentOracle,
    prev_count: u32,
    prev_paid: i128,
) -> Result<(), TestCaseError> {
    let invoice = client.get_invoice(invoice_id);
    prop_assert!(
        invoice.total_paid <= invoice.amount,
        "cumulative cap violated: total_paid={} amount={}",
        invoice.total_paid,
        invoice.amount
    );
    prop_assert!(
        invoice.total_paid <= invoice_amount,
        "total_paid {} exceeded invoice_amount {}",
        invoice.total_paid,
        invoice_amount
    );
    prop_assert!(
        invoice.total_paid >= prev_paid,
        "total_paid regressed: {} -> {}",
        prev_paid,
        invoice.total_paid
    );

    let count = env
        .as_contract(contract_id, || get_payment_count(env, invoice_id).unwrap());
    prop_assert!(
        count >= prev_count,
        "payment_count decreased: {} -> {}",
        prev_count,
        count
    );
    prop_assert_eq!(
        invoice.total_paid,
        oracle.total_paid,
        "oracle mismatch on total_paid: contract={} oracle={}",
        invoice.total_paid,
        oracle.total_paid
    );
    prop_assert_eq!(
        count,
        oracle.payment_count,
        "oracle mismatch on payment_count: contract={} oracle={}",
        count,
        oracle.payment_count
    );
    Ok(())
}

fn run_sequence_proptest(
    env: &Env,
    client: &QuickLendXContractClient,
    contract_id: &Address,
    invoice_id: &BytesN<32>,
    invoice_amount: i128,
    actions: &[PaymentAction],
) -> Result<PartialPaymentOracle, TestCaseError> {
    let mut oracle = PartialPaymentOracle::new(invoice_amount);
    let mut prev_count = 0u32;
    let mut prev_paid = 0i128;

    for action in actions {
        let tx_index = PartialPaymentOracle::tx_index_for_action(action);
        let amount = match action {
            PaymentAction::ValidPayment { amount, .. } => *amount,
            PaymentAction::ReplaySame { .. } => oracle
                .first_amount_by_tx
                .get(&tx_index)
                .copied()
                .unwrap_or(1),
            PaymentAction::ReplayDifferentAmount { alt_amount, .. } => *alt_amount,
        };

        let nonce_key = PartialPaymentOracle::nonce_key(tx_index);
        let nonce_was_seen = oracle.seen_nonces.contains(&nonce_key);
        let before_paid = client.get_invoice(invoice_id).total_paid;
        let before_count = env
            .as_contract(contract_id, || get_payment_count(env, invoice_id).unwrap());

        let contract_result = execute_action(env, client, invoice_id, action, &oracle);
        let oracle_result = oracle.apply(amount, tx_index);

        match (&contract_result, &oracle_result) {
            (Ok(()), Ok(())) => {}
            (Err(e1), Err(e2)) => {
                prop_assert_eq!(
                    *e1, *e2,
                    "error mismatch for action {:?}: contract={:?} oracle={:?}",
                    action, e1, e2
                );
            }
            (c, o) => prop_assert!(
                false,
                "contract/oracle success mismatch for {:?}: contract={:?} oracle={:?}",
                action, c, o
            ),
        }

        let after_paid = client.get_invoice(invoice_id).total_paid;
        let after_count = env
            .as_contract(contract_id, || get_payment_count(env, invoice_id).unwrap());

        if nonce_was_seen {
            prop_assert_eq!(
                after_paid, before_paid,
                "replay changed total_paid for {:?}",
                action
            );
            prop_assert_eq!(
                after_count, before_count,
                "replay changed payment_count for {:?}",
                action
            );
        }

        assert_step_invariants(
            env,
            client,
            contract_id,
            invoice_id,
            invoice_amount,
            &oracle,
            prev_count,
            prev_paid,
        )?;

        prev_count = after_count;
        prev_paid = after_paid;
    }
    Ok(oracle)
}

fn action_strategy(max_amount: i128) -> impl Strategy<Value = PaymentAction> {
    prop_oneof![
        (1i128..=max_amount, (0u32..20u32))
            .prop_map(|(amount, tx_index)| PaymentAction::ValidPayment { amount, tx_index }),
        (0u32..20u32).prop_map(|tx_index| PaymentAction::ReplaySame { tx_index }),
        ((0u32..20u32), (1i128..=max_amount)).prop_map(
            |(tx_index, alt_amount)| PaymentAction::ReplayDifferentAmount { tx_index, alt_amount },
        ),
    ]
}

/// Reorder only replay actions after all first-seen valid payments; replay
/// deduplication must leave `(total_paid, payment_count)` unchanged.
fn reorder_replays_after_first_seen(actions: &[PaymentAction]) -> Vec<PaymentAction> {
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut core = Vec::new();
    let mut replays = Vec::new();

    for action in actions {
        match action {
            PaymentAction::ValidPayment { tx_index, .. } => {
                if seen.insert(*tx_index) {
                    core.push(action.clone());
                } else {
                    replays.push(action.clone());
                }
            }
            PaymentAction::ReplaySame { tx_index }
            | PaymentAction::ReplayDifferentAmount { tx_index, .. } => {
                if seen.contains(tx_index) {
                    replays.push(action.clone());
                } else {
                    core.push(action.clone());
                }
            }
        }
    }
    replays.reverse();
    core.extend(replays);
    core
}

fn fuzz_partial_payment_case(
    invoice_amount: i128,
    actions: Vec<PaymentAction>,
) -> Result<(), TestCaseError> {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let (invoice_id, _business) =
        setup_funded_invoice(&env, &client, &contract_id, invoice_amount);

    let oracle_forward = run_sequence_proptest(
        &env, &client, &contract_id, &invoice_id, invoice_amount, &actions,
    )?;

    // Reordering: replays after first-seen payments must not change totals.
    let reordered = reorder_replays_after_first_seen(&actions);
    let env2 = Env::default();
    env2.mock_all_auths();
    let contract_id2 = env2.register(QuickLendXContract, ());
    let client2 = QuickLendXContractClient::new(&env2, &contract_id2);
    let (invoice_id2, _) =
        setup_funded_invoice(&env2, &client2, &contract_id2, invoice_amount);

    let oracle_reordered = run_sequence_proptest(
        &env2, &client2, &contract_id2, &invoice_id2, invoice_amount, &reordered,
    )?;

    prop_assert_eq!(
        oracle_forward.total_paid,
        oracle_reordered.total_paid,
        "reordering replays changed cumulative total_paid"
    );
    prop_assert_eq!(
        oracle_forward.payment_count,
        oracle_reordered.payment_count,
        "reordering replays changed payment_count"
    );
    prop_assert!(
        oracle_forward.total_paid <= invoice_amount,
        "security: overpayment past invoice amount"
    );
    Ok(())
}

proptest! {
    #![proptest_config(partial_payment_proptest_config_smoke())]

    /// Fast smoke run (10 cases) for local dev and quick CI feedback.
    #[test]
    fn test_fuzz_partial_payment_smoke(
        (invoice_amount, actions) in partial_payment_strategy(),
    ) {
        fuzz_partial_payment_case(invoice_amount, actions)?;
    }
}

proptest! {
    #![proptest_config(partial_payment_proptest_config())]

    /// Interleave valid and replayed partial payments; assert cumulative cap,
    /// monotonic payment count, and transaction_id deduplication under reordering.
    #[test]
    fn test_fuzz_partial_payment(
        (invoice_amount, actions) in partial_payment_strategy(),
    ) {
        fuzz_partial_payment_case(invoice_amount, actions)?;
    }
}

// ---------------------------------------------------------------------------
// Deterministic edge-case tests
// ---------------------------------------------------------------------------

fn edge_setup(invoice_amount: i128) -> (Env, QuickLendXContractClient<'static>, Address, BytesN<32>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let (invoice_id, _) = setup_funded_invoice(&env, &client, &contract_id, invoice_amount);
    (env, client, contract_id, invoice_id)
}

#[test]
fn partial_payment_zero_amount_rejected() {
    let (env, client, _contract_id, invoice_id) = edge_setup(1_000);
    let result = client.try_process_partial_payment(
        &invoice_id,
        &0,
        &String::from_str(&env, "zero-tx"),
    );
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidAmount);
    assert_eq!(client.get_invoice(&invoice_id).total_paid, 0);
}

#[test]
fn partial_payment_exact_final_marks_paid() {
    let (env, client, _contract_id, invoice_id) = edge_setup(1_000);
    client.process_partial_payment(
        &invoice_id,
        &1_000,
        &String::from_str(&env, "exact-final"),
    );
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.total_paid, 1_000);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
}

#[test]
fn partial_payment_after_finalization_rejected() {
    let (env, client, _contract_id, invoice_id) = edge_setup(1_000);
    client.process_partial_payment(
        &invoice_id,
        &1_000,
        &String::from_str(&env, "final-tx"),
    );
    let result = client.try_process_partial_payment(
        &invoice_id,
        &1,
        &String::from_str(&env, "after-final"),
    );
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), QuickLendXError::InvalidStatus);
    assert_eq!(client.get_invoice(&invoice_id).total_paid, 1_000);
}

#[test]
fn partial_payment_replay_no_double_credit() {
    let (env, client, contract_id, invoice_id) = edge_setup(1_000);
    let tx = String::from_str(&env, "replay-tx");
    client.process_partial_payment(&invoice_id, &400, &tx);
    client.process_partial_payment(&invoice_id, &900, &tx);
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.total_paid, 400, "double-credit via nonce replay");
    let count = env
        .as_contract(&contract_id, || get_payment_count(&env, &invoice_id).unwrap());
    assert_eq!(count, 1);
}

#[test]
fn partial_payment_reordered_replays_match_forward() {
    let actions = alloc::vec![
        PaymentAction::ValidPayment {
            amount: 300,
            tx_index: 1,
        },
        PaymentAction::ValidPayment {
            amount: 200,
            tx_index: 2,
        },
        PaymentAction::ReplaySame { tx_index: 1 },
        PaymentAction::ReplayDifferentAmount {
            tx_index: 2,
            alt_amount: 500,
        },
    ];
    let (env, client, contract_id, invoice_id) = edge_setup(1_000);
    let forward = run_sequence(&env, &client, &contract_id, &invoice_id, 1_000, &actions);
    let reordered = reorder_replays_after_first_seen(&actions);
    let (env2, client2, contract_id2, invoice_id2) = edge_setup(1_000);
    let reversed = run_sequence(&env2, &client2, &contract_id2, &invoice_id2, 1_000, &reordered);

    assert_eq!(forward.total_paid, reversed.total_paid);
    assert_eq!(forward.payment_count, reversed.payment_count);
}
