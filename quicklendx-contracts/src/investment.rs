use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol, Vec};
use crate::types::{Investment, InvestmentStatus};
use crate::errors::QuickLendXError;

impl InvestmentStatus {
    pub fn validate_transition(previous: &Self, next: &Self) -> Result<(), QuickLendXError> {
        match (previous, next) {
            (InvestmentStatus::Active, InvestmentStatus::Withdrawn) => Ok(()),
            (InvestmentStatus::Active, InvestmentStatus::Defaulted) => Ok(()),
            _ if previous == next => Ok(()),
            _ => Err(QuickLendXError::InvalidStatus),
        }
    }
}

pub struct InvestmentStorage;

impl InvestmentStorage {
    pub fn generate_unique_investment_id(env: &Env) -> BytesN<32> {
        let mut id_bytes = [0u8; 32];
        let timestamp = env.ledger().timestamp();
        let sequence = env.ledger().sequence();
        id_bytes[0..8].copy_from_slice(&timestamp.to_be_bytes());
        id_bytes[8..12].copy_from_slice(&sequence.to_be_bytes());
        BytesN::from_array(env, &id_bytes)
    }

    pub fn store_investment(env: &Env, investment: &Investment) {
        env.storage().instance().set(&investment.investment_id, investment);
        let key = (symbol_short!("inv_inv"), investment.investor.clone());
        let mut investor_investments: Vec<BytesN<32>> = env.storage().instance().get(&key).unwrap_or_else(|| Vec::new(env));
        investor_investments.push_back(investment.investment_id.clone());
        env.storage().instance().set(&key, &investor_investments);
        
        let inv_key = (symbol_short!("inv_id"), investment.invoice_id.clone());
        env.storage().instance().set(&inv_key, &investment.investment_id);
    }

    pub fn get_investment(env: &Env, id: &BytesN<32>) -> Option<Investment> {
        env.storage().instance().get(id)
    }

    pub fn update_investment(env: &Env, investment: &Investment) {
        env.storage().instance().set(&investment.investment_id, investment);
    }

    pub fn get_investment_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Investment> {
        let key = (symbol_short!("inv_id"), invoice_id.clone());
        let id: BytesN<32> = env.storage().instance().get(&key)?;
        Self::get_investment(env, &id)
    }

    pub fn get_investments_by_investor(env: &Env, investor: &Address) -> Vec<BytesN<32>> {
        let key = (symbol_short!("inv_inv"), investor.clone());
        env.storage().instance().get(&key).unwrap_or_else(|| Vec::new(env))
    }
}