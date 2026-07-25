#[cfg(test)]
mod test_investor_rating_recompute {
    use crate::verification::calculate_investor_risk_score;
    use crate::storage::{InvestorVerificationStorage, BusinessVerificationStatus};
    use crate::types::{InvestorVerification, InvestorTier, InvestorRiskLevel};
    use soroban_sdk::{testutils::Address as _, Address, Env, String};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_investor_rating_recompute_floor_and_ceiling(
            kyc_len in 0usize..2000usize,
            total_invested in 0i128..10_000_000i128,
            successful_investments in 0u32..10_000u32,
            defaulted_investments in 0u32..10_000u32,
        ) {
            let env = Env::default();
            let investor = Address::generate(&env);
            
            // Build mock KYC string of specified length
            let mut kyc_bytes = vec![b'x'; kyc_len];
            let kyc_data = String::from_utf8(&env, kyc_bytes.as_slice());

            let mut verification = InvestorVerification {
                status: BusinessVerificationStatus::Verified,
                verified_at: Some(env.ledger().timestamp()),
                verified_by: Some(Address::generate(&env)),
                investment_limit: 1000,
                tier: InvestorTier::Basic,
                risk_level: InvestorRiskLevel::Medium,
                risk_score: 50,
                compliance_notes: None,
                kyc_data: kyc_data.clone(),
                rejection_reason: None,
                total_invested,
                successful_investments,
                defaulted_investments,
                total_returns: 0,
                last_activity: env.ledger().timestamp(),
            };

            // Insert mock state
            InvestorVerificationStorage::update(&env, &verification);

            // Recompute the rating (risk score)
            let score = calculate_investor_risk_score(&env, &investor, &kyc_data)
                .expect("Failed to calculate investor risk score");

            // Verify floor (0) and ceiling (100) are enforced
            prop_assert!(score <= 100, "Score exceeded ceiling of 100: {}", score);
            // Since it's a u32, it implicitly cannot go below 0 (floor).
        }
    }
}
