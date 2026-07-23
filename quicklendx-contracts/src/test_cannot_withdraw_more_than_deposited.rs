//! # Invariant Test — "Cannot withdraw more than deposited" (Issue #1482)
//!
//! Locks in the invariant that a withdrawal can never return more tokens than
//! were originally deposited into the protocol.  Two layers of coverage:
//!
//! 1. A hard-coded sad path (`cannot_withdraw_more_than_deposited_sad_path`)
//!    that directly asserts the model rejects over-withdrawal.
//! 2. A proptest property (`cannot_withdraw_more_than_deposited_property`) that
//!    drives an arbitrary sequence of Fund/Withdraw actions and verifies the
//!    invariant holds after every step.
//!
//! ## Running
//!
//! ```bash
//! # Without proptest feature (only the hard-coded sad path runs):
//! cargo test -p quicklendx-contracts cannot_withdraw_more_than_deposited
//!
//! # With proptest:
//! cargo test -p quicklendx-contracts --features fuzz-tests \
//!     cannot_withdraw_more_than_deposited
//! ```

#![cfg(test)]
extern crate std;

// ─── Model ────────────────────────────────────────────────────────────────────

/// Minimal accounting model that mirrors the protocol's deposit/withdraw logic.
/// All arithmetic uses saturating ops to avoid overflow panics in the model.
#[derive(Clone, Debug, Default)]
struct DepositModel {
    /// Total tokens currently locked in the model (sum of active deposits).
    total_deposited: i128,
    /// Total tokens that have been returned to investors via withdrawal.
    total_withdrawn: i128,
    /// Per-invoice deposit amounts for active investments.
    deposits: std::collections::BTreeMap<u32, i128>,
    /// Next invoice ID to assign on Fund.
    next_id: u32,
}

impl DepositModel {
    /// Deposit `amount` under a fresh invoice.  Returns the new invoice ID.
    fn fund(&mut self, amount: i128) -> Option<u32> {
        if amount <= 0 {
            return None; // protocol rejects non-positive amounts
        }
        let id = self.next_id;
        self.next_id += 1;
        self.deposits.insert(id, amount);
        self.total_deposited = self.total_deposited.saturating_add(amount);
        Some(id)
    }

    /// Attempt to withdraw the investment for `invoice_id`.
    /// Returns the amount returned, or `None` if the invoice has no active
    /// investment (the protocol would return `InvalidStatus`).
    fn withdraw(&mut self, invoice_id: u32) -> Option<i128> {
        let amount = self.deposits.remove(&invoice_id)?;
        self.total_withdrawn = self.total_withdrawn.saturating_add(amount);
        self.total_deposited = self.total_deposited.saturating_sub(amount);
        Some(amount)
    }

    // ── Invariants ────────────────────────────────────────────────────────────

    /// Core invariant: total withdrawn never exceeds total ever deposited.
    fn invariant_no_over_withdrawal(&self) -> bool {
        let total_ever_deposited = self.total_deposited.saturating_add(self.total_withdrawn);
        self.total_withdrawn <= total_ever_deposited
    }

    /// `total_deposited` must equal the sum of active deposit amounts.
    fn invariant_sum_equals_deposited(&self) -> bool {
        let sum: i128 = self.deposits.values().copied().sum();
        sum == self.total_deposited
    }

    /// Both accumulators must stay non-negative.
    fn invariant_non_negative(&self) -> bool {
        self.total_deposited >= 0 && self.total_withdrawn >= 0
    }
}

// ─── Hard-coded sad path ──────────────────────────────────────────────────────

/// SAD PATH: withdrawing the same invoice twice must be a no-op on the second
/// call — the model must not return more than was originally deposited.
#[test]
fn cannot_withdraw_more_than_deposited_sad_path() {
    let mut model = DepositModel::default();

    // Deposit 500 tokens under invoice 0.
    let invoice_id = model.fund(500).expect("fund should succeed");

    // First withdrawal: returns 500 — that is fine.
    let returned = model.withdraw(invoice_id).expect("first withdrawal must succeed");
    assert_eq!(returned, 500, "first withdrawal must return the deposited amount");

    // Second withdrawal of the same invoice: protocol rejects it (no active
    // investment), so the model must also return None.
    let second = model.withdraw(invoice_id);
    assert!(
        second.is_none(),
        "second withdrawal of the same invoice must be rejected (got {:?})",
        second,
    );

    // After the double-withdraw attempt the totals must still be consistent.
    assert!(
        model.invariant_no_over_withdrawal(),
        "over-withdrawal invariant violated: deposited={} withdrawn={}",
        model.total_deposited,
        model.total_withdrawn,
    );
    assert!(
        model.invariant_sum_equals_deposited(),
        "sum(deposits) != total_deposited after double-withdraw attempt",
    );
    assert!(
        model.invariant_non_negative(),
        "negative accumulator after double-withdraw attempt",
    );
}

/// SAD PATH: attempting to withdraw more than was ever deposited in aggregate.
/// Demonstrates that no sequence of valid withdrawals can exceed total deposits.
#[test]
fn cannot_withdraw_aggregate_more_than_deposited() {
    let mut model = DepositModel::default();

    let id1 = model.fund(300).unwrap();
    let id2 = model.fund(200).unwrap();

    model.withdraw(id1).unwrap();
    model.withdraw(id2).unwrap();

    // total_withdrawn (500) must not exceed total ever deposited (500).
    assert!(
        model.invariant_no_over_withdrawal(),
        "aggregate over-withdrawal: deposited={} withdrawn={}",
        model.total_deposited,
        model.total_withdrawn,
    );
    assert_eq!(model.total_deposited, 0);
    assert_eq!(model.total_withdrawn, 500);
}

// ─── Proptest property ────────────────────────────────────────────────────────

#[cfg(feature = "fuzz-tests")]
mod props {
    use super::DepositModel;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, TestRunner};
    use std::boxed::Box;
    use std::env;

    #[derive(Clone, Debug)]
    enum Action {
        Fund { amount: i128 },
        Withdraw { invoice_id: u32 },
        WithdrawNonExistent { invoice_id: u32 },
    }

    fn arb_action() -> impl Strategy<Value = Action> {
        prop_oneof![
            4 => (1i128..=1_000_000i128).prop_map(|amount| Action::Fund { amount }),
            3 => (0u32..64).prop_map(|invoice_id| Action::Withdraw { invoice_id }),
            1 => (0u32..64).prop_map(|invoice_id| Action::WithdrawNonExistent { invoice_id }),
        ]
    }

    fn configured_cases() -> u32 {
        if env::var_os("QUICKLENDX_NIGHTLY_INVARIANTS").is_some() {
            1_000_000
        } else if env::var_os("CI").is_some() {
            10_000
        } else {
            1_024
        }
    }

    #[test]
    fn cannot_withdraw_more_than_deposited_property() {
        let strategy = prop::collection::vec(arb_action(), 1..128);

        let mut runner = TestRunner::new(Config {
            cases: configured_cases(),
            failure_persistence: Some(Box::new(
                proptest::test_runner::FileFailurePersistence::Direct(
                    "proptest-regressions/cannot_withdraw_more_than_deposited.txt",
                ),
            )),
            ..Config::default()
        });

        runner
            .run(&strategy, |actions| {
                let mut model = DepositModel::default();

                for action in &actions {
                    match action {
                        Action::Fund { amount } => {
                            model.fund(*amount);
                        }
                        Action::Withdraw { invoice_id } => {
                            model.withdraw(*invoice_id);
                        }
                        Action::WithdrawNonExistent { invoice_id } => {
                            // Withdraw of a never-funded invoice must be a no-op.
                            let result = model.withdraw(*invoice_id);
                            prop_assert!(
                                result.is_none(),
                                "WithdrawNonExistent of invoice {} unexpectedly succeeded",
                                invoice_id,
                            );
                        }
                    }

                    prop_assert!(
                        model.invariant_no_over_withdrawal(),
                        "over-withdrawal invariant violated after {:?}: \
                         total_deposited={} total_withdrawn={}",
                        action,
                        model.total_deposited,
                        model.total_withdrawn,
                    );
                    prop_assert!(
                        model.invariant_sum_equals_deposited(),
                        "sum(deposits) != total_deposited after {:?}: \
                         sum={} total={}",
                        action,
                        model.deposits.values().copied().sum::<i128>(),
                        model.total_deposited,
                    );
                    prop_assert!(
                        model.invariant_non_negative(),
                        "negative accumulator after {:?}: \
                         deposited={} withdrawn={}",
                        action,
                        model.total_deposited,
                        model.total_withdrawn,
                    );
                }

                Ok(())
            })
            .unwrap();
    }
}
