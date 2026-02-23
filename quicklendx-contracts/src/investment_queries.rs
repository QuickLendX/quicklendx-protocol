use soroban_sdk::{Env, Address, Vec, Symbol};

/// Read-only investment query helpers
pub struct InvestmentQueries;

impl InvestmentQueries {
    /// Returns investment IDs indexed by investor address
    pub fn by_investor(env: &Env, investor: Address) -> Vec<u64> {
        env.storage()
            .get(&(Symbol::short("inv_by_investor"), investor))
            .unwrap_or(Vec::new(env))
    }

    /// Returns investment IDs for a specific invoice
    pub fn by_invoice(env: &Env, invoice_id: u64) -> Vec<u64> {
        env.storage()
            .get(&(Symbol::short("inv_by_invoice"), invoice_id))
            .unwrap_or(Vec::new(env))
    }

    /// Returns investment IDs filtered by status
    pub fn by_status(env: &Env, status: Symbol) -> Vec<u64> {
        env.storage()
            .get(&(Symbol::short("inv_by_status"), status))
            .unwrap_or(Vec::new(env))
    }
}
