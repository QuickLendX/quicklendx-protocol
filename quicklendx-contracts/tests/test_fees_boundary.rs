//! Integration tests and proptest for fee boundaries, surcharge bounds,
//! whole-transaction bounds, and cross-fee validator consistency (Issue #2968).

use proptest::prelude::*;
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env};

pub use quicklendx_contracts::errors;
use quicklendx_contracts::errors::QuickLendXError;

pub mod audit {
    use soroban_sdk::{Address, Env, String};
    #[derive(Clone, Copy)]
    pub enum AuditOperation {
        ConfigFeeStructureChanged,
        ConfigRevenueDistributionChanged,
    }
    pub fn log_config_change(
        _env: &Env,
        _op: AuditOperation,
        _actor: Address,
        _target: &str,
        _old: Option<String>,
        _new: Option<String>,
    ) {
    }
    pub fn write_i128_to_buf(_buf: &mut [u8], _val: i128) -> usize {
        0
    }
    pub fn write_u64_to_buf(_buf: &mut [u8], _val: u64) -> usize {
        0
    }
}

pub mod events {
    use super::fees::FeeType;
    use soroban_sdk::{Address, Env};
    pub fn emit_treasury_configured(_env: &Env, _addr: &Address, _admin: &Address) {}
    pub fn emit_fee_structure_updated(
        _env: &Env,
        _fee_type: &FeeType,
        _old: u32,
        _new: u32,
        _admin: &Address,
    ) {
    }
    pub fn emit_platform_fee_config_updated(_env: &Env, _old: u32, _new: u32, _admin: &Address) {}
    pub fn emit_revenue_distributed(_env: &Env, _period: u64, _t: i128, _d: i128, _p: i128) {}
    pub fn emit_treasury_rotation_initiated(
        _env: &Env,
        _admin: &Address,
        _new_addr: &Address,
        _deadline: u64,
    ) {
    }
    pub fn emit_treasury_rotation_confirmed(_env: &Env, _old: &Address, _new_addr: &Address) {}
    pub fn treasury_rotation_cancelled(_env: &Env, _admin: &Address) {}
}

pub mod payments {
    use quicklendx_contracts::errors::QuickLendXError;
    use soroban_sdk::{Address, Env};
    pub fn transfer_funds(
        _env: &Env,
        _currency: &Address,
        _from: &Address,
        _to: &Address,
        _amt: i128,
    ) -> Result<(), QuickLendXError> {
        Ok(())
    }
}

pub struct AdminStorage;
impl AdminStorage {
    pub fn require_admin(_env: &Env, _admin: &Address) -> Result<(), QuickLendXError> {
        Ok(())
    }
}

#[path = "../src/fees.rs"]
pub mod fees;

use fees::{FeeManager, FeeStructure, FeeType, MAX_LATE_PAYMENT_PENALTY_BPS};

#[contract]
pub struct MockFeeContract;

#[contractimpl]
impl MockFeeContract {
    pub fn init_fees(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        FeeManager::initialize(&env, &admin)
    }

    pub fn update_fee(
        env: Env,
        admin: Address,
        fee_type: FeeType,
        base_bps: u32,
        min_fee: i128,
        max_fee: i128,
        is_active: bool,
    ) -> Result<FeeStructure, QuickLendXError> {
        FeeManager::update_fee_structure(
            &env,
            &admin,
            fee_type,
            base_bps,
            min_fee,
            max_fee,
            is_active,
        )
    }

    pub fn calc_fees(
        env: Env,
        user: Address,
        amount: i128,
        is_early: bool,
        is_late: bool,
        penalty_bps: Option<u32>,
    ) -> Result<i128, QuickLendXError> {
        FeeManager::calculate_total_fees(&env, &user, amount, is_early, is_late, penalty_bps)
    }
}

/// Unit test `fees_never_exceed_amount`:
/// Reproduction step 2: fees >= amount returns `Err(QuickLendXError::InvalidAmount)`.
#[test]
fn test_fees_never_exceed_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(MockFeeContract, ());
    let client = MockFeeContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init_fees(&admin);

    // Default schedule minimums sum to 100 + 50 + 100 = 250.
    // Reproduction step 2: amount = 1 (1 < 250) must reject with InvalidAmount.
    let res_1 = client.try_calc_fees(&user, &1, &false, &false, &None);
    assert_eq!(res_1, Err(Ok(QuickLendXError::InvalidAmount)));

    // Exactly at boundary: amount = 250 (250 >= 250) must reject with InvalidAmount.
    let res_250 = client.try_calc_fees(&user, &250, &false, &false, &None);
    assert_eq!(res_250, Err(Ok(QuickLendXError::InvalidAmount)));

    // Just above boundary: amount = 251 (250 < 251) must succeed with Ok(250).
    let res_251 = client.try_calc_fees(&user, &251, &false, &false, &None);
    assert_eq!(res_251, Ok(Ok(250)));

    // Larger transaction: amount = 10_000 -> fees = 350 < 10_000.
    let res_10k = client.try_calc_fees(&user, &10_000, &false, &false, &None);
    assert_eq!(res_10k, Ok(Ok(350)));
}

/// Unit test `late_surcharge_bps_capped`:
/// Tests that surcharge > 5_000 bps gives `InvalidFeeBasisPoints`, and <= 5_000 succeeds.
#[test]
fn test_late_surcharge_bps_capped() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(MockFeeContract, ());
    let client = MockFeeContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.init_fees(&admin);

    // Add LatePayment structure: base_bps=100, min=100, max=5_000_000
    client.update_fee(
        &admin,
        &FeeType::LatePayment,
        &100,
        &100,
        &5_000_000,
        &true,
    );

    // 5_001 gives InvalidFeeBasisPoints
    let res_5001 = client.try_calc_fees(
        &user,
        &1_000_000,
        &false,
        &true,
        &Some(5_001),
    );
    assert_eq!(res_5001, Err(Ok(QuickLendXError::InvalidFeeBasisPoints)));

    // u32::MAX gives InvalidFeeBasisPoints
    let res_max = client.try_calc_fees(
        &user,
        &1_000_000,
        &false,
        &true,
        &Some(u32::MAX),
    );
    assert_eq!(res_max, Err(Ok(QuickLendXError::InvalidFeeBasisPoints)));

    // 5_000 succeeds (MAX_LATE_PAYMENT_PENALTY_BPS)
    let res_5000 = client.try_calc_fees(
        &user,
        &1_000_000,
        &false,
        &true,
        &Some(MAX_LATE_PAYMENT_PENALTY_BPS),
    );
    assert!(res_5000.is_ok(), "5000 bps surcharge must succeed");
    let fees = res_5000.unwrap().unwrap();
    assert!(fees < 1_000_000);

    // 0 bps surcharge also succeeds
    let res_0 = client.try_calc_fees(
        &user,
        &1_000_000,
        &false,
        &true,
        &Some(0),
    );
    assert!(res_0.is_ok(), "0 bps surcharge must succeed");
}

/// Unit test `update_existing_fee_near_cap`:
/// Tests reproduction steps 4 & 5 and pushing beyond the cap.
#[test]
fn test_update_existing_fee_near_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(MockFeeContract, ());
    let client = MockFeeContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);

    client.init_fees(&admin);

    // Default structures: Platform: 100, Processing: 50, Verification: 100.
    // Sum without Platform: 50 + 100 = 150.
    // MAX_TOTAL_MIN_FEES = 5_000_000_000_000 / 2 = 2_500_000_000_000.
    // Reproduction step 4: updating Platform to flat fee 2_499_999_999_850:
    // Total active min fees = 150 + 2_499_999_999_850 = 2_500_000_000_000 (exactly the cap).
    let res_step4 = client.try_update_fee(
        &admin,
        &FeeType::Platform,
        &0,
        &2_499_999_999_850,
        &2_499_999_999_850,
        &true,
    );
    assert!(
        matches!(res_step4, Ok(Ok(_))),
        "Updating Platform to reach exactly the cap must succeed: {:?}",
        res_step4
    );

    // Reproduction step 5:
    // 150 + 2_499_999_999_750 = 2_499_999_999_900 (< cap).
    // Calling it again (updating existing structure with same value) must NOT double-count.
    let res_step5_first = client.try_update_fee(
        &admin,
        &FeeType::Platform,
        &0,
        &2_499_999_999_750,
        &2_499_999_999_750,
        &true,
    );
    assert!(
        matches!(res_step5_first, Ok(Ok(_))),
        "First update must succeed: {:?}",
        res_step5_first
    );

    let res_step5_repeat = client.try_update_fee(
        &admin,
        &FeeType::Platform,
        &0,
        &2_499_999_999_750,
        &2_499_999_999_750,
        &true,
    );
    assert!(
        matches!(res_step5_repeat, Ok(Ok(_))),
        "Repeating identical update without double-counting must succeed: {:?}",
        res_step5_repeat
    );

    // Pushing real post-update total to MAX_TOTAL_MIN_FEES + 1:
    // 150 + 2_499_999_999_851 = 2_500_000_000_001 (> MAX_TOTAL_MIN_FEES).
    // Must fail with InvalidFeeConfiguration.
    let res_over_cap = client.try_update_fee(
        &admin,
        &FeeType::Platform,
        &0,
        &2_499_999_999_851,
        &2_499_999_999_851,
        &true,
    );
    match res_over_cap {
        Err(Ok(QuickLendXError::InvalidFeeConfiguration)) => {}
        other => panic!("Expected InvalidFeeConfiguration, got {:?}", other),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Property test (>= 256 cases, configured for 500 cases):
    /// Random schedule within the validator's limits, random amount in [1, 10^15], random flags.
    /// Either Ok(f) with 0 <= f < amount, or Err(QuickLendXError::InvalidAmount).
    #[test]
    fn test_property_fee_quote_never_exceeds_amount(
        amount in 1i128..=1_000_000_000_000_000i128, // [1, 10^15]
        is_early in any::<bool>(),
        is_late in any::<bool>(),
        penalty_bps in prop::option::of(0u32..=5_000u32),
        platform_bps in 0u32..=1_000u32,
        processing_bps in 0u32..=500u32,
        verification_bps in 0u32..=500u32,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(MockFeeContract, ());
        let client = MockFeeContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        client.init_fees(&admin);

        // Update structures within valid bounds
        let _ = client.try_update_fee(
            &admin,
            &FeeType::Platform,
            &platform_bps,
            &10,
            &1_000_000,
            &true,
        );
        let _ = client.try_update_fee(
            &admin,
            &FeeType::Processing,
            &processing_bps,
            &5,
            &500_000,
            &true,
        );
        let _ = client.try_update_fee(
            &admin,
            &FeeType::Verification,
            &verification_bps,
            &10,
            &500_000,
            &true,
        );
        let _ = client.try_update_fee(
            &admin,
            &FeeType::LatePayment,
            &100,
            &10,
            &1_000_000,
            &true,
        );

        let res = client.try_calc_fees(
            &user,
            &amount,
            &is_early,
            &is_late,
            &penalty_bps,
        );

        match res {
            Ok(Ok(fee)) => {
                prop_assert!(
                    fee >= 0,
                    "Calculated fee must be non-negative, got {}", fee
                );
                prop_assert!(
                    fee < amount,
                    "Calculated fee ({}) must be strictly less than transaction amount ({})",
                    fee, amount
                );
            }
            Err(Ok(QuickLendXError::InvalidAmount)) => {
                // Expected when total_fees >= amount
            }
            other => {
                prop_assert!(
                    false,
                    "Unexpected result: {:?}", other
                );
            }
        }
    }
}
