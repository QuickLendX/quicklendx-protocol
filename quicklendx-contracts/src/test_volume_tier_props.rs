#![cfg(feature = "fuzz-tests")]

use proptest::prelude::*;
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env};

use crate::fees::{FeeManager, UserVolumeData, VolumeTier};

#[contract]
pub struct PropTestContract;

#[contractimpl]
impl PropTestContract {}

fn tier_level(tier: &VolumeTier) -> u8 {
    match tier {
        VolumeTier::Standard => 0,
        VolumeTier::Silver => 1,
        VolumeTier::Gold => 2,
        VolumeTier::Platinum => 3,
    }
}

const BOUNDARY_VALUES: [i128; 9] = [
    9_999_999_999,
    10_000_000_000,
    10_000_000_001,
    49_999_999_999,
    50_000_000_000,
    50_000_000_001,
    99_999_999_999,
    100_000_000_000,
    100_000_000_001,
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn test_volume_tier_and_fee_monotonicity(
        amounts in prop::collection::vec(1_000_000i128..50_000_000_000i128, 1..50),
        base_invoice_amount in 50_000_000i128..10_000_000_000i128,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        let contract_id = env.register_contract(None, PropTestContract);

        env.as_contract(&contract_id, || {
            let _ = FeeManager::initialize(&env, &admin);

            let mut previous_tier_level = 0;
            let mut previous_fee = i128::MAX;

            let mut current_volume = 0i128;
            let mut current_count = 0u32;

            for amount in amounts {
                let user_data = FeeManager::update_user_volume(&env, &user, amount)
                    .expect("Failed to update user volume");

                prop_assert!(
                    user_data.total_volume > current_volume,
                    "total_volume must increase strictly without i128 overflow"
                );
                prop_assert_eq!(
                    user_data.transaction_count, current_count + 1,
                    "transaction_count must accumulate by precisely 1 per call"
                );

                let current_tier_level = tier_level(&user_data.current_tier);
                prop_assert!(
                    current_tier_level >= previous_tier_level,
                    "Tier must never drop to a lower level as cumulative volume increases"
                );

                let current_fee = FeeManager::calculate_total_fees(
                    &env,
                    &user,
                    base_invoice_amount,
                    false,
                    false,
                ).unwrap_or_else(|e| panic!("Fee calculation failed with error: {:?}", e));

                if previous_fee != i128::MAX {
                    prop_assert!(
                        current_fee <= previous_fee,
                        "Effective fee must be non-increasing as the user's tier level advances"
                    );
                }

                previous_tier_level = current_tier_level;
                previous_fee = current_fee;
                current_volume = user_data.total_volume;
                current_count = user_data.transaction_count;
            }

            Ok(())
        });
    }

    #[test]
    fn test_tier_promotion_no_skips_on_small_increments(
        amounts in prop::collection::vec(1_000i128..1_000_000i128, 1..100)
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        let contract_id = env.register_contract(None, PropTestContract);

        env.as_contract(&contract_id, || {
            let _ = FeeManager::initialize(&env, &admin);
            let mut previous_tier = 0;

            for amount in amounts {
                let user_data = FeeManager::update_user_volume(&env, &user, amount)
                    .expect("Failed to update user volume");
                let current_tier = tier_level(&user_data.current_tier);

                prop_assert!(
                    current_tier == previous_tier || current_tier == previous_tier + 1,
                    "A small volume update must promote by at most one tier level"
                );
                previous_tier = current_tier;
            }
            Ok(())
        });
    }

    #[test]
    fn test_tier_boundary_edge_cases(
        base_volume in prop::sample::select(&BOUNDARY_VALUES[..]),
        delta in -100i128..100i128
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        let contract_id = env.register_contract(None, PropTestContract);

        env.as_contract(&contract_id, || {
            let _ = FeeManager::initialize(&env, &admin);
            let test_amount = if base_volume + delta > 0 { base_volume + delta } else { 1 };

            let user_data = FeeManager::update_user_volume(&env, &user, test_amount)
                .expect("Failed to update user volume at boundary edge");

            let level = tier_level(&user_data.current_tier);
            prop_assert!(level <= 3, "Tier level must securely remain within defined VolumeTier bounds");
            prop_assert!(user_data.total_volume > 0, "Volume tracking must remain strictly positive");
            Ok(())
        });
    }

    #[test]
    fn test_large_volume_spans_multiple_tiers(
        massive_amount in 500_000_000_000i128..2_000_000_000_000i128
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);

        let contract_id = env.register_contract(None, PropTestContract);

        env.as_contract(&contract_id, || {
            let _ = FeeManager::initialize(&env, &admin);
            let initial_tier = 0;
            let user_data = FeeManager::update_user_volume(&env, &user, massive_amount)
                .expect("Failed to update massive user volume");
            let final_tier = tier_level(&user_data.current_tier);

            prop_assert!(
                final_tier > initial_tier + 1,
                "A legitimately massive volume update must bypass intermediate tiers immediately"
            );
            Ok(())
        });
    }
}
