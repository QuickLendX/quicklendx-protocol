# Pause-State Event Emission

> **Modules**: `src/pause.rs`, `src/events.rs`  
> **Test coverage**: `src/test_pause.rs`  
> **Event schema**: [docs/events_complete.md](events_complete.md)

---

## Purpose

When the protocol is paused, `require_unpaused` short-circuits every
protected entrypoint and returns an error.  Previously this rejection was
only visible to the calling transaction.  With this feature, every blocked
call also emits a structured `PauseBlocked` event so that off-chain
monitors and incident-response dashboards can:

- Count rejected transactions per entrypoint during an active pause.
- Attribute traffic to specific callers for post-incident analysis.
- Correlate blocked calls with the ledger timestamps at which they occurred.

---

## Hot-path guarantee

Event emission must not add overhead during normal (unpaused) operation.

`require_unpaused` is structured as an early-return:

```rust
pub fn require_unpaused<E: EventEmitter>(
    &self,
    entrypoint: &'static str,
    caller: u64,
    ledger_ts: u64,
    emitter: &mut E,
) -> Result<(), PauseError> {
    if !self.paused {
        return Ok(());   // ← returns here; emitter is never touched
    }
    emitter.emit_pause_blocked(PauseBlockedEvent { entrypoint, caller, ledger_ts });
    Err(PauseError::ContractPaused)
}
```

When `paused == false`, the function returns `Ok(())` before reaching the
emitter.  Production call sites pass a `NoopEmitter` — a zero-sized type
whose single method is `#[inline(always)]` with an empty body — so even the
unreachable branch compiles to nothing.

**Cost on unpaused path: one boolean comparison and a return.**

---

## Event schema

### Topic

```
PauseBlocked
```

Indexers must subscribe to this exact byte sequence.  The topic is exported
as the constant `events::TOPIC_PAUSE_BLOCKED`.

### Payload fields

| Field | Type | Description |
|---|---|---|
| `entrypoint` | `&'static str` | Stable symbol of the blocked entrypoint (see table below) |
| `caller` | `u64` | Numeric ID of the calling account |
| `ledger_ts` | `u64` | Ledger timestamp in seconds since the epoch |

### Entrypoint symbols

These strings are stable across upgrades.  Rename only with a coordinated
indexer migration and a semver bump.

| Constant | String value | Protected action |
|---|---|---|
| `EP_INVOICE_UPLOAD` | `"invoice_upload"` | Business uploads an invoice |
| `EP_BID_PLACEMENT` | `"bid_placement"` | Investor places a bid |
| `EP_SETTLEMENT_INITIATION` | `"settlement_initiation"` | Business initiates settlement |
| `EP_ESCROW_RELEASE` | `"escrow_release"` | Business triggers escrow release |
| `EP_INVESTMENT_ACTION` | `"investment_action"` | Investor takes an investment action |

All five symbols are also enumerated in `pause::ALL_ENTRYPOINTS` for use by
monitoring tools that want to assert complete entrypoint coverage.

---

## Usage pattern

### Production (zero overhead)

```rust
use quicklendx_contracts::events::NoopEmitter;
use quicklendx_contracts::pause::{PauseState, EP_INVOICE_UPLOAD};

fn upload_invoice(state: &PauseState, caller: u64, ledger_ts: u64) -> Result<(), ...> {
    state.require_unpaused(EP_INVOICE_UPLOAD, caller, ledger_ts, &mut NoopEmitter)?;
    // ... rest of entrypoint
}
```

### Custom emitter (e.g. Soroban on-chain events)

```rust
use quicklendx_contracts::events::{EventEmitter, PauseBlockedEvent};

struct SorobanEmitter<'a>(&'a soroban_sdk::Env);

impl EventEmitter for SorobanEmitter<'_> {
    fn emit_pause_blocked(&mut self, event: PauseBlockedEvent) {
        self.0.events().publish(
            (TOPIC_PAUSE_BLOCKED, event.entrypoint),
            (event.caller, event.ledger_ts),
        );
    }
}
```

### Tests

```rust
use quicklendx_contracts::events::VecEmitter;
use quicklendx_contracts::pause::{PauseState, EP_BID_PLACEMENT};

let mut sink = VecEmitter::default();
let _ = PauseState::active()
    .require_unpaused(EP_BID_PLACEMENT, 42, 99_000, &mut sink);

assert_eq!(sink.events().len(), 1);
assert_eq!(sink.events()[0].entrypoint, "bid_placement");
assert_eq!(sink.events()[0].caller, 42);
assert_eq!(sink.events()[0].ledger_ts, 99_000);
```

---

## Indexer compatibility notes

1. **Topic string is `"PauseBlocked"`** — exported as `TOPIC_PAUSE_BLOCKED`.
   Subscribe on this exact value.

2. **Entrypoint strings are ASCII, lowercase, underscore-separated** — they
   will never contain spaces, slashes, or Unicode.  Safe to use as map keys
   or metric labels without sanitisation.

3. **Field order is stable** — `entrypoint` before `caller` before
   `ledger_ts`.  If fields are added in future they will be appended.

4. **One event per blocked call** — each invocation of `require_unpaused`
   that finds `paused == true` emits exactly one event.  There is no
   deduplication or batching.

5. **Events are not emitted for unpaused calls** — an absence of
   `PauseBlocked` events during a window means either the protocol was live
   the entire time or no calls reached the guarded entrypoints.

---

## Test summary

Run with:

```bash
cargo test test_pause
```

| Test group | Count | What is verified |
|---|---|---|
| Live-path, no emission | 5 | Each EP — `Ok(())` returned, sink empty |
| Blocked-path per entrypoint | 5 | Each EP — `Err(ContractPaused)`, correct event fields |
| Field propagation | 2 | `caller` and `ledger_ts` pass through unmodified, including edge values |
| Topic stability | 6 | Each EP symbol equals its literal string; `TOPIC_PAUSE_BLOCKED == "PauseBlocked"` |
| Multiple blocked calls | 2 | Events accumulate; `ALL_ENTRYPOINTS` produces exactly 5 events |
| Mixed live/blocked | 1 | Only blocked calls produce events |
| State immutability | 1 | `PauseState` struct unchanged after `require_unpaused` |
| `ALL_ENTRYPOINTS` completeness | 1 | Slice contains every `EP_*` constant |
| `VecEmitter::clear` | 1 | Reset clears the collected-events buffer |
| **Total** | **24** | |
