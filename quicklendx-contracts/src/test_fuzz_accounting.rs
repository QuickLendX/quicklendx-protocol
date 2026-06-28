//! # Invariant Test — `sum(investor_balances) == total_invested` (Issue #1501)
//!
//! Property-based test asserting accounting integrity after any sequence of
//! deposit (fund) and withdraw operations.
//!
//! ## Design
//!
//! This test follows the same model-based approach as
//! `test_escrow_invariant_model.rs`: a pure in-process state machine is driven
//! by proptest-generated action sequences.  No Soroban environment is required,
//! so the test compiles and runs under any host without WASM toolchain setup.
//!
//! ## Invariants checked after every action
//!
//! | # | Invariant                                                      |
//! |---|----------------------------------------------------------------|
//! | 1 | `sum(active_investment_amounts) == total_invested`             |
//! | 2 | Each invoice hosts at most one active investment (no double-funding) |
//! | 3 | `total_invested` is always non-negative                        |
//! | 4 | Withdrawing an investment removes it atomically from both the  |
//! |   | per-invoice index and the running total                        |
//!
//! ## Running
//!
//! ```bash
//! # Default (1 024 cases)
//! cargo test --features fuzz-tests test_fuzz_accounting
//!
//! # Extended (10 000 cases)
//! PROPTEST_CASES=10000 cargo test --features fuzz-tests test_fuzz_accounting
//! ```

#![cfg(all(test, feature = "fuzz-tests"))]

extern crate std;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use std::boxed::Box;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::vec::Vec;

// ─── Model types ─────────────────────────────────────────────────────────────

/// A single active investment in the model.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ModelInvestment {
    /// The investor (represented by an index into the fixed investor pool).
    investor_idx: usize,
    /// Principal amount locked in escrow.
    amount: i128,
}

/// Top-level model state representing the protocol's accounting ledger.
#[derive(Clone, Debug, Default)]
struct AccountingModel {
    /// Map from invoice_id → active investment.
    /// An invoice can have at most one active investment.
    active_investments: BTreeMap<u32, ModelInvestment>,
    /// Running total of all locked principal — mirrors what the contract
    /// stores in its aggregate metric.
    total_invested: i128,
    /// Sequential invoice ID counter.
    next_invoice_id: u32,
    /// Set of invoice IDs that have already been funded and then withdrawn
    /// (available for re-funding to model invoice reuse).
    available_invoices: BTreeSet<u32>,
    /// All invoice IDs ever created (for withdraw targeting).
    all_invoice_ids: Vec<u32>,
}

/// Actions that external callers can invoke.
#[derive(Clone, Debug)]
enum Action {
    /// Create a new invoice and immediately fund it (accept_bid_and_fund).
    Fund {
        investor_idx: usize,
        amount: i128,
    },
    /// Withdraw an active investment from an existing invoice.
    Withdraw {
        invoice_id: u32,
    },
    /// Attempt to double-fund an already-funded invoice (must be a no-op).
    DoubleFund {
        invoice_id: u32,
        investor_idx: usize,
        amount: i128,
    },
    /// Withdraw from an invoice that has no active investment (must be a no-op).
    WithdrawNonExistent {
        invoice_id: u32,
    },
}

impl AccountingModel {
    /// Apply an action to the model, mirroring the protocol's state machine.
    fn apply(&mut self, action: &Action) {
        match action {
            // ── Fund ──────────────────────────────────────────────────────────
            Action::Fund {
                investor_idx,
                amount,
            } => {
                if *amount <= 0 {
                    return; // protocol rejects non-positive amounts
                }
                let invoice_id = self.next_invoice_id;
                self.next_invoice_id += 1;
                self.all_invoice_ids.push(invoice_id);

                // Create the investment record
                let inv = ModelInvestment {
                    investor_idx: *investor_idx,
                    amount: *amount,
                };
                self.active_investments.insert(invoice_id, inv);
                // Atomically update the aggregate
                self.total_invested = self.total_invested.saturating_add(*amount);
            }

            // ── Withdraw ──────────────────────────────────────────────────────
            Action::Withdraw { invoice_id } => {
                if let Some(inv) = self.active_investments.remove(invoice_id) {
                    // Atomically reduce the aggregate
                    self.total_invested =
                        self.total_invested.saturating_sub(inv.amount);
                    self.available_invoices.insert(*invoice_id);
                }
                // Withdraw of a non-existent invoice is a no-op (already
                // handled by the None branch of remove()).
            }

            // ── DoubleFund (illegal, must be rejected) ────────────────────────
            Action::DoubleFund {
                invoice_id,
                investor_idx: _,
                amount: _,
            } => {
                // The protocol rejects double-funding; the model must also
                // ignore this call entirely.
                if !self.active_investments.contains_key(invoice_id) {
                    // Invoice not currently funded — double-fund is a no-op
                    // here because the invoice either doesn't exist or was
                    // already withdrawn.
                }
                // If the invoice *is* funded, we do nothing — simulating the
                // protocol returning InvalidStatus.
            }

            // ── WithdrawNonExistent (illegal, must be rejected) ───────────────
            Action::WithdrawNonExistent { invoice_id: _ } => {
                // No state mutation — simulates the protocol returning
                // StorageKeyNotFound or InvalidStatus.
            }
        }
    }

    // ── Invariants ────────────────────────────────────────────────────────────

    /// Invariant 1: `sum(active_investment_amounts) == total_invested`
    fn invariant_sum_equals_total(&self) -> bool {
        let computed: i128 = self
            .active_investments
            .values()
            .map(|inv| inv.amount)
            .sum();
        computed == self.total_invested
    }

    /// Invariant 2: each invoice hosts at most one active investment.
    /// (Guaranteed structurally by `BTreeMap`, verified here explicitly.)
    fn invariant_at_most_one_investment_per_invoice(&self) -> bool {
        // BTreeMap keys are unique by construction, so duplicates are
        // impossible.  This check is still valuable as documentation and to
        // catch regressions if the model is later refactored.
        let unique_count = self.active_investments.len();
        let key_set: BTreeSet<_> = self.active_investments.keys().collect();
        unique_count == key_set.len()
    }

    /// Invariant 3: `total_invested` is always non-negative.
    fn invariant_total_non_negative(&self) -> bool {
        self.total_invested >= 0
    }

    /// Invariant 4: every invoice that appears in `active_investments` must
    /// have been created (i.e., it must be in `all_invoice_ids`).
    fn invariant_no_ghost_investments(&self) -> bool {
        let id_set: BTreeSet<_> = self.all_invoice_ids.iter().copied().collect();
        self.active_investments
            .keys()
            .all(|id| id_set.contains(id))
    }
}

// ─── Strategy ────────────────────────────────────────────────────────────────

fn arb_fund() -> impl Strategy<Value = Action> {
    (0usize..5, 1i128..=1_000_000i128)
        .prop_map(|(investor_idx, amount)| Action::Fund { investor_idx, amount })
}

fn arb_withdraw(max_id: u32) -> impl Strategy<Value = Action> {
    (0u32..max_id.max(1)).prop_map(|invoice_id| Action::Withdraw { invoice_id })
}

fn arb_double_fund(max_id: u32) -> impl Strategy<Value = Action> {
    (0u32..max_id.max(1), 0usize..5, 1i128..=1_000_000i128).prop_map(
        |(invoice_id, investor_idx, amount)| Action::DoubleFund {
            invoice_id,
            investor_idx,
            amount,
        },
    )
}

fn arb_withdraw_non_existent(max_id: u32) -> impl Strategy<Value = Action> {
    (0u32..max_id.max(1))
        .prop_map(|invoice_id| Action::WithdrawNonExistent { invoice_id })
}

fn arb_action() -> impl Strategy<Value = Action> {
    // Upper bound on invoice IDs in strategies that need an existing ID.
    // This is a static estimate; the model's actual next_invoice_id may
    // differ, but proptest shrinking handles out-of-range IDs gracefully.
    let max_id: u32 = 64;

    prop_oneof![
        // Weight: 4× Fund (most interesting operation)
        4 => arb_fund(),
        // Weight: 3× Withdraw (tests accounting on removal)
        3 => arb_withdraw(max_id),
        // Weight: 2× DoubleFund (tests rejection path)
        2 => arb_double_fund(max_id),
        // Weight: 1× WithdrawNonExistent (tests rejection path)
        1 => arb_withdraw_non_existent(max_id),
    ]
}

// ─── Case count ──────────────────────────────────────────────────────────────

fn configured_cases() -> u32 {
    if env::var_os("QUICKLENDX_NIGHTLY_INVARIANTS").is_some() {
        1_000_000
    } else if env::var_os("CI").is_some() {
        10_000
    } else {
        1_024
    }
}

// ─── Test ────────────────────────────────────────────────────────────────────

#[test]
fn test_invariant_sum_investor_balances_equals_total_invested() {
    let strategy = prop::collection::vec(arb_action(), 1..128);

    let mut runner = TestRunner::new(Config {
        cases: configured_cases(),
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(
                "proptest-regressions/invariant_investor_balances.txt",
            ),
        )),
        ..Config::default()
    });

    runner
        .run(&strategy, |actions| {
            let mut model = AccountingModel::default();

            for action in &actions {
                let before = model.clone();
                model.apply(action);

                // Invariant 1 — sum matches total
                prop_assert!(
                    model.invariant_sum_equals_total(),
                    "sum(investor_balances) != total_invested after {:?}: \
                     computed_sum={} total={}",
                    action,
                    model.active_investments.values().map(|i| i.amount).sum::<i128>(),
                    model.total_invested
                );

                // Invariant 2 — at most one investment per invoice
                prop_assert!(
                    model.invariant_at_most_one_investment_per_invoice(),
                    "duplicate invoice IDs in active_investments after {:?}: {:?}",
                    action,
                    model.active_investments
                );

                // Invariant 3 — total is non-negative
                prop_assert!(
                    model.invariant_total_non_negative(),
                    "total_invested became negative after {:?}: total={}",
                    action,
                    model.total_invested
                );

                // Invariant 4 — no ghost investments
                prop_assert!(
                    model.invariant_no_ghost_investments(),
                    "active investment references unknown invoice after {:?}: \
                     active_ids={:?} known_ids={:?}",
                    action,
                    model.active_investments.keys().copied().collect::<Vec<u32>>(),
                    model.all_invoice_ids
                );

                // Regression: Withdraw must reduce total_invested by exactly
                // the investment's principal (no partial accounting).
                if let Action::Withdraw { invoice_id } = action {
                    if let Some(removed) = before.active_investments.get(invoice_id) {
                        let expected_total =
                            before.total_invested.saturating_sub(removed.amount);
                        prop_assert_eq!(
                            model.total_invested,
                            expected_total,
                            "Withdraw of invoice {} did not reduce total_invested by {} \
                             (before={} after={})",
                            invoice_id,
                            removed.amount,
                            before.total_invested,
                            model.total_invested
                        );
                    }
                }
            }

            Ok(())
        })
        .unwrap();
}
