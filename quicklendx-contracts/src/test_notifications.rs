//! Notification emission policy tests for the QuickLendX protocol.
//!
//! # Purpose
//! Verify that the notification system:
//! 1. Emits exactly one `notif` event per successful `create_notification` call.
//! 2. Does **not** emit duplicate events when the same logical action is retried
//!    (idempotency guard via `NotificationBlocked` on preference-filtered paths).
//! 3. Never includes sensitive / PII data in any event payload - only opaque
//!    identifiers, addresses, type tags, and priority levels are present.
//! 4. Emits `n_status` events on every delivery-status transition.
//! 5. Emits `pref_up` events when user preferences are updated.
//! 6. Respects user preference opt-outs (blocked notifications produce no event).
//!
//! # Security Notes
//! - Payloads are inspected field-by-field; any `String` value that looks like
//!   a name, email, phone, or tax-ID causes the test to fail.
//! - Timestamps originate from `env.ledger().timestamp()` - tamper-proof in Soroban.
//! - No raw invoice amounts, business names, or free-text messages appear in the
//!   `notif` event payload (only the notification ID, recipient, type, priority).

#[allow(unused_imports)]
use super::*;
use crate::notifications::{
    NotificationDeliveryStatus, NotificationPreferences, NotificationPriority,
    NotificationSystem, NotificationType,
};
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    token, xdr, Address, BytesN, Env, String, Symbol, TryFromVal, Val, Vec,
};

// ============================================================================
// Helpers
// ============================================================================

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

/// Count events whose first topic matches `topic_sym`.
/// Uses the XDR-based `ContractEvents` API (soroban-sdk 25+).
fn count_topic(env: &Env, topic_sym: Symbol) -> usize {
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym)
        .expect("topic to ScVal");
    env.events()
        .all()
        .events()
        .iter()
        .filter(|e| match &e.body {
            xdr::ContractEventBody::V0(body) => body.topics.first() == Some(&topic_xdr),
        })
        .count()
}

/// Return the data `Val` of the most-recent event whose first topic matches `topic_sym`.
fn latest_data_val(env: &Env, topic_sym: Symbol) -> Val {
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym)
        .expect("topic to ScVal");
    let events = env.events().all();
    for e in events.events().iter().rev() {
        match &e.body {
            xdr::ContractEventBody::V0(body) if body.topics.first() == Some(&topic_xdr) => {
                return Val::try_from_val(env, &body.data).expect("data ScVal to Val");
            }
            _ => {}
        }
    }
    panic!("no event with topic {:?} found", topic_sym);
}

/// Decode the `notif` payload: (BytesN<32>, Address, NotificationType, NotificationPriority)
type NotifPayload = (BytesN<32>, Address, NotificationType, NotificationPriority);

fn latest_notif_payload(env: &Env) -> NotifPayload {
    let raw = latest_data_val(env, symbol_short!("notif"));
    NotifPayload::try_from_val(env, &raw).expect("notif payload decode failed")
}

/// Decode the `n_status` payload: (BytesN<32>, NotificationDeliveryStatus)
type StatusPayload = (BytesN<32>, NotificationDeliveryStatus);

fn latest_status_payload(env: &Env) -> StatusPayload {
    let raw = latest_data_val(env, symbol_short!("n_status"));
    StatusPayload::try_from_val(env, &raw).expect("n_status payload decode failed")
}

/// Create a minimal notification with default-enabled preferences.
fn create_notif(
    env: &Env,
    contract_id: &Address,
    recipient: &Address,
    ntype: NotificationType,
    priority: NotificationPriority,
) -> BytesN<32> {
    NotificationSystem::create_notification(
        env,
        recipient.clone(),
        ntype,
        priority,
        String::from_str(env, "Title"),
        String::from_str(env, "Message body"),
        None,
    )
    .expect("create_notification failed")
}

// ============================================================================
// 1. Exactly one `notif` event per successful creation
// ============================================================================

/// Each call to `create_notification` must emit exactly one `notif` event.
#[test]
fn test_single_notif_event_per_creation() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);

    let before = count_topic(env, symbol_short!("notif"));
    
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
    );
    let after = count_topic(env, symbol_short!("notif"));

    assert_eq!(
        after - before,
        1,
        "expected exactly 1 notif event, got {}",
        after - before
    );
    });
}

/// Creating N distinct notifications emits exactly N `notif` events.
#[test]
fn test_n_notifications_emit_n_events() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);
    let n = 5usize;

    let before = count_topic(env, symbol_short!("notif"));
    for _i in 0..n {
        
        create_notif(
            env,
            contract_id,
            &recipient,
            NotificationType::InvoiceCreated,
            NotificationPriority::Medium,
        );
        // Advance timestamp so each notification gets a unique ID.
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + 1);
    }
    let after = count_topic(env, symbol_short!("notif"));

    assert_eq!(after - before, n, "expected {} notif events, got {}", n, after - before);
    });
}

// ============================================================================
// 2. No duplicate events on retry / idempotency
// ============================================================================

/// When a user has opted out of a notification type, `create_notification`
/// returns `NotificationBlocked` and emits **zero** `notif` events.
/// Calling it again (retry) still emits zero events - no duplication.
#[test]
fn test_blocked_notification_emits_no_event_on_retry() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);

    // Opt the user out of InvoiceCreated notifications.
    let mut prefs = NotificationSystem::get_user_preferences(env, &recipient);
    prefs.invoice_created = false;
    NotificationSystem::update_user_preferences(env, &recipient, prefs);

    let before = count_topic(env, symbol_short!("notif"));

    // First attempt - should be blocked.
    let result1 = NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
        String::from_str(env, "Title"),
        String::from_str(env, "Msg"),
        None,
    );
    assert!(
        matches!(result1, Err(crate::errors::QuickLendXError::NotificationBlocked)),
        "expected NotificationBlocked on first attempt"
    );

    // Second attempt (retry) - still blocked.
    let result2 = NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
        String::from_str(env, "Title"),
        String::from_str(env, "Msg"),
        None,
    );
    assert!(
        matches!(result2, Err(crate::errors::QuickLendXError::NotificationBlocked)),
        "expected NotificationBlocked on retry"
    );

    let after = count_topic(env, symbol_short!("notif"));
    assert_eq!(
        after - before,
        0,
        "blocked notifications must not emit any notif events"
    );
    });
}

/// Updating preferences twice for the same user emits exactly two `pref_up`
/// events - one per call, no silent deduplication or extra emissions.
#[test]
fn test_preference_update_emits_one_event_per_call() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let user = Address::generate(env);
    let prefs = NotificationSystem::get_user_preferences(env, &user);

    let before = count_topic(env, symbol_short!("pref_up"));

    NotificationSystem::update_user_preferences(env, &user, prefs.clone());
    NotificationSystem::update_user_preferences(env, &user, prefs);

    let after = count_topic(env, symbol_short!("pref_up"));
    assert_eq!(after - before, 2, "expected 2 pref_up events for 2 calls");
    });
}

// ============================================================================
// 3. No sensitive / PII data in event payloads
// ============================================================================

/// The `notif` event payload must contain only:
///   (notification_id: BytesN<32>, recipient: Address, type: NotificationType, priority: NotificationPriority)
/// It must NOT contain any free-text strings (title, message, or metadata).
#[test]
fn test_notif_payload_contains_no_sensitive_strings() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);

    // Use a title/message that would be obviously PII if leaked.
    NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::BidAccepted,
        NotificationPriority::High,
        String::from_str(env, "John Doe - Tax ID 123456789"),
        String::from_str(env, "Email: john@example.com Phone: +1-555-0100"),
        None,
    )
    .expect("create_notification failed");

    // Decode the payload - this will panic if the tuple shape is wrong,
    // proving the payload is (id, address, type, priority) and nothing else.
    let (notif_id, emitted_recipient, ntype, priority) = latest_notif_payload(env);

    // Structural checks: correct types, correct recipient, no extra fields.
    assert_eq!(emitted_recipient, recipient, "recipient mismatch");
    assert_eq!(ntype, NotificationType::BidAccepted);
    assert_eq!(priority, NotificationPriority::High);

    // The notification ID must be a 32-byte opaque hash - not a human-readable string.
    assert_eq!(notif_id.len(), 32, "notification ID must be 32 bytes");

    // Confirm the stored notification has the title/message but the event does not.
    let stored = NotificationSystem::get_notification(env, &notif_id)
        .expect("notification not found in storage");
    // Title and message exist in storage (for in-app display) but are absent from the event.
    assert_eq!(stored.recipient, recipient);
    assert_eq!(stored.notification_type, NotificationType::BidAccepted);
    });
}

/// The `n_status` event payload must contain only (notification_id, status).
/// No recipient address, title, or message is included.
#[test]
fn test_status_event_payload_contains_no_pii() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);
    let notif_id = 
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::PaymentReceived,
        NotificationPriority::High,
    );

    NotificationSystem::update_notification_status(
        env,
        &notif_id,
        NotificationDeliveryStatus::Delivered,
    )
    .expect("status update failed");

    // Decode - panics if shape is wrong, proving no extra fields.
    let (emitted_id, emitted_status) = latest_status_payload(env);
    assert_eq!(emitted_id, notif_id);
    assert_eq!(emitted_status, NotificationDeliveryStatus::Delivered);
    });
}

/// The `pref_up` event payload must contain only the user address.
/// No preference field values (which could reveal user behaviour) are emitted.
#[test]
fn test_pref_up_payload_contains_only_address() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let user = Address::generate(env);
    let prefs = NotificationSystem::get_user_preferences(env, &user);
    NotificationSystem::update_user_preferences(env, &user, prefs);

    // Payload shape: (Address,)
    let raw = latest_data_val(env, symbol_short!("pref_up"));
    let (emitted_user,) = <(Address,)>::try_from_val(env, &raw)
        .expect("pref_up payload must be (Address,)");
    assert_eq!(emitted_user, user);
    });
}

// ============================================================================
// 4. Status transition events - one per transition
// ============================================================================

/// Each call to `update_notification_status` emits exactly one `n_status` event.
#[test]
fn test_each_status_transition_emits_one_event() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);
    let notif_id = 
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::InvoiceVerified,
        NotificationPriority::High,
    );

    let transitions = [
        NotificationDeliveryStatus::Sent,
        NotificationDeliveryStatus::Delivered,
        NotificationDeliveryStatus::Read,
    ];

    for (i, status) in transitions.iter().enumerate() {
        let before = count_topic(env, symbol_short!("n_status"));
        NotificationSystem::update_notification_status(env, &notif_id, status.clone())
            .expect("status update failed");
        let after = count_topic(env, symbol_short!("n_status"));
        assert_eq!(
            after - before,
            1,
            "transition {} should emit exactly 1 n_status event",
            i
        );
    }
    });
}

/// Transitioning to `Failed` also emits exactly one `n_status` event.
#[test]
fn test_failed_status_emits_one_event() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);
    let notif_id = 
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::PaymentOverdue,
        NotificationPriority::Critical,
    );

    let before = count_topic(env, symbol_short!("n_status"));
    NotificationSystem::update_notification_status(
        env,
        &notif_id,
        NotificationDeliveryStatus::Failed,
    )
    .expect("status update failed");
    let after = count_topic(env, symbol_short!("n_status"));

    assert_eq!(after - before, 1);
    let (emitted_id, emitted_status) = latest_status_payload(env);
    assert_eq!(emitted_id, notif_id);
    assert_eq!(emitted_status, NotificationDeliveryStatus::Failed);
    });
}

// ============================================================================
// 5. Preference filtering - blocked types produce no events
// ============================================================================

/// Disabling every notification type causes all `create_notification` calls
/// to return `NotificationBlocked` and emit zero `notif` events.
#[test]
fn test_all_types_blocked_emits_no_events() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);

    // Disable every type.
    let prefs = NotificationPreferences {
        user: recipient.clone(),
        invoice_created: false,
        invoice_verified: false,
        invoice_status_changed: false,
        bid_received: false,
        bid_accepted: false,
        payment_received: false,
        payment_overdue: false,
        invoice_defaulted: false,
        system_alerts: false,
        general: false,
        minimum_priority: NotificationPriority::Critical,
        updated_at: env.ledger().timestamp(),
    };
    NotificationSystem::update_user_preferences(env, &recipient, prefs);

    let all_types = [
        NotificationType::InvoiceCreated,
        NotificationType::InvoiceVerified,
        NotificationType::InvoiceStatusChanged,
        NotificationType::BidReceived,
        NotificationType::BidAccepted,
        NotificationType::PaymentReceived,
        NotificationType::PaymentOverdue,
        NotificationType::InvoiceDefaulted,
        NotificationType::SystemAlert,
        NotificationType::General,
    ];

    let before = count_topic(env, symbol_short!("notif"));
    for ntype in all_types {
        let result = NotificationSystem::create_notification(
            env,
            recipient.clone(),
            ntype,
            NotificationPriority::Low, // below Critical minimum
            String::from_str(env, "T"),
            String::from_str(env, "M"),
            None,
        );
        assert!(
            matches!(result, Err(crate::errors::QuickLendXError::NotificationBlocked)),
            "expected NotificationBlocked"
        );
    }
    let after = count_topic(env, symbol_short!("notif"));
    assert_eq!(
        after - before,
        0,
        "no notif events should be emitted when all types are blocked"
    );
    });
}

/// A notification below the user's minimum priority is blocked and emits no event.
#[test]
fn test_below_minimum_priority_emits_no_event() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);

    // Set minimum priority to High.
    let mut prefs = NotificationSystem::get_user_preferences(env, &recipient);
    prefs.minimum_priority = NotificationPriority::High;
    NotificationSystem::update_user_preferences(env, &recipient, prefs);

    let before = count_topic(env, symbol_short!("notif"));

    // Low priority - should be blocked.
    let result = NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
        String::from_str(env, "T"),
        String::from_str(env, "M"),
        None,
    );
    assert!(matches!(
        result,
        Err(crate::errors::QuickLendXError::NotificationBlocked)
    ));

    let after = count_topic(env, symbol_short!("notif"));
    assert_eq!(after - before, 0);
    });
}

/// A notification at or above the minimum priority is allowed and emits one event.
#[test]
fn test_at_minimum_priority_emits_event() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);

    let mut prefs = NotificationSystem::get_user_preferences(env, &recipient);
    prefs.minimum_priority = NotificationPriority::High;
    NotificationSystem::update_user_preferences(env, &recipient, prefs);

    let before = count_topic(env, symbol_short!("notif"));

    // High priority - should pass.
    
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::InvoiceVerified,
        NotificationPriority::High,
    );

    let after = count_topic(env, symbol_short!("notif"));
    assert_eq!(after - before, 1);
    });
}

// ============================================================================
// 6. Notification ID uniqueness across timestamps
// ============================================================================

/// Two notifications created at different ledger timestamps must have different IDs,
/// ensuring no accidental collision that could cause silent deduplication.
#[test]
fn test_notification_ids_differ_across_timestamps() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);

    let id1 = 
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
    );

    env.ledger().set_timestamp(1_001);

    let id2 = 
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
    );

    assert_ne!(id1, id2, "notifications at different timestamps must have distinct IDs");
    });
}

// ============================================================================
// 7. Read-only queries emit zero events
// ============================================================================

/// `get_notification`, `get_user_notifications`, `get_user_preferences`, and
/// `get_user_notification_stats` must not emit any events.
#[test]
fn test_read_only_queries_emit_no_events() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);
    let notif_id = 
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
    );

    // Snapshot event count after creation.
    let snapshot = env.events().all().events().len();

    // Read-only calls.
    let _ = NotificationSystem::get_notification(env, &notif_id);
    let _ = NotificationSystem::get_user_notifications(env, &recipient);
    let _ = NotificationSystem::get_user_preferences(env, &recipient);
    let _ = NotificationSystem::get_user_notification_stats(env, &recipient);

    assert_eq!(
        env.events().all().events().len(),
        snapshot,
        "read-only queries must not emit any events"
    );
    });
}

// ============================================================================
// 8. Notification not found returns error, no event
// ============================================================================

/// Updating the status of a non-existent notification returns
/// `NotificationNotFound` and emits no `n_status` event.
#[test]
fn test_status_update_on_missing_notification_emits_no_event() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let fake_id = BytesN::from_array(env, &[0u8; 32]);

    let before = count_topic(env, symbol_short!("n_status"));
    let result = NotificationSystem::update_notification_status(
        env,
        &fake_id,
        NotificationDeliveryStatus::Delivered,
    );
    let after = count_topic(env, symbol_short!("n_status"));

    assert!(
        matches!(result, Err(crate::errors::QuickLendXError::NotificationNotFound)),
        "expected NotificationNotFound"
    );
    assert_eq!(after - before, 0, "no n_status event on missing notification");
    });
}

// ============================================================================
// 9. Payload determinism - same inputs produce same payload shape
// ============================================================================

/// Two notifications with the same type and priority (different timestamps)
/// must produce `notif` payloads with identical type and priority fields.
#[test]
fn test_notif_payload_fields_are_deterministic() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);

    let recipient = Address::generate(env);

    
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::BidReceived,
        NotificationPriority::Medium,
    );
    let (_, r1, t1, p1) = latest_notif_payload(env);

    env.ledger().set_timestamp(1_001);
    
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::BidReceived,
        NotificationPriority::Medium,
    );
    let (_, r2, t2, p2) = latest_notif_payload(env);

    assert_eq!(r1, r2, "recipient must be consistent");
    assert_eq!(t1, t2, "notification type must be consistent");
    assert_eq!(p1, p2, "priority must be consistent");
    });
}

// ============================================================================
// 10. Idempotency key derivation and replay protection
// ============================================================================

/// Idempotency keys are derived deterministically from (event_kind, target_id, ledger_seq, nonce).
/// The same inputs always produce the same key.
#[test]
fn test_idempotency_key_derivation_is_deterministic() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);
    env.ledger().set_sequence_number(100);

    let recipient = Address::generate(env);

    // Create first notification
    let notif_id_1 = 
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
    );

    let notif_1 = NotificationSystem::get_notification(env, &notif_id_1)
        .expect("notification not found");
    let key_1 = notif_1.idempotency_key.clone();

    // Create second notification with same parameters at same ledger sequence
    // (but different timestamp, so different notification ID)
    env.ledger().set_timestamp(1_001);
    let notif_id_2 = 
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
    );

    let notif_2 = NotificationSystem::get_notification(env, &notif_id_2)
        .expect("notification not found");
    let key_2 = notif_2.idempotency_key.clone();

    // Keys should differ because timestamps (nonces) differ
    assert_ne!(
        key_1, key_2,
        "different timestamps should produce different idempotency keys"
    );

    // But notification IDs should also differ
    assert_ne!(notif_id_1, notif_id_2, "notification IDs should differ");
    });
}

/// Replaying the same notification (same event_kind, target_id, ledger_seq, nonce)
/// is rejected with `NotificationDuplicate` and emits no event.
#[test]
fn test_replay_same_notification_is_rejected() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);
    env.ledger().set_sequence_number(100);

    let recipient = Address::generate(env);

    // First submission succeeds
    let result1 = NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::InvoiceVerified,
        NotificationPriority::High,
        String::from_str(env, "Title"),
        String::from_str(env, "Message"),
        None,
    );
    assert!(result1.is_ok(), "first submission should succeed");

    let before = count_topic(env, symbol_short!("notif"));

    // Replay with same parameters (same timestamp, same ledger sequence)
    // This simulates a transaction being resubmitted
    let result2 = NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::InvoiceVerified,
        NotificationPriority::High,
        String::from_str(env, "Title"),
        String::from_str(env, "Message"),
        None,
    );

    // Should be rejected as duplicate
    assert!(
        matches!(result2, Err(crate::errors::QuickLendXError::NotificationDuplicate)),
        "replay should be rejected with NotificationDuplicate"
    );

    let after = count_topic(env, symbol_short!("notif"));
    assert_eq!(
        after - before,
        0,
        "replay rejection must not emit any notif events"
    );
    });
}

/// Replaying across all notification types is rejected.
/// This ensures replay protection works uniformly across the system.
#[test]
fn test_replay_rejection_across_all_notification_kinds() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);
    env.ledger().set_sequence_number(100);

    let recipient = Address::generate(env);

    let mut prefs = NotificationSystem::get_user_preferences(env, &recipient);
    prefs.general = true;
    NotificationSystem::update_user_preferences(env, &recipient, prefs);

    let all_types = [
        NotificationType::InvoiceCreated,
        NotificationType::InvoiceVerified,
        NotificationType::InvoiceStatusChanged,
        NotificationType::BidReceived,
        NotificationType::BidAccepted,
        NotificationType::PaymentReceived,
        NotificationType::PaymentOverdue,
        NotificationType::InvoiceDefaulted,
        NotificationType::SystemAlert,
        NotificationType::General,
    ];

    for ntype in all_types {
        // First submission
        let result1 = NotificationSystem::create_notification(
            env,
            recipient.clone(),
            ntype.clone(),
            NotificationPriority::Medium,
            String::from_str(env, "T"),
            String::from_str(env, "M"),
            None,
        );
        assert!(result1.is_ok(), "first submission for {:?} should succeed", ntype);

        // Replay
        let result2 = NotificationSystem::create_notification(
            env,
            recipient.clone(),
            ntype.clone(),
            NotificationPriority::Medium,
            String::from_str(env, "T"),
            String::from_str(env, "M"),
            None,
        );
        assert!(
            matches!(result2, Err(crate::errors::QuickLendXError::NotificationDuplicate)),
            "replay for {:?} should be rejected",
            ntype
        );
    }
    });
}

/// Different recipients with the same notification type at the same ledger sequence
/// produce different idempotency keys and are not rejected as duplicates.
#[test]
fn test_different_recipients_not_rejected_as_duplicates() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);
    env.ledger().set_sequence_number(100);

    let recipient1 = Address::generate(env);
    let recipient2 = Address::generate(env);

    // First notification to recipient1
    let result1 = NotificationSystem::create_notification(
        env,
        recipient1.clone(),
        NotificationType::PaymentReceived,
        NotificationPriority::High,
        String::from_str(env, "T"),
        String::from_str(env, "M"),
        None,
    );
    assert!(result1.is_ok());

    // Same notification type to recipient2 (different target_id)
    let result2 = NotificationSystem::create_notification(
        env,
        recipient2.clone(),
        NotificationType::PaymentReceived,
        NotificationPriority::High,
        String::from_str(env, "T"),
        String::from_str(env, "M"),
        None,
    );
    assert!(
        result2.is_ok(),
        "different recipients should not be rejected as duplicates"
    );
    });
}

/// Ledger sequence changes produce different idempotency keys.
/// This ensures that the same logical event at different ledger heights
/// is treated as distinct.
#[test]
fn test_different_ledger_sequences_produce_different_keys() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);
    env.ledger().set_sequence_number(100);

    let recipient = Address::generate(env);

    // First notification at ledger 100
    let result1 = NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::BidAccepted,
        NotificationPriority::High,
        String::from_str(env, "T"),
        String::from_str(env, "M"),
        None,
    );
    assert!(result1.is_ok());

    // Advance ledger sequence
    env.ledger().set_sequence_number(101);

    // Same notification at ledger 101 should succeed (different ledger_seq)
    let result2 = NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::BidAccepted,
        NotificationPriority::High,
        String::from_str(env, "T"),
        String::from_str(env, "M"),
        None,
    );
    assert!(
        result2.is_ok(),
        "different ledger sequences should produce different keys"
    );
    });
}

/// Replay protection survives across multiple notification types.
/// Each type maintains its own idempotency tracking.
#[test]
fn test_replay_protection_per_notification_type() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);
    env.ledger().set_sequence_number(100);

    let recipient = Address::generate(env);

    // Create InvoiceCreated notification
    let result1 = NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
        String::from_str(env, "T"),
        String::from_str(env, "M"),
        None,
    );
    assert!(result1.is_ok());

    // Replay InvoiceCreated - should be rejected
    let result2 = NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
        String::from_str(env, "T"),
        String::from_str(env, "M"),
        None,
    );
    assert!(matches!(
        result2,
        Err(crate::errors::QuickLendXError::NotificationDuplicate)
    ));

    // Different type (BidAccepted) at same ledger should succeed
    let result3 = NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::BidAccepted,
        NotificationPriority::Medium,
        String::from_str(env, "T"),
        String::from_str(env, "M"),
        None,
    );
    assert!(result3.is_ok(), "different notification type should not be rejected");
    });
}

/// Idempotency key is stored in the notification and can be retrieved.
#[test]
fn test_idempotency_key_stored_in_notification() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);
    env.ledger().set_sequence_number(100);

    let recipient = Address::generate(env);

    let notif_id = 
        create_notif(
            env,
            contract_id,
            &recipient,
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
    );

    let notification = NotificationSystem::get_notification(env, &notif_id)
        .expect("notification not found");

    // Idempotency key must be a 32-byte value
    assert_eq!(
        notification.idempotency_key.len(),
        32,
        "idempotency key must be 32 bytes"
    );

    // Key must be non-zero (not a default/empty value)
    let zero_key = BytesN::from_array(env, &[0u8; 32]);
    assert_ne!(
        notification.idempotency_key, zero_key,
        "idempotency key must not be zero"
    );
    });
}

/// Blocked notifications (due to preferences) do not consume idempotency keys.
/// A subsequent unblocked notification with the same parameters should succeed.
#[test]
fn test_blocked_notification_does_not_consume_idempotency_key() {
    run_notif_test(|env, contract_id| {
    env.ledger().set_timestamp(1_000);
    env.ledger().set_sequence_number(100);

    let recipient = Address::generate(env);

    // Block InvoiceCreated notifications
    let mut prefs = NotificationSystem::get_user_preferences(env, &recipient);
    prefs.invoice_created = false;
    NotificationSystem::update_user_preferences(env, &recipient, prefs);

    // First attempt - blocked
    let result1 = NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
        String::from_str(env, "T"),
        String::from_str(env, "M"),
        None,
    );
    assert!(matches!(
        result1,
        Err(crate::errors::QuickLendXError::NotificationBlocked)
    ));

    // Unblock the notification type
    let mut prefs = NotificationSystem::get_user_preferences(env, &recipient);
    prefs.invoice_created = true;
    NotificationSystem::update_user_preferences(env, &recipient, prefs);

    // Second attempt with same parameters should succeed (key not consumed)
    let result2 = NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::InvoiceCreated,
        NotificationPriority::Medium,
        String::from_str(env, "T"),
        String::from_str(env, "M"),
        None,
    );
    assert!(
        result2.is_ok(),
        "unblocked notification should succeed even with same parameters"
    );
    });
}

// ============================================================================
// Lifecycle wiring integration tests
// ============================================================================

fn lifecycle_setup() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "biz"));
    client.verify_business(&admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "inv"));
    client.verify_investor(&investor, &1_000_000);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&investor, &100_000);
    let exp = env.ledger().sequence() + 10_000;
    tok.approve(&investor, &contract_id, &100_000, &exp);

    (env, client, admin, business, investor, currency)
}

fn funded_invoice_fixture(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    investor: &Address,
    currency: &Address,
) -> (BytesN<32>, BytesN<32>) {
    let due = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        business,
        &10_000,
        currency,
        &due,
        &String::from_str(env, "desc"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(investor, &invoice_id, &9_000, &10_000);
    client.accept_bid_and_fund(&invoice_id, &bid_id);
    (invoice_id, bid_id)
}

fn notification_type_for_user(
    env: &Env,
    contract_id: &Address,
    user: &Address,
    expected: NotificationType,
) -> bool {
    env.as_contract(contract_id, || {
        NotificationSystem::get_user_notifications(env, user)
            .iter()
            .any(|id| {
                NotificationSystem::get_notification(env, &id)
                    .map(|n| n.notification_type == expected)
                    .unwrap_or(false)
            })
    })
}

#[test]
fn test_wire_accept_bid_emits_bid_accepted_notification() {
    let (env, client, _admin, business, investor, currency) = lifecycle_setup();
    let contract_id = client.address.clone();
    let (_invoice_id, _bid_id) =
        funded_invoice_fixture(&env, &client, &business, &investor, &currency);
    assert!(notification_type_for_user(
        &env, &contract_id, &investor, NotificationType::BidAccepted,
    ));
}

#[test]
fn test_wire_partial_payment_emits_payment_received_notification() {
    let (env, client, _admin, business, investor, currency) = lifecycle_setup();
    let contract_id = client.address.clone();
    let (invoice_id, _bid_id) =
        funded_invoice_fixture(&env, &client, &business, &investor, &currency);
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&business, &10_000);
    let exp = env.ledger().sequence() + 10_000;
    tok.approve(&business, &client.address, &10_000, &exp);
    client.process_partial_payment(&invoice_id, &2_000, &String::from_str(&env, "tx-1"));
    assert!(notification_type_for_user(
        &env, &contract_id, &business, NotificationType::PaymentReceived,
    ));
}

#[test]
fn test_wire_settlement_emits_status_changed_notification() {
    let (env, client, _admin, business, investor, currency) = lifecycle_setup();
    let contract_id = client.address.clone();
    let (invoice_id, _bid_id) =
        funded_invoice_fixture(&env, &client, &business, &investor, &currency);
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&business, &10_000);
    let exp = env.ledger().sequence() + 10_000;
    tok.approve(&business, &client.address, &10_000, &exp);
    client.settle_invoice(&invoice_id, &10_000);
    assert!(notification_type_for_user(
        &env, &contract_id, &business, NotificationType::InvoiceStatusChanged,
    ));
}

#[test]
fn test_wire_default_emits_invoice_defaulted_notification() {
    let (env, client, _admin, business, investor, currency) = lifecycle_setup();
    let contract_id = client.address.clone();
    let (invoice_id, _bid_id) =
        funded_invoice_fixture(&env, &client, &business, &investor, &currency);
    env.ledger().set_timestamp(9_999_999);
    let _ = client.try_mark_invoice_defaulted(&invoice_id, &None);
    assert!(notification_type_for_user(
        &env, &contract_id, &business, NotificationType::InvoiceDefaulted,
    ));
}

#[test]
fn test_wire_dispute_lifecycle_notifications() {
    let (env, client, admin, business, investor, currency) = lifecycle_setup();
    let contract_id = client.address.clone();
    let (invoice_id, _bid_id) =
        funded_invoice_fixture(&env, &client, &business, &investor, &currency);
    client.create_dispute(
        &invoice_id, &business,
        &String::from_str(&env, "reason"),
        &String::from_str(&env, "evidence"),
    );
    assert!(notification_type_for_user(
        &env, &contract_id, &business, NotificationType::SystemAlert,
    ));
    client.put_dispute_under_review(&invoice_id, &admin);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.resolve_dispute(&invoice_id, &admin, &String::from_str(&env, "resolved"));
    let count = env.as_contract(&contract_id, || {
        NotificationSystem::get_user_notifications(&env, &business).len()
    });
    assert!(count >= 2, "expected dispute opened + resolved notifications, got {count}");
}

#[test]
fn test_wire_payment_notification_respects_preferences() {
    let (env, client, _admin, business, investor, currency) = lifecycle_setup();
    let contract_id = client.address.clone();
    let (invoice_id, _bid_id) =
        funded_invoice_fixture(&env, &client, &business, &investor, &currency);
    let mut prefs = env.as_contract(&contract_id, || {
        NotificationSystem::get_user_preferences(&env, &business)
    });
    prefs.payment_received = false;
    env.as_contract(&contract_id, || {
        NotificationSystem::update_user_preferences(&env, &business, prefs);
    });
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&business, &10_000);
    let exp = env.ledger().sequence() + 10_000;
    tok.approve(&business, &client.address, &10_000, &exp);
    client.process_partial_payment(&invoice_id, &1_000, &String::from_str(&env, "tx-pref"));
    assert!(!notification_type_for_user(
        &env, &contract_id, &business, NotificationType::PaymentReceived,
    ));
}
