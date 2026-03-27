use crate::errors::QuickLendXError;
use soroban_sdk::{contracttype, Address, BytesN, Vec};

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
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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

// InvestmentStorage has been moved to crate::storage
