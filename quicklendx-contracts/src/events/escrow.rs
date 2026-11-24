#![allow(deprecated)]

use crate::payments::Escrow;
use soroban_sdk::{contractevent, Address, BytesN, Env};

/// Semantic alias: `FundsLocked` == `EscrowCreated`.
/// Emitted when investor funds are locked in escrow upon bid acceptance.
/// Use `TOPIC_ESCROW_CREATED` as the topic.
pub type FundsLocked = EscrowCreated;

/// Emitted when investor funds are locked in escrow (bid accepted).
///
/// Topic: [`TOPIC_ESCROW_CREATED`] (`"esc_cr"`)
///
/// # Fields
/// - `escrow_id` – Unique escrow identifier.
/// - `invoice_id` – The invoice being funded.
/// - `investor` – Address of the investor whose funds are locked.
/// - `business` – Address of the business receiving the funds.
/// - `amount` – Amount locked in escrow.
///
/// # Security
/// Funds are locked atomically with bid acceptance. No PII included.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct EscrowCreated {
    pub escrow_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub business: Address,
    pub amount: i128,
}

/// Emitted when escrow funds are released to the business.
///
/// Topic: [`TOPIC_ESCROW_RELEASED`] (`"esc_rel"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct EscrowReleased {
    pub escrow_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
}

/// Emitted when escrow funds are refunded to the investor.
///
/// Topic: [`TOPIC_ESCROW_REFUNDED`] (`"esc_ref"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct EscrowRefunded {
    pub escrow_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
}

/// Emitted when an investor withdraws their investment before settlement.
///
/// Topic: [`TOPIC_INVESTMENT_WITHDRAWN`] (`"inv_wd"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvestmentWithdrawn {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
}

pub fn emit_escrow_created(env: &Env, escrow: &Escrow) {
    EscrowCreated {
        escrow_id: escrow.escrow_id.clone(),
        invoice_id: escrow.invoice_id.clone(),
        investor: escrow.investor.clone(),
        business: escrow.business.clone(),
        amount: escrow.amount,
    }
    .publish(env);
}

pub fn emit_escrow_released(
    env: &Env,
    escrow_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    business: &Address,
    amount: i128,
) {
    EscrowReleased {
        escrow_id: escrow_id.clone(),
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        amount,
    }
    .publish(env);
}

pub fn emit_escrow_refunded(
    env: &Env,
    escrow_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    investor: &Address,
    amount: i128,
) {
    EscrowRefunded {
        escrow_id: escrow_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount,
    }
    .publish(env);
}

pub fn emit_investment_withdrawn(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    investor: &Address,
    amount: i128,
) {
    InvestmentWithdrawn {
        investment_id: investment_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount,
    }
    .publish(env);
}
