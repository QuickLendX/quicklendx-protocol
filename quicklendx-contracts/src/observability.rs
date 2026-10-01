//! Versioned observability metadata shared by protocol events and audit records.
//!
//! The envelope is intentionally small and stable: consumers can correlate an
//! event with its audit record using `operation_id`, while `phase` documents
//! that the record represents a committed transition rather than an attempted
//! or rejected call.

use soroban_sdk::{contracttype, symbol_short, BytesN, Env};

/// Current observability schema version.
pub const OBSERVABILITY_SCHEMA_VERSION: u32 = 1;

/// Stable lifecycle phase for a record that is safe to reconcile.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransitionPhase {
    Committed,
}

/// Correlation metadata shared by operational records.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Correlation {
    pub schema_version: u32,
    pub operation_id: BytesN<32>,
    pub phase: TransitionPhase,
    pub ledger_sequence: u32,
    pub ledger_timestamp: u64,
}

/// Allocate a unique operation id. This mutates only the observability counter.
pub fn allocate_operation_id(env: &Env) -> BytesN<32> {
    let key = symbol_short!("obs_cnt");
    let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
    let next = current.saturating_add(1);
    env.storage().instance().set(&key, &next);

    let mut bytes = [0u8; 32];
    bytes[0..4].copy_from_slice(&OBSERVABILITY_SCHEMA_VERSION.to_be_bytes());
    bytes[4..12].copy_from_slice(&env.ledger().timestamp().to_be_bytes());
    bytes[12..16].copy_from_slice(&env.ledger().sequence().to_be_bytes());
    bytes[16..24].copy_from_slice(&next.to_be_bytes());
    BytesN::from_array(env, &bytes)
}

pub fn committed_correlation(env: &Env, operation_id: BytesN<32>) -> Correlation {
    Correlation {
        schema_version: OBSERVABILITY_SCHEMA_VERSION,
        operation_id,
        phase: TransitionPhase::Committed,
        ledger_sequence: env.ledger().sequence(),
        ledger_timestamp: env.ledger().timestamp(),
    }
}
