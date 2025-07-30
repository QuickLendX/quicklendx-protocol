
use soroban_sdk::{contracttype, Address, BytesN, Env};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvestmentStatus {
    Active,
    Withdrawn,
    Completed,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceCoverage {
    pub provider: Address,
    pub coverage_amount: i128,
    pub premium_amount: i128,
    pub coverage_percentage: u8, // 0-100
    pub active: bool,
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
    pub insurance: Option<InsuranceCoverage>,
}

pub struct InvestmentStorage;

impl InvestmentStorage {
    /// Generate a unique investment ID using timestamp and counter
    pub fn generate_unique_investment_id(env: &Env) -> BytesN<32> {
        use soroban_sdk::symbol_short;
        
        let timestamp = env.ledger().timestamp();
        let counter_key = symbol_short!("inv_cnt");
        let counter = env.storage().instance().get(&counter_key).unwrap_or(0u64);
        env.storage().instance().set(&counter_key, &(counter + 1));
        
        let mut id_bytes = [0u8; 32];
        // Add investment prefix to distinguish from other entity types
        id_bytes[0] = 0x1A; // 'I' for Investment
        id_bytes[1] = 0x4E; // 'N' for iNvestment
        // Embed timestamp in next 8 bytes
        id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        // Embed counter in next 8 bytes
        id_bytes[10..18].copy_from_slice(&counter.to_be_bytes());
        // Fill remaining bytes with a pattern to ensure uniqueness
        for i in 18..32 {
            id_bytes[i] = ((timestamp + counter as u64 + 0x1A4E) % 256) as u8;
        }
        
        BytesN::from_array(env, &id_bytes)
    }
    
    pub fn store_investment(env: &Env, investment: &Investment) {
        env.storage()
            .instance()
            .set(&investment.investment_id, investment);
    }
    pub fn get_investment(env: &Env, investment_id: &BytesN<32>) -> Option<Investment> {
        env.storage().instance().get(investment_id)
    }
    pub fn update_investment(env: &Env, investment: &Investment) {
        env.storage()
            .instance()
            .set(&investment.investment_id, investment);
    }

    /// Add insurance coverage to an investment
    pub fn add_insurance(
        env: &Env,
        investment_id: &BytesN<32>,
        provider: Address,
        coverage_percentage: u8,
        premium: i128,
    ) -> Result<(), crate::errors::QuickLendXError> {
        if coverage_percentage > 100 {
            return Err(crate::errors::QuickLendXError::InvalidCoveragePercentage);
        }
        let mut investment = Self::get_investment(env, investment_id)
            .ok_or(crate::errors::QuickLendXError::StorageKeyNotFound)?;
        let coverage_amount = investment.amount * coverage_percentage as i128 / 100;
        investment.insurance = Some(InsuranceCoverage {
            provider,
            coverage_amount,
            premium_amount: premium,
            coverage_percentage,
            active: true,
        });
        Self::update_investment(env, &investment);
        Ok(())
    }

    /// Process insurance claim for an investment
    pub fn process_insurance_claim(
        env: &Env,
        investment_id: &BytesN<32>,
    ) -> Result<i128, crate::errors::QuickLendXError> {
        let mut investment = Self::get_investment(env, investment_id)
            .ok_or(crate::errors::QuickLendXError::StorageKeyNotFound)?;
        if let Some(ref mut insurance) = investment.insurance {
            if !insurance.active {
                return Err(crate::errors::QuickLendXError::InsuranceNotActive);
            }
            insurance.active = false;
            Self::update_investment(env, &investment);
            Ok(insurance.coverage_amount)
        } else {
            Err(crate::errors::QuickLendXError::NoInsuranceCoverage)
        }
    }
}
}
