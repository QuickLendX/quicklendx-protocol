//! Fee management module for the QuickLendX protocol.
//!
//! Handles platform fee configuration, revenue tracking, volume-tier discounts,
//! and treasury routing for all fee types supported by the protocol.
use crate::errors::QuickLendXError;
use crate::events;
use soroban_sdk::{contracttype, symbol_short, vec, Address, Env, Map, Symbol, Vec};

// Constants
const MAX_FEE_BPS: u32 = 1000; // 10% hard cap for all fees
#[allow(dead_code)]
const MIN_FEE_BPS: u32 = 0;
/// Basis-point denominator for percentage calculations (100% = 10,000 bps).
const BPS_DENOMINATOR: i128 = 10_000;
const DEFAULT_PLATFORM_FEE_BPS: u32 = 200; // 2%
const MAX_PLATFORM_FEE_BPS: u32 = 1000; // 10%
const ROTATION_TTL_SECONDS: u64 = 604_800; // 7 days
const EARLY_PLATFORM_DISCOUNT_BPS: i128 = 1_000; // 10%
const LATE_FEE_SURCHARGE_BPS: i128 = 2_000; // 20%

// Storage keys
const FEE_CONFIG_KEY: Symbol = symbol_short!("fee_cfg");
const REVENUE_KEY: Symbol = symbol_short!("revenue");
const VOLUME_KEY: Symbol = symbol_short!("volume");
#[allow(dead_code)]
const TREASURY_CONFIG_KEY: Symbol = symbol_short!("treasury");
const PLATFORM_FEE_KEY: Symbol = symbol_short!("plt_fee");
const ROTATION_KEY: Symbol = symbol_short!("rotate");
/// Guard key: set to `true` once `initialize` completes to prevent re-initialization.
const FEES_INIT_KEY: Symbol = symbol_short!("fee_init");

/// Fee types supported by the platform
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub enum FeeType {
    Platform,
    Processing,
    Verification,
    EarlyPayment,
    LatePayment,
}

/// Volume tier for discounted fees
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub enum VolumeTier {
    Standard,
    Silver,
    Gold,
    Platinum,
}

/// Fee structure configuration
#[contracttype]
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
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
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct UserVolumeData {
    pub user: Address,
    pub total_volume: i128,
    pub transaction_count: u32,
    pub current_tier: VolumeTier,
    pub last_updated: u64,
}

/// Treasury configuration for platform fees
#[contracttype]
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct TreasuryConfig {
    pub treasury_address: Address,
    pub is_active: bool,
    pub updated_at: u64,
    pub updated_by: Address,
}

/// Platform fee configuration  
#[contracttype]
#[derive(Clone, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct PlatformFeeConfig {
    pub fee_bps: u32,
    pub treasury_address: Option<Address>, // Simplified - just store address directly
    pub updated_at: u64,
    pub updated_by: Address,
}

/// Revenue configuration
#[contracttype]
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct RevenueConfig {
    pub treasury_address: Address,
    pub treasury_share_bps: u32,
    pub developer_share_bps: u32,
    pub platform_share_bps: u32,
    pub auto_distribution: bool,
    pub min_distribution_amount: i128,
}

/// Pending two-step treasury/fee-recipient rotation request.
///
/// Admin initiates the rotation; the new address must confirm by calling
/// `confirm_treasury_rotation`, proving ownership before the deadline.
/// This prevents accidental misrouting to addresses the team does not control.
#[contracttype]
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct RecipientRotationRequest {
    pub new_address: Address,
    pub initiated_by: Address,
    pub initiated_at: u64,
    pub confirmation_deadline: u64,
}

/// Revenue tracking
#[contracttype]
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
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
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
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
        // Explicit admin authorization: the caller must be the designated admin.
        admin.require_auth();

        // Guard: reject re-initialization to prevent overwriting live fee config.
        if env.storage().instance().has(&FEES_INIT_KEY) {
            return Err(QuickLendXError::InvalidFeeConfiguration);
        }

        // Initialize default fee structures
        let default_fees = vec![
            env,
            FeeStructure {
                fee_type: FeeType::Platform,
                base_fee_bps: DEFAULT_PLATFORM_FEE_BPS,
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

        // Initialize platform fee configuration
        let platform_fee_config = PlatformFeeConfig {
            fee_bps: DEFAULT_PLATFORM_FEE_BPS,
            treasury_address: None,
            updated_at: env.ledger().timestamp(),
            updated_by: admin.clone(),
        };
        env.storage()
            .instance()
            .set(&PLATFORM_FEE_KEY, &platform_fee_config);

        // Mark the fee system as initialized.
        env.storage().instance().set(&FEES_INIT_KEY, &true);

        Ok(())
    }

    /// Configure treasury for platform fee routing
    pub fn configure_treasury(
        env: &Env,
        admin: &Address,
        treasury_address: Address,
    ) -> Result<TreasuryConfig, QuickLendXError> {
        admin.require_auth();

        // Reject self-assignment: treasury must not be the contract itself.
        if treasury_address == env.current_contract_address() {
            return Err(QuickLendXError::InvalidAddress);
        }

        // Fetch existing config and reject duplicate treasury address.
        let mut platform_config = Self::get_platform_fee_config(env)?;
        if let Some(ref existing) = platform_config.treasury_address {
            if *existing == treasury_address {
                return Err(QuickLendXError::InvalidFeeConfiguration);
            }
        }

        let treasury_config = TreasuryConfig {
            treasury_address: treasury_address.clone(),
            is_active: true,
            updated_at: env.ledger().timestamp(),
            updated_by: admin.clone(),
        };

        platform_config.treasury_address = Some(treasury_address.clone());
        platform_config.updated_at = env.ledger().timestamp();
        platform_config.updated_by = admin.clone();

        env.storage()
            .instance()
            .set(&PLATFORM_FEE_KEY, &platform_config);

        events::emit_treasury_configured(env, &treasury_address, admin);

        Ok(treasury_config)
    }

    /// Update platform fee basis points
    pub fn update_platform_fee(
        env: &Env,
        admin: &Address,
        fee_bps: u32,
    ) -> Result<(), QuickLendXError> {
        // Auth is checked by the caller
        admin.require_auth();

        if fee_bps > MAX_PLATFORM_FEE_BPS {
            return Err(QuickLendXError::InvalidFeeBasisPoints);
        }

        let mut config = Self::get_platform_fee_config(env)?;

        if config.fee_bps == fee_bps {
            return Ok(());
        }

        let old_fee_bps = config.fee_bps;
        config.fee_bps = fee_bps;
        config.updated_at = env.ledger().timestamp();
        config.updated_by = admin.clone();

        env.storage().instance().set(&PLATFORM_FEE_KEY, &config);

        events::emit_platform_fee_config_updated(env, old_fee_bps, fee_bps, admin);

        Ok(())
    }

    /// Get platform fee configuration
    pub fn get_platform_fee_config(env: &Env) -> Result<PlatformFeeConfig, QuickLendXError> {
        env.storage()
            .instance()
            .get(&PLATFORM_FEE_KEY)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    /// Calculate platform fee for settlement
    pub fn calculate_platform_fee(
        env: &Env,
        investment_amount: i128,
        payment_amount: i128,
    ) -> Result<(i128, i128), QuickLendXError> {
        let config = Self::get_platform_fee_config(env)?;

        if payment_amount <= investment_amount {
            return Ok((payment_amount, 0));
        }

        let profit = payment_amount.saturating_sub(investment_amount);
        let platform_fee = profit.saturating_mul(config.fee_bps as i128) / BPS_DENOMINATOR;
        let investor_return = payment_amount.saturating_sub(platform_fee);

        Ok((investor_return, platform_fee))
    }

    /// Get treasury address if configured
    pub fn get_treasury_address(env: &Env) -> Option<Address> {
        if let Ok(config) = Self::get_platform_fee_config(env) {
            config.treasury_address
        } else {
            None
        }
    }

    pub fn get_fee_structure(
        env: &Env,
        fee_type: &FeeType,
    ) -> Result<FeeStructure, QuickLendXError> {
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

    /// Validate min/max fee consistency for a specific fee type.
    ///
    /// # Consistency Rules
    /// 1. **Range Validity**: `min_fee <= max_fee`
    /// 2. **Non-negative Values**: Both `min_fee` and `max_fee` must be >= 0
    /// 3. **Reasonable Bounds**: `max_fee` must not exceed 10x the base fee
    ///    (calculated as `base_fee_bps / 100` to account for BPS unit)
    /// 4. **Minimum Floor**: When base_fee_bps > 0, min_fee should be <= base fee max
    ///
    /// # Security Notes
    /// - Prevents fee structures where max_fee could bypass intended limits
    /// - Ensures min_fee doesn't force all transactions into floor pricing
    /// - Guards against misconfiguration where bounds are inversely related
    ///
    /// # Errors
    /// - `InvalidAmount` if min_fee > max_fee or either is negative
    /// - `InvalidFeeConfiguration` if bounds exceed reasonable thresholds
    pub fn validate_fee_structure_consistency(
        fee_type: &FeeType,
        base_fee_bps: u32,
        min_fee: i128,
        max_fee: i128,
    ) -> Result<(), QuickLendXError> {
        // Rule 1: Non-negative constraint
        if min_fee < 0 {
            return Err(QuickLendXError::InvalidAmount);
        }
        if max_fee < 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        // Rule 2: Range ordering constraint
        if max_fee < min_fee {
            return Err(QuickLendXError::InvalidAmount);
        }

        // Rule 3: Sanity check on max_fee (shouldn't exceed reasonable bounds)
        // For platform/processing fees, max_fee shouldn't be excessively large
        // Set a protocol-wide absolute maximum of 10M stroops
        const ABSOLUTE_MAX_FEE: i128 = 10_000_000_000_000; // 10M stroops

        if max_fee > ABSOLUTE_MAX_FEE {
            return Err(QuickLendXError::InvalidFeeConfiguration);
        }

        // Rule 4: Fee type-specific consistency checks
        match fee_type {
            FeeType::Platform | FeeType::Processing | FeeType::Verification => {
                // For these fee types, ensure max doesn't exceed a reasonable bound
                // based on the base rate. A max of 100x base seems reasonable.
                let calculated_max_threshold = (base_fee_bps as i128)
                    .saturating_mul(100)
                    .saturating_mul(100); // 100x times BPS value * 100
                if max_fee > calculated_max_threshold && calculated_max_threshold > 0 {
                    return Err(QuickLendXError::InvalidFeeConfiguration);
                }
            }
            FeeType::EarlyPayment | FeeType::LatePayment => {
                // Early/late payment fees may have different thresholds
                // Allow more flexibility but still bounded
                let calculated_max_threshold = (base_fee_bps as i128)
                    .saturating_mul(500)
                    .saturating_mul(100); // 500x for flexibility
                if max_fee > calculated_max_threshold && calculated_max_threshold > 0 {
                    return Err(QuickLendXError::InvalidFeeConfiguration);
                }
            }
        }

        Ok(())
    }

    /// Validate consistency across all fee structures in the system.
    ///
    /// # Cross-Type Consistency Rules
    /// 1. No two fee types can have overlapping responsibility zones
    /// 2. Total of all min_fees shouldn't exceed half the protocol's maximum
    /// 3. LatePayment fees must have higher or equal max bounds than standard fees
    ///
    /// # Errors
    /// - `InvalidFeeConfiguration` if cross-type consistency violations detected
    pub fn validate_cross_fee_consistency(
        env: &Env,
        fee_type: &FeeType,
        min_fee: i128,
        _max_fee: i128,
    ) -> Result<(), QuickLendXError> {
        let fee_structures: Vec<FeeStructure> = match env.storage().instance().get(&FEE_CONFIG_KEY)
        {
            Some(structures) => structures,
            None => return Ok(()), // No existing structures, skip cross-check
        };

        // Check rule 1: For LatePayment, ensure it doesn't undercut regular fees
        if *fee_type == FeeType::LatePayment {
            for i in 0..fee_structures.len() {
                let structure = fee_structures.get(i).unwrap();
                if structure.fee_type == FeeType::Platform && structure.min_fee > min_fee {
                    // LatePayment min shouldn't be less than Platform min
                    // (though LatePayment max should be higher)
                    if min_fee < structure.min_fee {
                        return Err(QuickLendXError::InvalidFeeConfiguration);
                    }
                }
            }
        }

        // Check rule 2: Validate total system fee exposure isn't unreasonable
        let mut total_active_min_fees: i128 = 0;
        for i in 0..fee_structures.len() {
            let structure = fee_structures.get(i).unwrap();
            if structure.is_active {
                total_active_min_fees = total_active_min_fees.saturating_add(structure.min_fee);
            }
        }

        // Add the current fee being configured
        total_active_min_fees = total_active_min_fees.saturating_add(min_fee);

        // Total min fees shouldn't exceed half the protocol max
        const PROTOCOL_MAX_SINGLE_TRANSACTION: i128 = 5_000_000_000_000; // 5M stroops
        const MAX_TOTAL_MIN_FEES: i128 = PROTOCOL_MAX_SINGLE_TRANSACTION / 2;

        if total_active_min_fees > MAX_TOTAL_MIN_FEES {
            return Err(QuickLendXError::InvalidFeeConfiguration);
        }

        Ok(())
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
            return Err(QuickLendXError::InvalidFeeBasisPoints);
        }

        // Apply comprehensive consistency checks
        Self::validate_fee_structure_consistency(&fee_type, base_fee_bps, min_fee, max_fee)?;
        Self::validate_cross_fee_consistency(env, &fee_type, min_fee, max_fee)?;
        let mut fee_structures: Vec<FeeStructure> = env
            .storage()
            .instance()
            .get(&FEE_CONFIG_KEY)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;
        let mut found = false;
        let mut old_bps = 0;
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
                old_bps = structure.base_fee_bps;
                fee_structures.set(i, updated_structure.clone());
                found = true;
                break;
            }
        }
        if !found {
            fee_structures.push_back(updated_structure.clone());
        }
        env.storage()
            .instance()
            .set(&FEE_CONFIG_KEY, &fee_structures);
        events::emit_fee_structure_updated(env, &fee_type, old_bps, base_fee_bps, admin);
        Ok(updated_structure)
    }

    /// Calculate deterministic transaction fees for a user and payment-timing context.
    ///
    /// Fee application order is intentionally fixed so the same inputs always produce
    /// the same output:
    /// 1. Compute each active fee's raw basis-point amount.
    /// 2. Clamp the raw fee into that structure's `[min_fee, max_fee]` range.
    /// 3. Apply the user's volume-tier discount to every fee except `LatePayment`.
    /// 4. Apply the early-payment discount to the `Platform` fee only.
    /// 5. Apply the late-payment surcharge to the `LatePayment` fee only.
    ///
    /// # Security notes
    /// - Uses saturating arithmetic to avoid overflow-based panics.
    /// - Iterates only over configured fee structures, keeping work deterministic.
    /// - Uses integer division throughout, so rounding always truncates toward zero.
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
                fee =
                    fee.saturating_sub(fee.saturating_mul(tier_discount as i128) / BPS_DENOMINATOR);
            }
            if is_early_payment && structure.fee_type == FeeType::Platform {
                fee = fee.saturating_sub(
                    fee.saturating_mul(EARLY_PLATFORM_DISCOUNT_BPS) / BPS_DENOMINATOR,
                );
            }
            if is_late_payment && structure.fee_type == FeeType::LatePayment {
                fee = fee
                    .saturating_add(fee.saturating_mul(LATE_FEE_SURCHARGE_BPS) / BPS_DENOMINATOR);
            }
            total_fees = total_fees.saturating_add(fee);
        }
        Ok(total_fees)
    }

    /// Calculate the raw fee for one structure and clamp it to the configured bounds.
    ///
    /// The clamp happens before tier discounts or timing modifiers so that the contract
    /// always applies discounts and penalties to a bounded intermediate value.
    fn calculate_base_fee(structure: &FeeStructure, amount: i128) -> Result<i128, QuickLendXError> {
        let fee = amount.saturating_mul(structure.base_fee_bps as i128) / BPS_DENOMINATOR;
        let fee = if fee < structure.min_fee {
            structure.min_fee
        } else if fee > structure.max_fee {
            structure.max_fee
        } else {
            fee
        };
        Ok(fee)
    }

    /// Return the fixed discount, in basis points, for a user's current volume tier.
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

    /// Update a user's cumulative transaction volume and derived discount tier.
    ///
    /// Tier thresholds are monotonic and based only on persisted cumulative volume,
    /// which keeps the derived tier deterministic for repeated inputs.
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

    /// Validate a fee collection map before persisting.
    fn validate_fee_collection_map(
        fees_collected: &Map<FeeType, i128>,
        total_amount: i128,
    ) -> Result<(), QuickLendXError> {
        let mut computed_total: i128 = 0;

        for fee_type in fees_collected.keys() {
            let amount = fees_collected.get(fee_type).unwrap_or(0);

            if amount < 0 {
                return Err(QuickLendXError::InvalidAmount);
            }

            computed_total = computed_total
                .checked_add(amount)
                .ok_or(QuickLendXError::InvalidFeeConfiguration)?;
        }

        if computed_total != total_amount {
            return Err(QuickLendXError::InvalidFeeConfiguration);
        }

        Ok(())
    }

    pub fn collect_fees(
        env: &Env,
        user: &Address,
        fees_collected: Map<FeeType, i128>,
        total_amount: i128,
    ) -> Result<(), QuickLendXError> {
        if total_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        // Validate the map: no negatives, sum == total_amount.
        // Missing fee types are acceptable (treated as zero).
        Self::validate_fee_collection_map(&fees_collected, total_amount)?;

        let period = Self::get_current_period(env);
        let key = (REVENUE_KEY, period);
        let mut revenue_data: RevenueData =
            env.storage().instance().get(&key).unwrap_or(RevenueData {
                period,
                total_collected: 0,
                fees_by_type: Map::new(env),
                total_distributed: 0,
                pending_distribution: 0,
                transaction_count: 0,
            });

        revenue_data.total_collected = revenue_data.total_collected.saturating_add(total_amount);
        revenue_data.pending_distribution = revenue_data
            .pending_distribution
            .saturating_add(total_amount);
        revenue_data.transaction_count = revenue_data.transaction_count.saturating_add(1);

        // Merge incoming fees into existing period map rather than overwriting.
        // This preserves fees collected in earlier calls within the same period.
        for fee_type in fees_collected.keys() {
            let amount = fees_collected.get(fee_type.clone()).unwrap_or(0);
            let existing: i128 = revenue_data.fees_by_type.get(fee_type.clone()).unwrap_or(0);
            revenue_data
                .fees_by_type
                .set(fee_type, existing.saturating_add(amount));
        }

        env.storage().instance().set(&key, &revenue_data);
        Self::update_user_volume(env, user, total_amount)?;
        Ok(())
    }

    fn get_current_period(env: &Env) -> u64 {
        env.ledger().timestamp() / 2_592_000
    }

    /// Configure revenue distribution with comprehensive share validation.
    ///
    /// # Safety invariants
    /// - Each individual share must be in [0, 10_000] bps.
    /// - The sum of all shares must equal exactly 10_000 bps (100%).
    /// - `min_distribution_amount` must be non-negative.
    ///
    /// # Errors
    /// - `InvalidFeeConfiguration` if any individual share exceeds 10_000 bps.
    /// - `InvalidAmount` if shares do not sum to 10_000 or min_distribution_amount < 0.
    pub fn configure_revenue_distribution(
        env: &Env,
        admin: &Address,
        config: RevenueConfig,
    ) -> Result<(), QuickLendXError> {
        admin.require_auth();

        // Validate individual share bounds
        Self::validate_revenue_shares(
            config.treasury_share_bps,
            config.developer_share_bps,
            config.platform_share_bps,
        )?;

        // Validate min distribution amount is non-negative
        if config.min_distribution_amount < 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let key = symbol_short!("rev_cfg");
        env.storage().instance().set(&key, &config);

        // Emit configuration event for audit trail
        env.events().publish(
            (symbol_short!("rev_cfg"),),
            (
                config.treasury_share_bps,
                config.developer_share_bps,
                config.platform_share_bps,
            ),
        );

        Ok(())
    }

    /// Validate that revenue shares are individually bounded and sum to 10_000 bps.
    ///
    /// # Invariants enforced
    /// - `0 <= each_share <= 10_000`
    /// - `treasury + developer + platform == 10_000`
    pub fn validate_revenue_shares(
        treasury_share_bps: u32,
        developer_share_bps: u32,
        platform_share_bps: u32,
    ) -> Result<(), QuickLendXError> {
        // Individual share bounds check
        if treasury_share_bps > 10_000
            || developer_share_bps > 10_000
            || platform_share_bps > 10_000
        {
            return Err(QuickLendXError::InvalidFeeConfiguration);
        }

        // Sum must equal exactly 10_000 bps (use checked arithmetic to prevent overflow)
        let total_shares = treasury_share_bps
            .checked_add(developer_share_bps)
            .and_then(|s| s.checked_add(platform_share_bps))
            .ok_or(QuickLendXError::InvalidFeeConfiguration)?;

        if total_shares != 10_000 {
            return Err(QuickLendXError::InvalidAmount);
        }

        Ok(())
    }

    /// Get current revenue split configuration
    pub fn get_revenue_split_config(env: &Env) -> Result<RevenueConfig, QuickLendXError> {
        let key = symbol_short!("rev_cfg");
        env.storage()
            .instance()
            .get(&key)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    /// Distribute accumulated revenue for a period according to the configured split.
    ///
    /// # Distribution algorithm
    /// 1. Treasury and developer amounts are calculated via `floor(pending * share_bps / 10_000)`.
    /// 2. Platform receives the remainder: `pending - treasury - developer`.
    /// 3. This guarantees `treasury + developer + platform == pending` (no dust loss).
    ///
    /// # Safety invariants enforced
    /// - Revenue config must exist and shares must sum to 10_000 bps.
    /// - If [`Self::get_treasury_address`] is set and `treasury_share_bps > 0`, the revenue
    ///   config’s `treasury_address` must match that routing target (same on-chain fee treasury).
    /// - Idempotency: when `pending_distribution == 0`, the call returns
    ///   [`QuickLendXError::OperationNotAllowed`] so a period cannot be “re-settled” until new
    ///   fees are collected (avoids duplicate events / no-op distributions when
    ///   `min_distribution_amount == 0`).
    /// - Pending distribution must meet the minimum threshold when it is positive.
    /// - Post-distribution sum must equal the original pending amount (accounting invariant).
    /// - Each distributed amount must be non-negative.
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

        // Re-validate shares at distribution time (defense in depth)
        Self::validate_revenue_shares(
            config.treasury_share_bps,
            config.developer_share_bps,
            config.platform_share_bps,
        )?;

        if config.treasury_share_bps > 0 {
            if let Some(fee_treasury) = Self::get_treasury_address(env) {
                if fee_treasury != config.treasury_address {
                    return Err(QuickLendXError::InvalidFeeConfiguration);
                }
            }
        }

        let revenue_key = (REVENUE_KEY, period);
        let mut revenue_data: RevenueData = env
            .storage()
            .instance()
            .get(&revenue_key)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        if revenue_data.pending_distribution == 0 {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        if revenue_data.pending_distribution < config.min_distribution_amount {
            return Err(QuickLendXError::InvalidAmount);
        }

        let amount = revenue_data.pending_distribution;

        // Calculate shares: treasury and developer via floor division, platform gets remainder
        let treasury_amount =
            amount.saturating_mul(config.treasury_share_bps as i128) / BPS_DENOMINATOR;
        let developer_amount =
            amount.saturating_mul(config.developer_share_bps as i128) / BPS_DENOMINATOR;
        let platform_amount = amount
            .saturating_sub(treasury_amount)
            .saturating_sub(developer_amount);

        // Safety: each amount must be non-negative
        if treasury_amount < 0 || developer_amount < 0 || platform_amount < 0 {
            return Err(QuickLendXError::InvalidFeeConfiguration);
        }

        // Accounting invariant: distributed amounts must exactly equal the pending amount
        let distributed_total = treasury_amount
            .checked_add(developer_amount)
            .and_then(|s| s.checked_add(platform_amount))
            .ok_or(QuickLendXError::InvalidFeeConfiguration)?;

        if distributed_total != amount {
            return Err(QuickLendXError::InvalidFeeConfiguration);
        }

        revenue_data.total_distributed = revenue_data.total_distributed.saturating_add(amount);
        revenue_data.pending_distribution = 0;
        env.storage().instance().set(&revenue_key, &revenue_data);

        // Emit distribution event for transparency and auditing
        env.events().publish(
            (symbol_short!("rev_dst"),),
            (period, treasury_amount, developer_amount, platform_amount),
        );

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
            revenue_data
                .total_collected
                .checked_div(revenue_data.transaction_count as i128)
                .unwrap_or(0)
        } else {
            0
        };
        let efficiency_score = if revenue_data.total_collected > 0 {
            let distributed_pct = revenue_data
                .total_distributed
                .saturating_mul(100)
                .checked_div(revenue_data.total_collected)
                .unwrap_or(0);
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
        if base_fee_bps > MAX_FEE_BPS {
            return Err(QuickLendXError::InvalidFeeBasisPoints);
        }
        if min_fee < 0 || max_fee < 0 || max_fee < min_fee {
            return Err(QuickLendXError::InvalidAmount);
        }
        Ok(())
    }

    /// Route platform fees to treasury if configured
    pub fn route_platform_fee(
        env: &Env,
        currency: &Address,
        from: &Address,
        fee_amount: i128,
    ) -> Result<Address, QuickLendXError> {
        if fee_amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        if let Some(treasury_address) = Self::get_treasury_address(env) {
            // Transfer to treasury
            crate::payments::transfer_funds(env, currency, from, &treasury_address, fee_amount)?;
            Ok(treasury_address)
        } else {
            // Default to contract address if no treasury configured
            let contract_address = env.current_contract_address();
            crate::payments::transfer_funds(env, currency, from, &contract_address, fee_amount)?;
            Ok(contract_address)
        }
    }

    /// Initiate a two-step treasury address rotation.
    ///
    /// Only the admin can call this. A `RecipientRotationRequest` is stored
    /// with a 7-day confirmation window. The new address must call
    /// `confirm_treasury_rotation` before the deadline to prove ownership.
    /// Only one pending rotation is allowed at a time.
    pub fn initiate_treasury_rotation(
        env: &Env,
        admin: &Address,
        new_address: Address,
    ) -> Result<RecipientRotationRequest, QuickLendXError> {
        admin.require_auth();

        if env
            .storage()
            .instance()
            .get::<_, RecipientRotationRequest>(&ROTATION_KEY)
            .is_some()
        {
            return Err(QuickLendXError::RotationAlreadyPending);
        }

        let current_treasury = Self::get_treasury_address(env);
        if let Some(ref existing) = current_treasury {
            if existing == &new_address {
                return Err(QuickLendXError::InvalidAddress);
            }
        }

        let now = env.ledger().timestamp();
        let request = RecipientRotationRequest {
            new_address,
            initiated_by: admin.clone(),
            initiated_at: now,
            confirmation_deadline: now.saturating_add(ROTATION_TTL_SECONDS),
        };

        env.storage().instance().set(&ROTATION_KEY, &request);
        Ok(request)
    }

    /// Confirm the pending treasury rotation.
    ///
    /// The new_address from the pending request must authorize this call,
    /// proving they control the destination before funds are ever routed there.
    /// Clears the rotation request and writes the new treasury address.
    pub fn confirm_treasury_rotation(
        env: &Env,
        new_address: &Address,
    ) -> Result<Address, QuickLendXError> {
        let request: RecipientRotationRequest = env
            .storage()
            .instance()
            .get(&ROTATION_KEY)
            .ok_or(QuickLendXError::RotationNotFound)?;

        if &request.new_address != new_address {
            return Err(QuickLendXError::Unauthorized);
        }

        new_address.require_auth();

        if env.ledger().timestamp() > request.confirmation_deadline {
            env.storage().instance().remove(&ROTATION_KEY);
            return Err(QuickLendXError::RotationExpired);
        }

        let mut platform_config = Self::get_platform_fee_config(env)?;
        platform_config.treasury_address = Some(new_address.clone());
        platform_config.updated_at = env.ledger().timestamp();
        platform_config.updated_by = new_address.clone();
        env.storage()
            .instance()
            .set(&PLATFORM_FEE_KEY, &platform_config);

        env.storage().instance().remove(&ROTATION_KEY);

        Ok(new_address.clone())
    }

    /// Cancel the pending treasury rotation (admin only).
    ///
    /// Can be called at any time before confirmation to abort the rotation.
    pub fn cancel_treasury_rotation(env: &Env, admin: &Address) -> Result<(), QuickLendXError> {
        admin.require_auth();

        if env
            .storage()
            .instance()
            .get::<_, RecipientRotationRequest>(&ROTATION_KEY)
            .is_none()
        {
            return Err(QuickLendXError::RotationNotFound);
        }

        env.storage().instance().remove(&ROTATION_KEY);
        Ok(())
    }

    /// Query any pending treasury rotation request.
    pub fn get_pending_rotation(env: &Env) -> Option<RecipientRotationRequest> {
        env.storage().instance().get(&ROTATION_KEY)
    }
}
