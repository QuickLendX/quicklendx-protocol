#![cfg(test)]

use soroban_sdk::Env;

/// Invariant test scaffold for protocol state consistency.
/// Intentionally minimal and non-invasive.
#[test]
fn invariant_env_creation_is_safe() {
    let env = Env::default();
    let _ = env.ledger().timestamp();
}
