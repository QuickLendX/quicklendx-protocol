#![cfg(test)]
extern crate std;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use std::env;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EscrowState {
    None,
    Held { amount: i128 },
    Released { amount: i128 },
    Refunded { amount: i128 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InvoiceState {
    Verified,
    Funded,
    Settled,
    Refunded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InvestmentState {
    None,
    Active { principal: i128 },
    Repaid { principal: i128 },
    Refunded { principal: i128 },
}

/// Public escrow lifecycle calls and mode toggles driven by the model.
///
/// The state machine intentionally includes legal and illegal attempts.  Illegal
/// calls model externally observable public-entrypoint rejections rather than
/// mutating private storage directly.  Examples include replaying `accept_bid`,
/// double-refunding, and trying to refund after release.
#[derive(Clone, Debug)]
enum Action {
    AcceptBid { amount: i128 },
    ReplayAcceptBid,
    ReleaseEscrow,
    RefundEscrow,
    DoubleRefund,
    RefundAfterRelease,
    Pause,
    Unpause,
    EmergencyModeOn,
    EmergencyModeOff,
}

#[derive(Clone, Debug)]
struct EscrowModel {
    invoice: InvoiceState,
    investment: InvestmentState,
    escrow: EscrowState,
    held_count: u8,
    principal: i128,
    released: i128,
    refunded: i128,
    paused: bool,
    emergency: bool,
}

impl Default for EscrowModel {
    fn default() -> Self {
        Self {
            invoice: InvoiceState::Verified,
            investment: InvestmentState::None,
            escrow: EscrowState::None,
            held_count: 0,
            principal: 0,
            released: 0,
            refunded: 0,
            paused: false,
            emergency: false,
        }
    }
}

impl EscrowModel {
    fn apply(&mut self, action: Action) {
        match action {
            Action::Pause => self.paused = true,
            Action::Unpause => self.paused = false,
            Action::EmergencyModeOn => self.emergency = true,
            Action::EmergencyModeOff => self.emergency = false,
            Action::AcceptBid { amount } => self.accept_bid(amount),
            Action::ReplayAcceptBid => self.accept_bid(self.principal.max(1)),
            Action::ReleaseEscrow => self.release_escrow(),
            Action::RefundEscrow | Action::DoubleRefund => self.refund_escrow(),
            Action::RefundAfterRelease => {
                self.release_escrow();
                self.refund_escrow();
            }
        }
    }

    fn accept_bid(&mut self, amount: i128) {
        if self.paused || self.emergency || amount <= 0 {
            return;
        }
        if self.invoice != InvoiceState::Verified || self.held_count != 0 {
            return;
        }
        if self.escrow != EscrowState::None || self.investment != InvestmentState::None {
            return;
        }

        self.invoice = InvoiceState::Funded;
        self.investment = InvestmentState::Active { principal: amount };
        self.escrow = EscrowState::Held { amount };
        self.held_count = 1;
        self.principal = amount;
    }

    fn release_escrow(&mut self) {
        if self.paused || self.emergency {
            return;
        }
        let EscrowState::Held { amount } = self.escrow else {
            return;
        };
        if self.invoice != InvoiceState::Funded {
            return;
        }

        self.escrow = EscrowState::Released { amount };
        self.invoice = InvoiceState::Settled;
        self.investment = InvestmentState::Repaid { principal: amount };
        self.held_count = 0;
        self.released = self.released.saturating_add(amount);
    }

    fn refund_escrow(&mut self) {
        if self.paused || self.emergency {
            return;
        }
        let EscrowState::Held { amount } = self.escrow else {
            return;
        };
        if self.invoice != InvoiceState::Funded {
            return;
        }

        self.escrow = EscrowState::Refunded { amount };
        self.invoice = InvoiceState::Refunded;
        self.investment = InvestmentState::Refunded { principal: amount };
        self.held_count = 0;
        self.refunded = self.refunded.saturating_add(amount);
    }

    /// Oracle predicate: no invoice may have more than one Held escrow.
    fn invariant_at_most_one_held(&self) -> bool {
        self.held_count <= 1
    }

    /// Oracle predicate: Released and Refunded are terminal escrow states.
    fn invariant_terminal_states(&self, before: &Self) -> bool {
        match before.escrow {
            EscrowState::Released { amount } => self.escrow == EscrowState::Released { amount },
            EscrowState::Refunded { amount } => self.escrow == EscrowState::Refunded { amount },
            _ => true,
        }
    }

    /// Oracle predicate: released plus refunded movement never exceeds principal
    /// and equals the principal once escrow exits Held.
    fn invariant_movements_match_principal(&self) -> bool {
        let moved = self.released.saturating_add(self.refunded);
        if moved > self.principal {
            return false;
        }
        match self.escrow {
            EscrowState::Released { .. } | EscrowState::Refunded { .. } => moved == self.principal,
            EscrowState::Held { .. } => moved == 0,
            EscrowState::None => moved == 0 && self.principal == 0,
        }
    }

    /// Oracle predicate: Invoice, Investment, and Escrow statuses remain coherent
    /// after every modeled public transition.
    fn invariant_cross_module_status_coherence(&self) -> bool {
        match (self.invoice, self.investment, self.escrow) {
            (InvoiceState::Verified, InvestmentState::None, EscrowState::None) => true,
            (
                InvoiceState::Funded,
                InvestmentState::Active { principal },
                EscrowState::Held { amount },
            ) => principal == amount,
            (
                InvoiceState::Settled,
                InvestmentState::Repaid { principal },
                EscrowState::Released { amount },
            ) => principal == amount,
            (
                InvoiceState::Refunded,
                InvestmentState::Refunded { principal },
                EscrowState::Refunded { amount },
            ) => principal == amount,
            _ => false,
        }
    }
}

fn arb_action() -> impl Strategy<Value = Action> {
    prop_oneof![
        (1i128..=1_000_000i128).prop_map(|amount| Action::AcceptBid { amount }),
        Just(Action::ReplayAcceptBid),
        Just(Action::ReleaseEscrow),
        Just(Action::RefundEscrow),
        Just(Action::DoubleRefund),
        Just(Action::RefundAfterRelease),
        Just(Action::Pause),
        Just(Action::Unpause),
        Just(Action::EmergencyModeOn),
        Just(Action::EmergencyModeOff),
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
fn test_escrow_invariant_model() {
    let strategy = prop::collection::vec(arb_action(), 1..128);
    let mut runner = TestRunner::new(Config {
        cases: configured_cases(),
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(
                "proptest-regressions/escrow_invariant_model.txt",
            ),
        )),
        ..Config::default()
    });

    runner
        .run(&strategy, |actions| {
            let mut model = EscrowModel::default();
            for action in actions {
                let before = model.clone();
                model.apply(action.clone());

                prop_assert!(
                    model.invariant_at_most_one_held(),
                    "more than one held escrow after {:?}: {:?}",
                    action,
                    model
                );
                prop_assert!(
                    model.invariant_terminal_states(&before),
                    "terminal escrow changed after {:?}: before={:?} after={:?}",
                    action,
                    before,
                    model
                );
                prop_assert!(
                    model.invariant_movements_match_principal(),
                    "movement/principal mismatch after {:?}: {:?}",
                    action,
                    model
                );
                prop_assert!(
                    model.invariant_cross_module_status_coherence(),
                    "cross-module status mismatch after {:?}: {:?}",
                    action,
                    model
                );
            }
            Ok(())
        })
        .unwrap();
}
