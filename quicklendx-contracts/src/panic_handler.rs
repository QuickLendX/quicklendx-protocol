//! Panic-handler harness for no-std Soroban contract tests.
//!
//! In the `wasm32v1-none` (no-std) release build the Soroban SDK's built-in
//! `#[panic_handler]` (installed by the `#[contract]` macro) calls `abort`
//! after the panic, so events cannot be emitted on that path.
//!
//! This module provides two things:
//!
//! 1. [`PanicCaught`] — a `#[contractevent]` that off-chain indexers can
//!    subscribe to.  Its schema compiles cleanly under `#![no_std]` because
//!    every field uses `soroban_sdk` primitives.
//!
//! 2. [`catch_panic`] — a **test-only** wrapper (gated by `#[cfg(test)]`)
//!    that runs a closure under `std::panic::catch_unwind`, catches any panic,
//!    emits a [`PanicCaught`] event into the supplied [`Env`], and returns an
//!    `Err` containing the panic message.
//!
//! # No-std discipline
//! Only the `catch_panic` helper uses `std`.  Everything else in this module
//! is `alloc`-only and WASM-safe.  The `#[cfg(test)] extern crate std;`
//! directive below is elided from all non-test builds.

// Bring std into scope for test builds so catch_panic can call
// std::panic::catch_unwind.  This line is stripped from WASM/no-std release
// builds by the #[cfg(test)] gate.
#[cfg(test)]
extern crate std;

use soroban_sdk::{contractevent, Env, String as SorobanString};

/// Topic constant for the `PanicCaught` contract event.
pub const TOPIC_PANIC_CAUGHT: &str = "panic_caught";

/// Emitted by the test harness whenever a panic is caught.
///
/// This event is produced only in test builds via [`catch_panic`].  On the
/// `wasm32v1-none` target, panics terminate the contract via `abort` and this
/// event is never reachable.
///
/// # Fields
/// - `message` – The string from `panic!("…")`, or `"unknown panic"` if the
///   payload type is not a `&str` or `String`.
/// - `timestamp` – `env.ledger().timestamp()` at the moment of emission.
#[contractevent]
pub struct PanicCaught {
    pub message: SorobanString,
    pub timestamp: u64,
}

/// Emit a [`PanicCaught`] event into `env`.
///
/// Prefer using [`catch_panic`] in tests so the emit is tied to an actual
/// caught panic.  Call this directly only when you need to simulate the event
/// without triggering a real panic (e.g. to verify subscriber behaviour).
///
/// Must be called inside a contract call context (e.g. via
/// `env.as_contract(&contract_id, || { emit_panic_caught(env, msg); })`).
pub fn emit_panic_caught(env: &Env, message: SorobanString) {
    PanicCaught {
        message,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Test-only harness: execute `f`, catch any panic, and emit [`PanicCaught`].
///
/// # Behaviour
/// - Returns `Ok(T)` when `f` completes without panicking; no event is emitted.
/// - Returns `Err(message)` when `f` panics; a [`PanicCaught`] event is emitted
///   into `env` before returning.
///
/// # Contract context
/// `emit_panic_caught` calls `env.events().publish()`, which requires that the
/// Soroban `Env` is executing inside a contract call.  Wrap the call site in
/// `env.as_contract(&contract_id, || catch_panic(&env, f))` to satisfy this
/// requirement in integration tests.
///
/// # Unwind safety
/// The closure is wrapped in [`std::panic::AssertUnwindSafe`] so that `f` does
/// not need to be `UnwindSafe`.  This is appropriate in test code where the
/// goal is to observe that a panic occurred rather than to resume from it.
///
/// This function is gated by `#[cfg(test)]` and is never compiled into a WASM
/// release build.
#[cfg(test)]
pub fn catch_panic<T, F: FnOnce() -> T>(env: &Env, f: F) -> Result<T, std::string::String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(val) => Ok(val),
        Err(payload) => {
            let msg: std::string::String = if let Some(s) = payload.downcast_ref::<&str>() {
                std::string::String::from(*s)
            } else if let Some(s) = payload.downcast_ref::<std::string::String>() {
                s.clone()
            } else {
                std::string::String::from("unknown panic")
            };
            let sdk_msg = SorobanString::from_str(env, &msg);
            emit_panic_caught(env, sdk_msg);
            Err(msg)
        }
    }
}
