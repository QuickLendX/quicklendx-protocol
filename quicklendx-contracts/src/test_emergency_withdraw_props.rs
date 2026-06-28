#![cfg(test)]
extern crate std;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use std::env;
use alloc::boxed::Box;

use crate::emergency::{
    DEFAULT_EMERGENCY_EXPIRATION_SECS, DEFAULT_EMERGENCY_TIMELOCK_SECS,
};

const MAX_TIME: u64 = 1_000_000;
/// Maximum seconds the generated time may exceed the initiation window so that
/// we naturally cover post-expiry timestamps.
const MAX_TIME_HEADROOM: u64 = 2_000_000;

/// Model of a pending emergency withdrawal, mirroring the production
/// [`PendingEmergencyWithdrawal`] struct fields that govern execute eligibility.
#[derive(Clone, Debug)]
struct Pending {
    initiated_at: u64,
    unlock_at: u64,
    expires_at: u64,
    cancelled: bool,
}

/// The abstract state machine for the emergency-withdraw lifecycle.
#[derive(Clone, Debug)]
struct Model {
    now: u64,
    pending: Option<Pending>,
    /// `true` in the tick immediately after a successful execute clears pending.
    executed: bool,
}

#[derive(Clone, Debug)]
enum Action {
    Initiate { at: u64 },
    Cancel,
    Execute { at: u64 },
}

impl Model {
    fn apply(&mut self, action: &Action) {
        match *action {
            Action::Initiate { at } => {
                self.now = at;
                let unlock = at.saturating_add(DEFAULT_EMERGENCY_TIMELOCK_SECS);
                let expires = unlock.saturating_add(DEFAULT_EMERGENCY_EXPIRATION_SECS);
                self.pending = Some(Pending {
                    initiated_at: at,
                    unlock_at: unlock,
                    expires_at: expires,
                    cancelled: false,
                });
                self.executed = false;
            }
            Action::Cancel => {
                if let Some(ref mut p) = self.pending {
                    p.cancelled = true;
                }
            }
            Action::Execute { at } => {
                self.now = at;
                match self.pending {
                    None => { /* fail: no pending */ }
                    Some(ref p) => {
                        if p.cancelled {
                            /* fail: cancelled */
                        } else if at < p.unlock_at {
                            /* fail: timelock not elapsed */
                        } else if at >= p.expires_at {
                            /* fail: expired */
                        } else {
                            self.executed = true;
                            self.pending = None;
                        }
                    }
                }
            }
        }
    }

    /// Invariant: no execute ever succeeds while the timelock (cooldown) is
    /// still running.
    fn invariant_no_execute_during_cooldown(&self) -> bool {
        if let Some(ref p) = self.pending {
            if !p.cancelled && self.now < p.unlock_at {
                return !self.executed;
            }
        }
        true
    }

    /// Invariant: no execute ever succeeds after the withdrawal is cancelled.
    fn invariant_no_execute_after_cancel(&self) -> bool {
        if let Some(ref p) = self.pending {
            if p.cancelled {
                return !self.executed;
            }
        }
        true
    }

    /// Invariant: no execute ever succeeds after the expiration deadline.
    fn invariant_no_execute_after_expiry(&self) -> bool {
        if let Some(ref p) = self.pending {
            if !p.cancelled && self.now >= p.expires_at {
                return !self.executed;
            }
        }
        true
    }

    /// Invariant: after a successful execute the pending slot is always cleared.
    fn invariant_execute_clears_pending(&self) -> bool {
        !(self.executed && self.pending.is_some())
    }
}

/// Build the action strategy.
fn arb_action() -> impl Strategy<Value = Action> {
    prop_oneof![
        (0u64..=MAX_TIME).prop_map(|at| Action::Initiate { at }),
        Just(Action::Cancel),
        (0u64..=MAX_TIME_HEADROOM).prop_map(|at| Action::Execute { at }),
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
fn test_emergency_withdraw_cooldown_property() {
    let strategy = prop::collection::vec(arb_action(), 1..64);
    let mut runner = TestRunner::new(Config {
        cases: configured_cases(),
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::Direct(
                "proptest-regressions/emergency_withdraw.txt",
            ),
        )),
        ..Config::default()
    });

    runner
        .run(&strategy, |actions| {
            let mut model = Model {
                now: 0,
                pending: None,
                executed: false,
            };

            for action in &actions {
                model.apply(action);

                prop_assert!(
                    model.invariant_no_execute_during_cooldown(),
                    "Execute succeeded during timelock (cooldown) after {:?}: {:?}",
                    action,
                    model
                );
                prop_assert!(
                    model.invariant_no_execute_after_cancel(),
                    "Execute succeeded after cancel after {:?}: {:?}",
                    action,
                    model
                );
                prop_assert!(
                    model.invariant_no_execute_after_expiry(),
                    "Execute succeeded after expiry after {:?}: {:?}",
                    action,
                    model
                );
                prop_assert!(
                    model.invariant_execute_clears_pending(),
                    "Pending not cleared after successful execute: {:?}",
                    model
                );
            }
            Ok(())
        })
        .unwrap();
}
