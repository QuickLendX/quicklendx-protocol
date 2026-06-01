/// # Pause-State Event Emission Tests
///
/// Covers `require_unpaused` for every guarded entrypoint.
///
/// ## What is verified
///
/// 1. **Emission on blocked path** — each `EP_*` entrypoint emits exactly one
///    `PauseBlockedEvent` with correct `entrypoint`, `caller`, and `ledger_ts`
///    when `paused == true`.
/// 2. **No emission on live path** — no event is emitted when `paused == false`.
/// 3. **Return value** — blocked path returns `Err(ContractPaused)`; live path
///    returns `Ok(())`.
/// 4. **Topic stability** — the `entrypoint` field in each emission equals the
///    corresponding `EP_*` constant, verifying that the stable topic strings
///    are wired correctly.
/// 5. **Field propagation** — `caller` and `ledger_ts` are passed through
///    unmodified.
/// 6. **Multiple consecutive blocks** — each blocked invocation appends a
///    distinct event; the count matches the number of calls.
/// 7. **State independence** — `require_unpaused` is pure with respect to
///    `PauseState`; the state struct is not mutated.
/// 8. **ALL_ENTRYPOINTS coverage** — the `ALL_ENTRYPOINTS` slice covers every
///    `EP_*` constant used in the tests.
use crate::events::{PauseBlockedEvent, TOPIC_PAUSE_BLOCKED, VecEmitter};
use crate::pause::{
    PauseError, PauseState, ALL_ENTRYPOINTS, EP_BID_PLACEMENT, EP_ESCROW_RELEASE,
    EP_INVESTMENT_ACTION, EP_INVOICE_UPLOAD, EP_SETTLEMENT_INITIATION,
};

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn paused() -> PauseState {
    PauseState::active()
}

fn live() -> PauseState {
    PauseState::inactive()
}

// ─────────────────────────────────────────────────────────────────────────────
// Live-path: no emission, Ok returned
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_unpaused_no_event_invoice_upload() {
    let mut sink = VecEmitter::default();
    let result = live().require_unpaused(EP_INVOICE_UPLOAD, 1, 1000, &mut sink);
    assert_eq!(result, Ok(()));
    assert!(sink.events().is_empty(), "No event on live path");
}

#[test]
fn test_unpaused_no_event_bid_placement() {
    let mut sink = VecEmitter::default();
    let result = live().require_unpaused(EP_BID_PLACEMENT, 2, 2000, &mut sink);
    assert_eq!(result, Ok(()));
    assert!(sink.events().is_empty());
}

#[test]
fn test_unpaused_no_event_settlement_initiation() {
    let mut sink = VecEmitter::default();
    let result = live().require_unpaused(EP_SETTLEMENT_INITIATION, 3, 3000, &mut sink);
    assert_eq!(result, Ok(()));
    assert!(sink.events().is_empty());
}

#[test]
fn test_unpaused_no_event_escrow_release() {
    let mut sink = VecEmitter::default();
    let result = live().require_unpaused(EP_ESCROW_RELEASE, 4, 4000, &mut sink);
    assert_eq!(result, Ok(()));
    assert!(sink.events().is_empty());
}

#[test]
fn test_unpaused_no_event_investment_action() {
    let mut sink = VecEmitter::default();
    let result = live().require_unpaused(EP_INVESTMENT_ACTION, 5, 5000, &mut sink);
    assert_eq!(result, Ok(()));
    assert!(sink.events().is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// Blocked path: event emitted, Err returned — one test per entrypoint
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_paused_emits_and_blocks_invoice_upload() {
    let mut sink = VecEmitter::default();
    let result = paused().require_unpaused(EP_INVOICE_UPLOAD, 10, 100, &mut sink);

    assert_eq!(result, Err(PauseError::ContractPaused));
    assert_eq!(sink.events().len(), 1);
    assert_eq!(
        sink.events()[0],
        PauseBlockedEvent {
            entrypoint: EP_INVOICE_UPLOAD,
            caller: 10,
            ledger_ts: 100,
        }
    );
}

#[test]
fn test_paused_emits_and_blocks_bid_placement() {
    let mut sink = VecEmitter::default();
    let result = paused().require_unpaused(EP_BID_PLACEMENT, 20, 200, &mut sink);

    assert_eq!(result, Err(PauseError::ContractPaused));
    assert_eq!(sink.events().len(), 1);
    assert_eq!(
        sink.events()[0],
        PauseBlockedEvent {
            entrypoint: EP_BID_PLACEMENT,
            caller: 20,
            ledger_ts: 200,
        }
    );
}

#[test]
fn test_paused_emits_and_blocks_settlement_initiation() {
    let mut sink = VecEmitter::default();
    let result = paused().require_unpaused(EP_SETTLEMENT_INITIATION, 30, 300, &mut sink);

    assert_eq!(result, Err(PauseError::ContractPaused));
    assert_eq!(sink.events().len(), 1);
    assert_eq!(
        sink.events()[0],
        PauseBlockedEvent {
            entrypoint: EP_SETTLEMENT_INITIATION,
            caller: 30,
            ledger_ts: 300,
        }
    );
}

#[test]
fn test_paused_emits_and_blocks_escrow_release() {
    let mut sink = VecEmitter::default();
    let result = paused().require_unpaused(EP_ESCROW_RELEASE, 40, 400, &mut sink);

    assert_eq!(result, Err(PauseError::ContractPaused));
    assert_eq!(sink.events().len(), 1);
    assert_eq!(
        sink.events()[0],
        PauseBlockedEvent {
            entrypoint: EP_ESCROW_RELEASE,
            caller: 40,
            ledger_ts: 400,
        }
    );
}

#[test]
fn test_paused_emits_and_blocks_investment_action() {
    let mut sink = VecEmitter::default();
    let result = paused().require_unpaused(EP_INVESTMENT_ACTION, 50, 500, &mut sink);

    assert_eq!(result, Err(PauseError::ContractPaused));
    assert_eq!(sink.events().len(), 1);
    assert_eq!(
        sink.events()[0],
        PauseBlockedEvent {
            entrypoint: EP_INVESTMENT_ACTION,
            caller: 50,
            ledger_ts: 500,
        }
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Field propagation — caller and ledger_ts pass through unmodified
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_event_carries_exact_caller_and_ledger_ts() {
    let caller = u64::MAX - 1;
    let ledger_ts = u64::MAX;
    let mut sink = VecEmitter::default();

    paused()
        .require_unpaused(EP_INVOICE_UPLOAD, caller, ledger_ts, &mut sink)
        .unwrap_err();

    let ev = &sink.events()[0];
    assert_eq!(ev.caller, caller);
    assert_eq!(ev.ledger_ts, ledger_ts);
}

#[test]
fn test_event_caller_zero_is_valid() {
    let mut sink = VecEmitter::default();
    paused()
        .require_unpaused(EP_BID_PLACEMENT, 0, 0, &mut sink)
        .unwrap_err();
    assert_eq!(sink.events()[0].caller, 0);
    assert_eq!(sink.events()[0].ledger_ts, 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Topic stability — entrypoint field equals the EP_* constant
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_event_topic_stability_invoice_upload() {
    let mut sink = VecEmitter::default();
    paused()
        .require_unpaused(EP_INVOICE_UPLOAD, 0, 0, &mut sink)
        .unwrap_err();
    assert_eq!(sink.events()[0].entrypoint, EP_INVOICE_UPLOAD);
    assert_eq!(sink.events()[0].entrypoint, "invoice_upload");
}

#[test]
fn test_event_topic_stability_bid_placement() {
    let mut sink = VecEmitter::default();
    paused()
        .require_unpaused(EP_BID_PLACEMENT, 0, 0, &mut sink)
        .unwrap_err();
    assert_eq!(sink.events()[0].entrypoint, "bid_placement");
}

#[test]
fn test_event_topic_stability_settlement_initiation() {
    let mut sink = VecEmitter::default();
    paused()
        .require_unpaused(EP_SETTLEMENT_INITIATION, 0, 0, &mut sink)
        .unwrap_err();
    assert_eq!(sink.events()[0].entrypoint, "settlement_initiation");
}

#[test]
fn test_event_topic_stability_escrow_release() {
    let mut sink = VecEmitter::default();
    paused()
        .require_unpaused(EP_ESCROW_RELEASE, 0, 0, &mut sink)
        .unwrap_err();
    assert_eq!(sink.events()[0].entrypoint, "escrow_release");
}

#[test]
fn test_event_topic_stability_investment_action() {
    let mut sink = VecEmitter::default();
    paused()
        .require_unpaused(EP_INVESTMENT_ACTION, 0, 0, &mut sink)
        .unwrap_err();
    assert_eq!(sink.events()[0].entrypoint, "investment_action");
}

// ─────────────────────────────────────────────────────────────────────────────
// TOPIC_PAUSE_BLOCKED constant stability
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_topic_pause_blocked_constant_value() {
    assert_eq!(TOPIC_PAUSE_BLOCKED, "PauseBlocked");
}

// ─────────────────────────────────────────────────────────────────────────────
// Multiple consecutive blocked calls — one event per call
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_multiple_blocked_calls_accumulate_events() {
    let state = paused();
    let mut sink = VecEmitter::default();

    state
        .require_unpaused(EP_INVOICE_UPLOAD, 1, 100, &mut sink)
        .unwrap_err();
    state
        .require_unpaused(EP_BID_PLACEMENT, 2, 200, &mut sink)
        .unwrap_err();
    state
        .require_unpaused(EP_SETTLEMENT_INITIATION, 3, 300, &mut sink)
        .unwrap_err();

    assert_eq!(sink.events().len(), 3);
    assert_eq!(sink.events()[0].entrypoint, EP_INVOICE_UPLOAD);
    assert_eq!(sink.events()[1].entrypoint, EP_BID_PLACEMENT);
    assert_eq!(sink.events()[2].entrypoint, EP_SETTLEMENT_INITIATION);
}

#[test]
fn test_all_entrypoints_blocked_emit_five_events() {
    let state = paused();
    let mut sink = VecEmitter::default();

    for (i, &ep) in ALL_ENTRYPOINTS.iter().enumerate() {
        state
            .require_unpaused(ep, i as u64, i as u64 * 100, &mut sink)
            .unwrap_err();
    }

    assert_eq!(
        sink.events().len(),
        ALL_ENTRYPOINTS.len(),
        "One event per entrypoint"
    );
    for (i, &ep) in ALL_ENTRYPOINTS.iter().enumerate() {
        assert_eq!(sink.events()[i].entrypoint, ep);
        assert_eq!(sink.events()[i].caller, i as u64);
        assert_eq!(sink.events()[i].ledger_ts, i as u64 * 100);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mixed paused/unpaused — only blocked calls emit events
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_mixed_live_and_blocked_calls() {
    let mut sink = VecEmitter::default();

    // live call — no event
    live()
        .require_unpaused(EP_INVOICE_UPLOAD, 1, 100, &mut sink)
        .unwrap();
    assert!(sink.events().is_empty());

    // paused call — one event
    paused()
        .require_unpaused(EP_BID_PLACEMENT, 2, 200, &mut sink)
        .unwrap_err();
    assert_eq!(sink.events().len(), 1);
    assert_eq!(sink.events()[0].entrypoint, EP_BID_PLACEMENT);

    // another live call — still one event
    live()
        .require_unpaused(EP_SETTLEMENT_INITIATION, 3, 300, &mut sink)
        .unwrap();
    assert_eq!(sink.events().len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// State immutability — PauseState is not mutated by require_unpaused
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_require_unpaused_does_not_mutate_state() {
    let state = paused();
    let original = state;
    let mut sink = VecEmitter::default();

    state
        .require_unpaused(EP_INVOICE_UPLOAD, 1, 1, &mut sink)
        .unwrap_err();

    assert_eq!(state, original, "PauseState must not be mutated");
}

// ─────────────────────────────────────────────────────────────────────────────
// ALL_ENTRYPOINTS completeness check
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_all_entrypoints_slice_contains_every_ep_constant() {
    let expected = [
        EP_INVOICE_UPLOAD,
        EP_BID_PLACEMENT,
        EP_SETTLEMENT_INITIATION,
        EP_ESCROW_RELEASE,
        EP_INVESTMENT_ACTION,
    ];
    assert_eq!(ALL_ENTRYPOINTS.len(), expected.len());
    for ep in &expected {
        assert!(
            ALL_ENTRYPOINTS.contains(ep),
            "EP_* constant '{ep}' missing from ALL_ENTRYPOINTS"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// VecEmitter::clear utility
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_vec_emitter_clear_resets_collected_events() {
    let mut sink = VecEmitter::default();
    paused()
        .require_unpaused(EP_INVOICE_UPLOAD, 1, 1, &mut sink)
        .unwrap_err();
    assert_eq!(sink.events().len(), 1);
    sink.clear();
    assert!(sink.events().is_empty(), "clear must reset the collector");
}
