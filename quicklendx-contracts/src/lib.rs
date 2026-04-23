#![no_std]

pub mod admin;
pub mod analytics;
pub mod audit;
pub mod backup;
pub mod bid;
pub mod currency;
pub mod defaults;
pub mod dispute;
pub mod emergency;
pub mod errors;
pub mod escrow;
pub mod events;
pub mod fees;
pub mod init;
pub mod investment;
pub mod investment_queries;
pub mod notifications;
pub mod pause;
pub mod payments;
pub mod profits;
pub mod protocol_limits;
pub mod reentrancy;
pub mod settlement;
pub mod storage;
pub mod types;
pub mod verification;
pub mod vesting;

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_admin;
#[cfg(test)]
mod test_admin_simple;
#[cfg(test)]
mod test_admin_standalone;
#[cfg(test)]
mod test_analytics_consistency;
#[cfg(test)]
mod test_audit;
#[cfg(test)]
mod test_backup;
#[cfg(test)]
mod test_bid_exclusivity;
#[cfg(test)]
mod test_bid_ranking;
#[cfg(test)]
mod test_bid_ttl;
#[cfg(test)]
mod test_bid_validation;
#[cfg(test)]
mod test_business_kyc;
#[cfg(test)]
mod test_cancel_refund;
#[cfg(test)]
mod test_currency;
#[cfg(test)]
mod test_default;
#[cfg(test)]
mod test_dispute;
#[cfg(test)]
mod test_due_date_boundaries;
#[cfg(test)]
mod test_emergency_withdraw;
#[cfg(test)]
mod test_errors;
#[cfg(test)]
mod test_escrow;
#[cfg(test)]
mod test_escrow_consistency;
#[cfg(test)]
mod test_escrow_refund;
#[cfg(test)]
mod test_escrow_refund_hardened;
#[cfg(test)]
mod test_events;
#[cfg(test)]
mod test_fee_analytics_boundaries;
#[cfg(test)]
mod test_fees;
#[cfg(test)]
mod test_fees_extended;
#[cfg(test)]
mod test_fuzz;
#[cfg(test)]
mod test_init;
#[cfg(test)]
mod test_init_debug;
#[cfg(test)]
mod test_insurance;
#[cfg(test)]
mod test_invariants;
#[cfg(test)]
mod test_investment_consistency;
#[cfg(test)]
mod test_investment_lifecycle;
#[cfg(test)]
mod test_investment_queries;
#[cfg(test)]
mod test_investment_terminal_states;
#[cfg(test)]
mod test_investor_kyc;
#[cfg(test)]
mod test_invoice;
#[cfg(test)]
mod test_invoice_id_collision;
#[cfg(test)]
mod test_invoice_metadata;
#[cfg(test)]
mod test_invoice_normalization;
#[cfg(test)]
mod test_ledger_timestamp_consistency;
#[cfg(test)]
mod test_lifecycle;
#[cfg(test)]
mod test_limit;
#[cfg(test)]
mod test_max_invoices_per_business;
#[cfg(test)]
mod test_min_invoice_amount;
#[cfg(test)]
mod test_overdue_expiration;
#[cfg(test)]
mod test_overflow;
#[cfg(test)]
mod test_partial_payments;
#[cfg(test)]
mod test_pause;
#[cfg(test)]
mod test_payment_history;
#[cfg(test)]
mod test_profit_fee;
#[cfg(test)]
mod test_profit_fee_formula;
#[cfg(test)]
mod test_protocol_limits;
#[cfg(test)]
mod test_queries;
#[cfg(test)]
mod test_reentrancy;
#[cfg(test)]
mod test_refund;
#[cfg(test)]
mod test_revenue_split;
#[cfg(test)]
mod test_risk_tier;
#[cfg(test)]
mod test_settlement;
#[cfg(test)]
mod test_storage;
#[cfg(test)]
mod test_string_limits;
#[cfg(test)]
mod test_treasury_rotation;
#[cfg(test)]
mod test_types;
#[cfg(test)]
mod test_vesting;

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};
use crate::errors::QuickLendXError;
use crate::settlement::{Progress, SettlementPaymentRecord};

pub const MAX_QUERY_LIMIT: u32 = 100;

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    pub fn initialize(env: Env, params: crate::init::InitializationParams) -> Result<(), QuickLendXError> {
        crate::init::ProtocolInitializer::initialize(&env, &params)
    }

    pub fn is_initialized(env: Env) -> bool {
        crate::init::ProtocolInitializer::is_initialized(&env)
    }

    pub fn record_payment(
        env: Env,
        invoice_id: BytesN<32>,
        payer: Address,
        amount: i128,
        payment_nonce: String,
    ) -> Result<Progress, QuickLendXError> {
        crate::settlement::record_payment(&env, &invoice_id, &payer, amount, payment_nonce)
    }

    pub fn get_payment_records(
        env: Env,
        invoice_id: BytesN<32>,
        from: u32,
        limit: u32,
    ) -> Result<Vec<SettlementPaymentRecord>, QuickLendXError> {
        crate::settlement::get_payment_records(&env, &invoice_id, from, limit)
    }

    pub fn get_payment_count(env: Env, invoice_id: BytesN<32>) -> Result<u32, QuickLendXError> {
        crate::settlement::get_payment_count(&env, &invoice_id)
    }

    pub fn get_invoice_progress(env: Env, invoice_id: BytesN<32>) -> Result<Progress, QuickLendXError> {
        crate::settlement::get_invoice_progress(&env, &invoice_id)
    }

    // Proxy methods for other modules would go here...
    // For now, focusing on the task requirement.
}
