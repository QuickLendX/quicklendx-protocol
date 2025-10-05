use crate::errors::QuickLendXError;
use soroban_sdk::{contracttype, symbol_short, vec, Address, Env, Map, Symbol, Vec};

// Constants
const MAX_FEE_BPS: u32 = 1000;
const MIN_FEE_BPS: u32 = 0;
const BPS_DENOMINATOR: i128 = 10_000;

// Storage keys
const FEE_CONFIG_KEY: Symbol = symbol_short!("fee_cfg");
const REVENUE_KEY: Symbol = symbol_short!("revenue");
const VOLUME_KEY: Symbol = symbol_short!("volume");

/// Fee types supported by the platform
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FeeType {
    Platform,
    Processing,
    Verification,
    EarlyPayment,
    LatePayment,
}

/// Volume tier for discounted fees
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VolumeTier {
    Standard,
    Silver,
    Gold,
    Platinum,
}

/// Fee structure configuration
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeeStructure {
    pub fee_type: FeeType,
    pub base_fee_bps: u32,
    pub min_fee: i128,
    pub max_fee: i128,
    pub is_active: bool,
    pub updated_at: u64,
    pub updated_by: Address,
}

/// User volume data
#[contracttype]
#[derive(Clone, Debug)]
pub struct UserVolumeData {
    pub user: Address,
    pub total_volume: i128,
    pub transaction_count: u32,
    pub current_tier: VolumeTier,
    pub last_updated: u64,
}

/// Revenue configuration
#[contracttype]
#[derive(Clone, Debug)]
pub struct RevenueConfig {
    pub treasury_address: Address,
    pub treasury_share_bps: u32,
    pub developer_share_bps: u32,
    pub platform_share_bps: u32,
    pub auto_distribution: bool,
    pub min_distribution_amount: i128,
}

/// Revenue tracking
#[contracttype]
#[derive(Clone, Debug)]
pub struct RevenueData {
    pub period: u64,
    pub total_collected: i128,
    pub fees_by_type: Map<FeeType, i128>,
    pub total_distributed: i128,
    pub pending_distribution: i128,
    pub transaction_count: u32,
}

/// Fee analytics
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeeAnalytics {
    pub period: u64,
    pub total_fees: i128,
    pub average_fee_rate: i128,
    pub total_transactions: u32,
    pub fee_efficiency_score: u32,
}

pub struct FeeManager;

impl FeeManager {
    pub fn initialize(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        admin.require_auth();
        let default_fees = vec![
            env,
            FeeStructure {
                fee_type: FeeType::Platform,
                base_fee_bps: 200,
                min_fee: 100,
                max_fee: 1_000_000,
                is_active: true,
                updated_at: env.ledger().timestamp(),
                updated_by: admin.clone(),
            },
            FeeStructure {
                fee_type: FeeType::Processing,
                base_fee_bps: 50,
                min_fee: 50,
                max_fee: 500_000,
                is_active: true,
                updated_at: env.ledger().timestamp(),
                updated_by: admin.clone(),
            },
            FeeStructure {
                fee_type: FeeType::Verification,
                base_fee_bps: 100,
                min_fee: 100,
                max_fee: 100_000,
                is_active: true,
                updated_at: env.ledger().timestamp(),
                updated_by: admin.clone(),
            },
        ];
        env.storage().instance().set(&FEE_CONFIG_KEY, &default_fees);
        Ok(())
    }

    pub fn get_fee_structure(env: &Env, fee_type: &FeeType) -> Result<FeeStructure, QuickLendXError> {
        let fee_structures: Vec<FeeStructure> = env
            .storage()
            .instance()
            .get(&FEE_CONFIG_KEY)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        for i in 0..fee_structures.len() {
            let structure = fee_structures.get(i).unwrap();
            if structure.fee_type == *fee_type {
                return Ok(structure);
            }
        }
        Err(QuickLendXError::StorageKeyNotFound)
    }

    pub fn update_fee_structure(
        env: &Env,
        admin: &Address,
        fee_type: FeeType,
        base_fee_bps: u32,
        min_fee: i128,
        max_fee: i128,
        is_active: bool,
    ) -> Result<FeeStructure, QuickLendXError> {
        admin.require_auth();
        if base_fee_bps > MAX_FEE_BPS {
            return Err(QuickLendXError::InvalidAmount);
        }
        if min_fee < 0 || max_fee < min_fee {
            return Err(QuickLendXError::InvalidAmount);
        }
        let mut fee_structures: Vec<FeeStructure> = env
            .storage()
            .instance()
            .get(&FEE_CONFIG_KEY)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        let mut found = false;
        let updated_structure = FeeStructure {
            fee_type: fee_type.clone(),
            base_fee_bps,
            min_fee,
            max_fee,
            is_active,
            updated_at: env.ledger().timestamp(),
            updated_by: admin.clone(),
        };
        for i in 0..fee_structures.len() {
            let structure = fee_structures.get(i).unwrap();
            if structure.fee_type == fee_type {
                fee_structures.set(i, updated_structure.clone());
                found = true;
                break;
            }
        }
        if !found {
            fee_structures.push_back(updated_structure.clone());
        }
        env.storage().instance().set(&FEE_CONFIG_KEY, &fee_structures);
        Ok(updated_structure)
    }

    pub fn calculate_total_fees(
        env: &Env,
        user: &Address,
        transaction_amount: i128,
        is_early_payment: bool,
        is_late_payment: bool,
    ) -> Result<i128, QuickLendXError> {
        if transaction_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }
        let fee_structures: Vec<FeeStructure> = env
            .storage()
            .instance()
            .get(&FEE_CONFIG_KEY)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        let user_volume_data = Self::get_user_volume(env, user);
        let tier_discount = Self::get_tier_discount(&user_volume_data.current_tier);
        let mut total_fees: i128 = 0;
        for i in 0..fee_structures.len() {
            let structure = fee_structures.get(i).unwrap();
            if !structure.is_active {
                continue;
            }
            if structure.fee_type == FeeType::EarlyPayment && !is_early_payment {
                continue;
            }
            if structure.fee_type == FeeType::LatePayment && !is_late_payment {
                continue;
            }
            let mut fee = Self::calculate_base_fee(&structure, transaction_amount)?;
            if structure.fee_type != FeeType::LatePayment {
                fee = fee - (fee * tier_discount as i128 / BPS_DENOMINATOR);
            }
            if is_early_payment && structure.fee_type == FeeType::Platform {
                fee = fee - (fee * 1000 / BPS_DENOMINATOR);
            }
            if is_late_payment && structure.fee_type == FeeType::LatePayment {
                fee = fee + (fee * 2000 / BPS_DENOMINATOR);
            }
            total_fees += fee;
        }
        Ok(total_fees)
    }

    fn calculate_base_fee(
        structure: &FeeStructure,
        amount: i128,
    ) -> Result<i128, QuickLendXError> {
        let fee = amount * structure.base_fee_bps as i128 / BPS_DENOMINATOR;
        let fee = if fee < structure.min_fee {
            structure.min_fee
        } else if fee > structure.max_fee {
            structure.max_fee
        } else {
            fee
        };
        Ok(fee)
    }

    fn get_tier_discount(tier: &VolumeTier) -> u32 {
        match tier {
            VolumeTier::Standard => 0,
            VolumeTier::Silver => 500,
            VolumeTier::Gold => 1000,
            VolumeTier::Platinum => 1500,
        }
    }

    pub fn get_user_volume(env: &Env, user: &Address) -> UserVolumeData {
        let key = (VOLUME_KEY, user.clone());
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or(UserVolumeData {
                user: user.clone(),
                total_volume: 0,
                transaction_count: 0,
                current_tier: VolumeTier::Standard,
                last_updated: env.ledger().timestamp(),
            })
    }

    pub fn update_user_volume(
        env: &Env,
        user: &Address,
        transaction_amount: i128,
    ) -> Result<UserVolumeData, QuickLendXError> {
        let mut volume_data = Self::get_user_volume(env, user);
        volume_data.total_volume = volume_data.total_volume.saturating_add(transaction_amount);
        volume_data.transaction_count = volume_data.transaction_count.saturating_add(1);
        volume_data.last_updated = env.ledger().timestamp();
        volume_data.current_tier = if volume_data.total_volume >= 1_000_000_000_000 {
            VolumeTier::Platinum
        } else if volume_data.total_volume >= 500_000_000_000 {
            VolumeTier::Gold
        } else if volume_data.total_volume >= 100_000_000_000 {
            VolumeTier::Silver
        } else {
            VolumeTier::Standard
        };
        let key = (VOLUME_KEY, user.clone());
        env.storage().instance().set(&key, &volume_data);
        Ok(volume_data)
    }

    pub fn collect_fees(
        env: &Env,
        user: &Address,
        fees_collected: Map<FeeType, i128>,
        total_amount: i128,
    ) -> Result<(), QuickLendXError> {
        let period = Self::get_current_period(env);
        let key = (REVENUE_KEY, period);
        let mut revenue_data: RevenueData = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or(RevenueData {
                period,
                total_collected: 0,
                fees_by_type: Map::new(env),
                total_distributed: 0,
                pending_distribution: 0,
                transaction_count: 0,
            });
        revenue_data.total_collected = revenue_data.total_collected.saturating_add(total_amount);
        revenue_data.pending_distribution = revenue_data.pending_distribution.saturating_add(total_amount);
        revenue_data.transaction_count = revenue_data.transaction_count.saturating_add(1);
        // Copy fees by type into revenue data
        revenue_data.fees_by_type = fees_collected;
        env.storage().instance().set(&key, &revenue_data);
        Self::update_user_volume(env, user, total_amount)?;
        Ok(())
    }

    fn get_current_period(env: &Env) -> u64 {
        env.ledger().timestamp() / 2_592_000
    }

    pub fn configure_revenue_distribution(
        env: &Env,
        admin: &Address,
        config: RevenueConfig,
    ) -> Result<(), QuickLendXError> {
        admin.require_auth();
        let total_shares = config.treasury_share_bps + config.developer_share_bps + config.platform_share_bps;
        if total_shares != 10_000 {
            return Err(QuickLendXError::InvalidAmount);
        }
        let key = symbol_short!("rev_cfg");
        env.storage().instance().set(&key, &config);
        Ok(())
    }

    pub fn distribute_revenue(
        env: &Env,
        admin: &Address,
        period: u64,
    ) -> Result<(i128, i128, i128), QuickLendXError> {
        admin.require_auth();
        let config: RevenueConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("rev_cfg"))
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        let revenue_key = (REVENUE_KEY, period);
        let mut revenue_data: RevenueData = env
            .storage()
            .instance()
            .get(&revenue_key)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        if revenue_data.pending_distribution < config.min_distribution_amount {
            return Err(QuickLendXError::InvalidAmount);
        }
        let amount = revenue_data.pending_distribution;
        let treasury_amount = amount * config.treasury_share_bps as i128 / BPS_DENOMINATOR;
        let developer_amount = amount * config.developer_share_bps as i128 / BPS_DENOMINATOR;
        let platform_amount = amount - treasury_amount - developer_amount;
        revenue_data.total_distributed = revenue_data.total_distributed.saturating_add(amount);
        revenue_data.pending_distribution = 0;
        env.storage().instance().set(&revenue_key, &revenue_data);
        Ok((treasury_amount, developer_amount, platform_amount))
    }

    pub fn get_analytics(env: &Env, period: u64) -> Result<FeeAnalytics, QuickLendXError> {
        let revenue_key = (REVENUE_KEY, period);
        let revenue_data: RevenueData = env
            .storage()
            .instance()
            .get(&revenue_key)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        let average_fee_rate = if revenue_data.transaction_count > 0 {
            revenue_data.total_collected / revenue_data.transaction_count as i128
        } else {
            0
        };
        let efficiency_score = if revenue_data.total_collected > 0 {
            let distributed_pct = revenue_data.total_distributed * 100 / revenue_data.total_collected;
            distributed_pct.min(100) as u32
        } else {
            0
        };
        Ok(FeeAnalytics {
            period,
            total_fees: revenue_data.total_collected,
            average_fee_rate,
            total_transactions: revenue_data.transaction_count,
            fee_efficiency_score: efficiency_score,
        })
    }

    pub fn validate_fee_params(
        base_fee_bps: u32,
        min_fee: i128,
        max_fee: i128,
    ) -> Result<(), QuickLendXError> {
        if base_fee_bps < MIN_FEE_BPS || base_fee_bps > MAX_FEE_BPS {
            return Err(QuickLendXError::InvalidAmount);
        }
        if min_fee < 0 || max_fee < 0 || max_fee < min_fee {
            return Err(QuickLendXError::InvalidAmount);
        }
        Ok(())
    }
}
