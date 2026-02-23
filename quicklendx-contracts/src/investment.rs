use crate::errors::QuickLendXError;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol, Vec};

/// Premium rate applied to the covered amount expressed in basis points (1/10,000).
pub const DEFAULT_INSURANCE_PREMIUM_BPS: i128 = 200; // 2% of the covered amount.

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceCoverage {
    pub provider: Address,
    pub coverage_amount: i128,
    pub premium_amount: i128,
    pub coverage_percentage: u32,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvestmentStatus {
    Active,
    Withdrawn,
    Completed,
    Defaulted,
    Refunded,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Investment {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
    pub funded_at: u64,
    pub status: InvestmentStatus,
    pub insurance: Vec<InsuranceCoverage>,
}

impl Investment {
    pub fn calculate_premium(amount: i128, coverage_percentage: u32) -> i128 {
        if amount <= 0 || coverage_percentage == 0 {
            return 0;
        }

        let coverage_amount = amount
            .saturating_mul(coverage_percentage as i128)
            .checked_div(100)
            .unwrap_or(0);

        let premium = coverage_amount
            .saturating_mul(DEFAULT_INSURANCE_PREMIUM_BPS)
            .checked_div(10_000)
            .unwrap_or(0);

        if premium == 0 && coverage_amount > 0 {
            1
        } else {
            premium
        }
    }

    pub fn add_insurance(
        &mut self,
        provider: Address,
        coverage_percentage: u32,
        premium: i128,
    ) -> Result<i128, QuickLendXError> {
        if coverage_percentage == 0 || coverage_percentage > 100 {
            return Err(QuickLendXError::InvalidCoveragePercentage);
        }

        if premium <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        for coverage in self.insurance.iter() {
            if coverage.active {
                return Err(QuickLendXError::OperationNotAllowed);
            }
        }

        let coverage_amount = self
            .amount
            .saturating_mul(coverage_percentage as i128)
            .checked_div(100)
            .unwrap_or(0);

        self.insurance.push_back(InsuranceCoverage {
            provider,
            coverage_amount,
            premium_amount: premium,
            coverage_percentage,
            active: true,
        });

        Ok(coverage_amount)
    }

    pub fn has_active_insurance(&self) -> bool {
        for coverage in self.insurance.iter() {
            if coverage.active {
                return true;
            }
        }
        false
    }

    pub fn process_insurance_claim(&mut self) -> Option<(Address, i128)> {
        let len = self.insurance.len();
        for idx in 0..len {
            if let Some(mut coverage) = self.insurance.get(idx) {
                if coverage.active {
                    coverage.active = false;
                    let provider = coverage.provider.clone();
                    let amount = coverage.coverage_amount;
                    self.insurance.set(idx, coverage);
                    return Some((provider, amount));
                }
            }
        }
        None
    }
}

pub struct InvestmentStorage;

impl InvestmentStorage {
    fn invoice_index_key(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (symbol_short!("inv_map"), invoice_id.clone())
    }

    /// Generate a unique investment ID using timestamp and counter
    pub fn generate_unique_investment_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let counter_key = symbol_short!("invst_cnt");
        let counter = env.storage().instance().get(&counter_key).unwrap_or(0u64);
        let next_counter = counter.saturating_add(1);
        env.storage().instance().set(&counter_key, &next_counter);

        let mut id_bytes = [0u8; 32];
        // Add investment prefix to distinguish from other entity types
        id_bytes[0] = 0x1A; // 'I' for Investment
        id_bytes[1] = 0x4E; // 'N' for iNvestment
                            // Embed timestamp in next 8 bytes
        id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        // Embed counter in next 8 bytes
        id_bytes[10..18].copy_from_slice(&next_counter.to_be_bytes());
        // Fill remaining bytes with a pattern to ensure uniqueness (overflow-safe)
        let mix = timestamp.saturating_add(next_counter).saturating_add(0x1A4E);
        for i in 18..32 {
            id_bytes[i] = (mix % 256) as u8;
        }

        BytesN::from_array(env, &id_bytes)
    }

    pub fn store_investment(env: &Env, investment: &Investment) {
        env.storage()
            .instance()
            .set(&investment.investment_id, investment);

        env.storage().instance().set(
            &Self::invoice_index_key(&investment.invoice_id),
            &investment.investment_id,
        );

        // Add to investor index
        Self::add_to_investor_index(env, &investment.investor, &investment.investment_id);
    }
    pub fn get_investment(env: &Env, investment_id: &BytesN<32>) -> Option<Investment> {
        env.storage().instance().get(investment_id)
    }
    pub fn get_investment_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Investment> {
        let index_key = Self::invoice_index_key(invoice_id);
        let investment_id: Option<BytesN<32>> = env.storage().instance().get(&index_key);
        investment_id.and_then(|id| Self::get_investment(env, &id))
    }
    pub fn update_investment(env: &Env, investment: &Investment) {
        env.storage()
            .instance()
            .set(&investment.investment_id, investment);

        env.storage().instance().set(
            &Self::invoice_index_key(&investment.invoice_id),
            &investment.investment_id,
        );
    }

    fn investor_index_key(investor: &Address) -> (Symbol, Address) {
        (symbol_short!("invst_inv"), investor.clone())
    }

    /// Get all investments for an investor
    pub fn get_investments_by_investor(env: &Env, investor: &Address) -> Vec<BytesN<32>> {
        let key = Self::investor_index_key(investor);
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Add investment to investor index
    pub fn add_to_investor_index(env: &Env, investor: &Address, investment_id: &BytesN<32>) {
        let key = Self::investor_index_key(investor);
        let mut investments = Self::get_investments_by_investor(env, investor);
        // Check if already exists
        let mut exists = false;
        for inv_id in investments.iter() {
            if inv_id == *investment_id {
                exists = true;
                break;
            }
        }
        if !exists {
            investments.push_back(investment_id.clone());
            env.storage().instance().set(&key, &investments);
        }
    }
}
