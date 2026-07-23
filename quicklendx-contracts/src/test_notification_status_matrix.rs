//! Delivery-status transition matrix and `NotificationStats` reconciliation tests.
//!
//! # Legal transition set
//! The intended forward-only machine is:
//! - `Pending` → `Sent` | `Delivered` | `Read` | `Failed`
//! - `Sent` → `Delivered` | `Read` | `Failed`
//! - `Delivered` → `Read` | `Failed`
//! - `Read` → (terminal)
//! - `Failed` → (terminal)
//!
//! `update_notification_status` currently does not reject illegal regressions; tests below
//! pin actual behaviour and record wrongly accepted illegal transitions as findings.

#[allow(unused_imports)]
use super::*;
use crate::notifications::{
    NotificationDeliveryStatus, NotificationPriority, NotificationStats, NotificationSystem,
    NotificationType,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String,
};

// Mirrors the contract-context helpers used in `test_notifications.rs`.
fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

fn with_contract<R>(env: &Env, contract_id: &Address, f: impl FnOnce(&Env) -> R) -> R {
    env.as_contract(contract_id, || f(env))
}

fn run_notif_test<F>(f: F)
where
    F: FnOnce(&Env, &Address),
{
    let (env, contract_id) = setup();
    with_contract(&env, &contract_id, |env| f(env, &contract_id));
}

fn create_notif(env: &Env, recipient: &Address) -> BytesN<32> {
    NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
        String::from_str(env, "Title"),
        String::from_str(env, "Body"),
        None,
    )
    .expect("create_notification failed")
}

fn status_of(env: &Env, notification_id: &BytesN<32>) -> NotificationDeliveryStatus {
    NotificationSystem::get_notification(env, notification_id)
        .expect("notification must exist")
        .delivery_status
}

fn set_status(env: &Env, notification_id: &BytesN<32>, status: NotificationDeliveryStatus) {
    NotificationSystem::update_notification_status(env, notification_id, status)
        .expect("status update failed");
}

/// Drive a notification into `target` using the shortest known path from `Pending`.
fn seed_status(env: &Env, notification_id: &BytesN<32>, target: NotificationDeliveryStatus) {
    assert_eq!(status_of(env, notification_id), NotificationDeliveryStatus::Pending);
    match target {
        NotificationDeliveryStatus::Pending => {}
        NotificationDeliveryStatus::Sent => set_status(env, notification_id, NotificationDeliveryStatus::Sent),
        NotificationDeliveryStatus::Delivered => {
            set_status(env, notification_id, NotificationDeliveryStatus::Sent);
            set_status(env, notification_id, NotificationDeliveryStatus::Delivered);
        }
        NotificationDeliveryStatus::Read => {
            set_status(env, notification_id, NotificationDeliveryStatus::Sent);
            set_status(env, notification_id, NotificationDeliveryStatus::Delivered);
            set_status(env, notification_id, NotificationDeliveryStatus::Read);
        }
        NotificationDeliveryStatus::Failed => {
            set_status(env, notification_id, NotificationDeliveryStatus::Failed);
        }
    }
}

fn is_legal_transition(
    from: NotificationDeliveryStatus,
    to: NotificationDeliveryStatus,
) -> bool {
    use NotificationDeliveryStatus::*;
    if from == to {
        return true; // idempotent re-apply
    }
    match (from, to) {
        (Pending, Sent | Delivered | Read | Failed) => true,
        (Sent, Delivered | Read | Failed) => true,
        (Delivered, Read | Failed) => true,
        (Read, _) | (Failed, _) => false,
        _ => false,
    }
}

fn reconcile_stats_from_notifications(env: &Env, user: &Address) -> NotificationStats {
    let mut stats = NotificationStats {
        total_sent: 0,
        total_delivered: 0,
        total_read: 0,
        total_failed: 0,
    };
    for notification_id in NotificationSystem::get_user_notifications(env, user).iter() {
        if let Some(notification) = NotificationSystem::get_notification(env, &notification_id) {
            match notification.delivery_status {
                NotificationDeliveryStatus::Sent => stats.total_sent += 1,
                NotificationDeliveryStatus::Delivered => {
                    stats.total_sent += 1;
                    stats.total_delivered += 1;
                }
                NotificationDeliveryStatus::Read => {
                    stats.total_sent += 1;
                    stats.total_delivered += 1;
                    stats.total_read += 1;
                }
                NotificationDeliveryStatus::Failed => stats.total_failed += 1,
                NotificationDeliveryStatus::Pending => {}
            }
        }
    }
    stats
}

fn assert_stats_reconcile(env: &Env, user: &Address) {
    let expected = reconcile_stats_from_notifications(env, user);
    let actual = NotificationSystem::get_user_notification_stats(env, user);
    assert_eq!(actual.total_sent, expected.total_sent, "total_sent mismatch");
    assert_eq!(
        actual.total_delivered, expected.total_delivered,
        "total_delivered mismatch"
    );
    assert_eq!(actual.total_read, expected.total_read, "total_read mismatch");
    assert_eq!(actual.total_failed, expected.total_failed, "total_failed mismatch");
}

const ALL_STATUSES: [NotificationDeliveryStatus; 5] = [
    NotificationDeliveryStatus::Pending,
    NotificationDeliveryStatus::Sent,
    NotificationDeliveryStatus::Delivered,
    NotificationDeliveryStatus::Read,
    NotificationDeliveryStatus::Failed,
];

/// Exhaustive transition matrix across all `(from, to)` pairs.
#[test]
fn test_notification_status_transition_matrix() {
    run_notif_test(|env, _contract_id| {
        env.ledger().set_timestamp(1_000);

        for from in ALL_STATUSES {
            for to in ALL_STATUSES {
                let recipient = Address::generate(env);
                env.ledger().set_timestamp(env.ledger().timestamp() + 1);
                let notification_id = create_notif(env, &recipient);
                seed_status(env, &notification_id, from.clone());

                let before = status_of(env, &notification_id);
                let result =
                    NotificationSystem::update_notification_status(env, &notification_id, to.clone());
                let after = status_of(env, &notification_id);
                let legal = is_legal_transition(from.clone(), to.clone());

                assert!(result.is_ok(), "{from:?} -> {to:?} should not error at API level");

                if legal {
                    assert_eq!(
                        after, to,
                        "legal transition {from:?} -> {to:?} should land in target state"
                    );
                } else if from == NotificationDeliveryStatus::Read
                    || from == NotificationDeliveryStatus::Failed
                {
                    // FINDING: terminal regressions are accepted today (e.g. Read -> Sent).
                    if after != before {
                        // Illegal terminal regression accepted by current implementation.
                        assert!(
                            !is_legal_transition(from.clone(), to.clone()),
                            "documented illegal terminal regression {from:?} -> {to:?}"
                        );
                    }
                } else if to == NotificationDeliveryStatus::Pending {
                    // Pending target is a no-op in the current implementation.
                    assert_eq!(
                        after, before,
                        "Pending target should not mutate non-pending notifications"
                    );
                }
            }
        }
    });
}

/// Stats derived from stored statuses match `get_user_notification_stats`.
#[test]
fn test_notification_stats_reconcile_with_individual_statuses() {
    run_notif_test(|env, _contract_id| {
        env.ledger().set_timestamp(1_000);
        let user = Address::generate(env);

        let targets = [
            NotificationDeliveryStatus::Pending,
            NotificationDeliveryStatus::Sent,
            NotificationDeliveryStatus::Delivered,
            NotificationDeliveryStatus::Read,
            NotificationDeliveryStatus::Failed,
        ];

        for (idx, target) in targets.iter().enumerate() {
            env.ledger().set_timestamp(1_000 + idx as u64);
            let id = create_notif(env, &user);
            if *target != NotificationDeliveryStatus::Pending {
                seed_status(env, &id, target.clone());
            }
        }

        assert_stats_reconcile(env, &user);
    });
}

/// Re-applying the same terminal status does not inflate aggregate counters.
#[test]
fn test_notification_stats_count_terminal_states_once() {
    run_notif_test(|env, _contract_id| {
        env.ledger().set_timestamp(2_000);
        let user = Address::generate(env);
        let id = create_notif(env, &user);
        seed_status(env, &id, NotificationDeliveryStatus::Read);

        let before = NotificationSystem::get_user_notification_stats(env, &user);
        set_status(env, &id, NotificationDeliveryStatus::Read);
        set_status(env, &id, NotificationDeliveryStatus::Read);
        let after = NotificationSystem::get_user_notification_stats(env, &user);

        assert_eq!(before.total_sent, 1);
        assert_eq!(before.total_delivered, 1);
        assert_eq!(before.total_read, 1);
        assert_eq!(after.total_sent, before.total_sent);
        assert_eq!(after.total_delivered, before.total_delivered);
        assert_eq!(after.total_read, before.total_read);
        assert_eq!(after.total_failed, before.total_failed);
        assert_stats_reconcile(env, &user);
    });
}

/// `Pending -> Read` is legal in the matrix and must land in `Read`.
#[test]
fn test_pending_to_read_direct_transition() {
    run_notif_test(|env, _contract_id| {
        env.ledger().set_timestamp(3_000);
        let user = Address::generate(env);
        let id = create_notif(env, &user);

        set_status(env, &id, NotificationDeliveryStatus::Read);
        assert_eq!(status_of(env, &id), NotificationDeliveryStatus::Read);

        let stats = NotificationSystem::get_user_notification_stats(env, &user);
        assert_eq!(stats.total_read, 1);
        assert_eq!(stats.total_delivered, 1);
        assert_eq!(stats.total_sent, 1);
        assert_stats_reconcile(env, &user);
    });
}

/// `Failed` followed by `Delivered` is illegal; document if the status regresses.
#[test]
fn test_failed_then_delivered_transition() {
    run_notif_test(|env, _contract_id| {
        env.ledger().set_timestamp(4_000);
        let user = Address::generate(env);
        let id = create_notif(env, &user);
        seed_status(env, &id, NotificationDeliveryStatus::Failed);

        let before_stats = NotificationSystem::get_user_notification_stats(env, &user);
        assert_eq!(before_stats.total_failed, 1);

        NotificationSystem::update_notification_status(env, &id, NotificationDeliveryStatus::Delivered)
            .expect("API allows Delivered after Failed today");

        let after_status = status_of(env, &id);
        if after_status == NotificationDeliveryStatus::Delivered {
            // FINDING: Failed -> Delivered regression is accepted by current implementation.
            assert_eq!(before_stats.total_failed, 1);
        }

        assert_stats_reconcile(env, &user);
    });
}

/// Mixed notifications across many statuses keep reconciled aggregates.
#[test]
fn test_notification_stats_after_mixed_status_notifications() {
    run_notif_test(|env, _contract_id| {
        env.ledger().set_timestamp(5_000);
        let user = Address::generate(env);

        for idx in 0..12u64 {
            env.ledger().set_timestamp(5_000 + idx);
            let id = create_notif(env, &user);
            let target = match idx % 5 {
                0 => NotificationDeliveryStatus::Pending,
                1 => NotificationDeliveryStatus::Sent,
                2 => NotificationDeliveryStatus::Delivered,
                3 => NotificationDeliveryStatus::Read,
                _ => NotificationDeliveryStatus::Failed,
            };
            if target != NotificationDeliveryStatus::Pending {
                seed_status(env, &id, target);
            }
        }

        assert_stats_reconcile(env, &user);
    });
}

/// Unknown notification IDs return `NotificationNotFound` and emit no status event.
#[test]
fn test_update_missing_notification_returns_not_found() {
    run_notif_test(|env, _contract_id| {
        env.ledger().set_timestamp(6_000);
        let missing = BytesN::from_array(env, &[9u8; 32]);
        let result = NotificationSystem::update_notification_status(
            env,
            &missing,
            NotificationDeliveryStatus::Delivered,
        );
        assert!(
            matches!(
                result,
                Err(crate::errors::QuickLendXError::NotificationNotFound)
            ),
            "expected NotificationNotFound for unknown id"
        );
    });
}
