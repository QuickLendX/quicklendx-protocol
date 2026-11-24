#![allow(deprecated)]

use crate::types::Bid;
use soroban_sdk::{contractevent, Address, BytesN, Env};

/// Emitted when a bid is placed on an invoice.
///
/// Topic: [`TOPIC_BID_PLACED`] (`"bid_plc"`)
///
/// # Fields
/// - `bid_id` – Unique bid identifier.
/// - `invoice_id` – The invoice being bid on (auction_id in protocol terms).
/// - `investor` – Address of the bidder.
/// - `bid_amount` – Amount offered by the investor.
/// - `expected_return` – Total expected repayment amount.
/// - `timestamp` – Ledger timestamp when bid was placed.
/// - `expiration_timestamp` – Timestamp after which the bid expires.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct BidPlaced {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub timestamp: u64,
    pub expiration_timestamp: u64,
}

/// Emitted when a bid is accepted by the business owner.
///
/// Topic: [`TOPIC_BID_ACCEPTED`] (`"bid_acc"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct BidAccepted {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub business: Address,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub timestamp: u64,
}

/// Emitted when an investor withdraws their bid.
///
/// Topic: [`TOPIC_BID_WITHDRAWN`] (`"bid_wdr"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct BidWithdrawn {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub timestamp: u64,
}

/// Emitted when a bid is cancelled by its investor.
///
/// Topic: [`TOPIC_BID_CANCELLED`] (`"bid_cancelled"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct BidCancelled {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub timestamp: u64,
}

/// Emitted when a bid expires past its TTL.
///
/// Topic: [`TOPIC_BID_EXPIRED`] (`"bid_exp"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct BidExpired {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub expiration_timestamp: u64,
}

#[contractevent]
pub struct BidTtlUpdated {
    pub old_days: u64,
    pub new_days: u64,
    pub admin: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct BidExpiryGraceUpdated {
    pub old_seconds: u64,
    pub new_seconds: u64,
    pub admin: Address,
    pub timestamp: u64,
}

pub fn emit_bid_placed(env: &Env, bid: &Bid) {
    BidPlaced {
        bid_id: bid.bid_id.clone(),
        invoice_id: bid.invoice_id.clone(),
        investor: bid.investor.clone(),
        bid_amount: bid.bid_amount,
        expected_return: bid.expected_return,
        timestamp: bid.timestamp,
        expiration_timestamp: bid.expiration_timestamp,
    }
    .publish(env);
}

pub fn emit_bid_withdrawn(env: &Env, bid: &Bid) {
    BidWithdrawn {
        bid_id: bid.bid_id.clone(),
        invoice_id: bid.invoice_id.clone(),
        investor: bid.investor.clone(),
        bid_amount: bid.bid_amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bid_cancelled(env: &Env, bid: &Bid) {
    BidCancelled {
        bid_id: bid.bid_id.clone(),
        invoice_id: bid.invoice_id.clone(),
        investor: bid.investor.clone(),
        bid_amount: bid.bid_amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bid_accepted(env: &Env, bid: &Bid, invoice_id: &BytesN<32>, business: &Address) {
    BidAccepted {
        bid_id: bid.bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: bid.investor.clone(),
        business: business.clone(),
        bid_amount: bid.bid_amount,
        expected_return: bid.expected_return,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bid_expired(env: &Env, bid: &Bid) {
    BidExpired {
        bid_id: bid.bid_id.clone(),
        invoice_id: bid.invoice_id.clone(),
        investor: bid.investor.clone(),
        bid_amount: bid.bid_amount,
        expiration_timestamp: bid.expiration_timestamp,
    }
    .publish(env);
}

pub fn emit_bid_ttl_updated(env: &Env, old_days: u64, new_days: u64, admin: &Address) {
    BidTtlUpdated {
        old_days,
        new_days,
        admin: admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bid_expiry_grace_updated(
    env: &Env,
    old_seconds: u64,
    new_seconds: u64,
    admin: &Address,
) {
    BidExpiryGraceUpdated {
        old_seconds,
        new_seconds,
        admin: admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}
