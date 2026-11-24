#![allow(deprecated)]

use soroban_sdk::{contractevent, symbol_short, Address, BytesN, Env, String};

/// Semantic alias: `DisputeOpened` == `DisputeCreated`.
/// Use `TOPIC_DISPUTE_CREATED` as the topic.
pub type DisputeOpened = DisputeCreated;

/// Emitted when a dispute is opened on an invoice.
///
/// Topic: [`TOPIC_DISPUTE_CREATED`] (`"dsp_cr"`)
///
/// # Fields
/// - `invoice_id` – The disputed invoice.
/// - `created_by` – Address of the dispute initiator (business or investor).
/// - `reason` – Human-readable reason string (no PII, max 1000 chars).
/// - `timestamp` – Ledger timestamp at emission time.
///
/// # Security
/// Only the business owner or investor on the invoice may open a dispute.
/// The `reason` field must not contain PII; it is a reason code or short description.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct DisputeCreated {
    pub invoice_id: BytesN<32>,
    pub created_by: Address,
    pub reason: String,
    pub timestamp: u64,
}

/// Emitted when a dispute is moved to admin review.
///
/// Topic: [`TOPIC_DISPUTE_UNDER_REVIEW`] (`"dsp_ur"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct DisputeUnderReview {
    pub invoice_id: BytesN<32>,
    pub reviewed_by: Address,
    pub timestamp: u64,
}

/// Emitted when a dispute is resolved by an admin.
///
/// Topic: [`TOPIC_DISPUTE_RESOLVED`] (`"dsp_rs"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct DisputeResolved {
    pub invoice_id: BytesN<32>,
    pub resolved_by: Address,
    pub resolution: String,
    pub timestamp: u64,
}

/// Emitted when a dispute is rejected (dismissed) by an admin.
///
/// Topic: [`TOPIC_DISPUTE_REJECTED`] (`"dsp_rj"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct DisputeRejected {
    pub invoice_id: BytesN<32>,
    pub rejected_by: Address,
    pub reason: String,
    pub timestamp: u64,
}

pub fn emit_dispute_created(
    env: &Env,
    invoice_id: &BytesN<32>,
    created_by: &Address,
    reason: &String,
) {
    DisputeCreated {
        invoice_id: invoice_id.clone(),
        created_by: created_by.clone(),
        reason: reason.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_dispute_under_review(env: &Env, invoice_id: &BytesN<32>, reviewed_by: &Address) {
    DisputeUnderReview {
        invoice_id: invoice_id.clone(),
        reviewed_by: reviewed_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_dispute_resolved(
    env: &Env,
    invoice_id: &BytesN<32>,
    resolved_by: &Address,
    resolution: &String,
) {
    DisputeResolved {
        invoice_id: invoice_id.clone(),
        resolved_by: resolved_by.clone(),
        resolution: resolution.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_dispute_rejected(
    env: &Env,
    invoice_id: &BytesN<32>,
    rejected_by: &Address,
    reason: &String,
) {
    DisputeRejected {
        invoice_id: invoice_id.clone(),
        rejected_by: rejected_by.clone(),
        reason: reason.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

// ── Arbiter (dispute-resolution) events ──────────────────────────────────────

/// Emitted when an admin registers a new dispute arbiter.
pub fn arbiter_registered(env: &Env, admin: &Address, arbiter: &Address) {
    env.events().publish(
        (symbol_short!("arb_reg"),),
        (admin.clone(), arbiter.clone(), env.ledger().timestamp()),
    );
}

/// Emitted when an admin revokes a previously registered dispute arbiter.
pub fn arbiter_revoked(env: &Env, admin: &Address, arbiter: &Address) {
    env.events().publish(
        (symbol_short!("arb_rvk"),),
        (admin.clone(), arbiter.clone(), env.ledger().timestamp()),
    );
}
