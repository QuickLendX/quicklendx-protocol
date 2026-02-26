/// Test suite for risk score, tier calculation, and investment limit functions
///
/// Test Coverage:
/// 1. calculate_investor_risk_score - various kyc_data inputs
/// 2. determine_investor_tier - risk_score boundaries
/// 3. determine_risk_level - risk_score to risk level mapping
/// 4. calculate_investment_limit - tier and risk_level combinations
///
/// Target: 95%+ test coverage for risk and tier logic
#[cfg(test)]
mod test_risk_tier_calculation {
    use crate::verification::{
        calculate_investment_limit, calculate_investor_risk_score, determine_investor_tier,
        determine_risk_level, InvestorRiskLevel, InvestorTier, InvestorVerificationStorage,
    };
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        env.mock_all_auths();
        let _ = client.try_initialize_admin(&admin);
        let _ = client.try_initialize_protocol_limits(&admin, &1i128, &365u64, &86400u64);

        (env, client, admin)
    }

    // ============================================================================
    // calculate_investor_risk_score Tests
    // ============================================================================

    #[test]
    fn test_risk_score_incomplete_kyc() {
        let (env, _client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Short");

        let risk_score = calculate_investor_risk_score(&env, &investor, &kyc_data).unwrap();
        assert_eq!(
            risk_score, 30,
            "Incomplete KYC (< 100 chars) should add 30 risk"
        );
    }

    #[test]
    fn test_risk_score_medium_kyc() {
        let (env, _client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(
            &env,
            "This is a medium length KYC data with some information about the investor",
        );

        let risk_score = calculate_investor_risk_score(&env, &investor, &kyc_data).unwrap();
        assert_eq!(
            risk_score, 20,
            "Medium KYC (100-500 chars) should add 20 risk"
        );
    }

    #[test]
    fn test_risk_score_comprehensive_kyc() {
        let (env, _client, _admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(
            &env,
            "This is a comprehensive KYC data with all the necessary information about the investor including their identity verification, address verification, financial history, investment experience, risk tolerance assessment, and other relevant details required for proper due diligence and compliance purposes.",
        );

        let risk_score = calculate_investor_risk_score(&env, &investor, &kyc_data).unwrap();
        assert_eq!(
            risk_score, 10,
            "Comprehensive KYC (>= 500 chars) should add 10 risk"
        );
    }

    #[test]
    fn test_risk_score_with_high_default_rate() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data =
            String::from_str(&env, "Comprehensive KYC data with all required information");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.successful_investments = 5;
        v.defaulted_investments = 15;
        v.total_invested = 200000;

        InvestorVerificationStorage::update(&env, &v);

        let risk_score = calculate_investor_risk_score(&env, &investor, &kyc_data).unwrap();
        assert!(
            risk_score > 50,
            "High default rate (75%) should significantly increase risk score"
        );
    }

    #[test]
    fn test_risk_score_with_low_default_rate() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data =
            String::from_str(&env, "Comprehensive KYC data with all required information");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.successful_investments = 20;
        v.defaulted_investments = 1;
        v.total_invested = 500000;

        InvestorVerificationStorage::update(&env, &v);

        let risk_score = calculate_investor_risk_score(&env, &investor, &kyc_data).unwrap();
        assert!(
            risk_score < 30,
            "Low default rate (~5%) should keep risk score low"
        );
    }

    #[test]
    fn test_risk_score_with_high_total_invested() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data =
            String::from_str(&env, "Comprehensive KYC data with all required information");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.successful_investments = 10;
        v.defaulted_investments = 0;
        v.total_invested = 2000000;

        InvestorVerificationStorage::update(&env, &v);

        let risk_score = calculate_investor_risk_score(&env, &investor, &kyc_data).unwrap();
        assert!(
            risk_score < 20,
            "High total invested (2M+) should reduce risk score by 20"
        );
    }

    #[test]
    fn test_risk_score_capped_at_100() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Short");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.successful_investments = 1;
        v.defaulted_investments = 100;
        v.total_invested = 0;

        InvestorVerificationStorage::update(&env, &v);

        let risk_score = calculate_investor_risk_score(&env, &investor, &kyc_data).unwrap();
        assert!(risk_score <= 100, "Risk score should be capped at 100");
    }

    // ============================================================================
    // determine_investor_tier Tests
    // ============================================================================

    #[test]
    fn test_tier_vip() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Comprehensive KYC data");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.total_invested = 6000000;
        v.successful_investments = 60;

        InvestorVerificationStorage::update(&env, &v);

        let tier = determine_investor_tier(&env, &investor, 10).unwrap();
        assert_eq!(tier, InvestorTier::VIP, "Should be VIP tier");
    }

    #[test]
    fn test_tier_platinum() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Comprehensive KYC data");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.total_invested = 2000000;
        v.successful_investments = 25;

        InvestorVerificationStorage::update(&env, &v);

        let tier = determine_investor_tier(&env, &investor, 20).unwrap();
        assert_eq!(tier, InvestorTier::Platinum, "Should be Platinum tier");
    }

    #[test]
    fn test_tier_gold() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Comprehensive KYC data");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.total_invested = 200000;
        v.successful_investments = 15;

        InvestorVerificationStorage::update(&env, &v);

        let tier = determine_investor_tier(&env, &investor, 40).unwrap();
        assert_eq!(tier, InvestorTier::Gold, "Should be Gold tier");
    }

    #[test]
    fn test_tier_silver() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Comprehensive KYC data");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.total_invested = 50000;
        v.successful_investments = 5;

        InvestorVerificationStorage::update(&env, &v);

        let tier = determine_investor_tier(&env, &investor, 55).unwrap();
        assert_eq!(tier, InvestorTier::Silver, "Should be Silver tier");
    }

    #[test]
    fn test_tier_basic() {
        let (env, _client, _admin) = setup();
        let investor = Address::generate(&env);

        let tier = determine_investor_tier(&env, &investor, 70).unwrap();
        assert_eq!(
            tier,
            InvestorTier::Basic,
            "Should be Basic tier for new investor"
        );
    }

    #[test]
    fn test_tier_boundary_10() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Comprehensive KYC data");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.total_invested = 6000000;
        v.successful_investments = 60;

        InvestorVerificationStorage::update(&env, &v);

        let tier = determine_investor_tier(&env, &investor, 10).unwrap();
        assert_eq!(tier, InvestorTier::VIP, "Risk score 10 should be VIP");
    }

    #[test]
    fn test_tier_boundary_11() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Comprehensive KYC data");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.total_invested = 6000000;
        v.successful_investments = 60;

        InvestorVerificationStorage::update(&env, &v);

        let tier = determine_investor_tier(&env, &investor, 11).unwrap();
        assert_ne!(tier, InvestorTier::VIP, "Risk score 11 should not be VIP");
    }

    #[test]
    fn test_tier_boundary_20() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Comprehensive KYC data");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.total_invested = 2000000;
        v.successful_investments = 25;

        InvestorVerificationStorage::update(&env, &v);

        let tier = determine_investor_tier(&env, &investor, 20).unwrap();
        assert_eq!(
            tier,
            InvestorTier::Platinum,
            "Risk score 20 should be Platinum"
        );
    }

    #[test]
    fn test_tier_boundary_40() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Comprehensive KYC data");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.total_invested = 200000;
        v.successful_investments = 15;

        InvestorVerificationStorage::update(&env, &v);

        let tier = determine_investor_tier(&env, &investor, 40).unwrap();
        assert_eq!(tier, InvestorTier::Gold, "Risk score 40 should be Gold");
    }

    #[test]
    fn test_tier_boundary_60() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Comprehensive KYC data");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.total_invested = 50000;
        v.successful_investments = 5;

        InvestorVerificationStorage::update(&env, &v);

        let tier = determine_investor_tier(&env, &investor, 60).unwrap();
        assert_eq!(tier, InvestorTier::Silver, "Risk score 60 should be Silver");
    }

    #[test]
    fn test_tier_boundary_61() {
        let (env, client, admin) = setup();
        let investor = Address::generate(&env);
        let kyc_data = String::from_str(&env, "Comprehensive KYC data");

        let _ = client.try_submit_investor_kyc(&investor, &kyc_data);
        let _ = client.try_approve_investor_kyc(&admin, &investor);

        let verification = client.get_investor_verification(&investor).unwrap();
        let mut v = verification;
        v.total_invested = 50000;
        v.successful_investments = 5;

        InvestorVerificationStorage::update(&env, &v);

        let tier = determine_investor_tier(&env, &investor, 61).unwrap();
        assert_eq!(tier, InvestorTier::Basic, "Risk score 61 should be Basic");
    }

    // ============================================================================
    // determine_risk_level Tests
    // ============================================================================

    #[test]
    fn test_risk_level_low() {
        assert_eq!(determine_risk_level(0), InvestorRiskLevel::Low);
        assert_eq!(determine_risk_level(25), InvestorRiskLevel::Low);
    }

    #[test]
    fn test_risk_level_medium() {
        assert_eq!(determine_risk_level(26), InvestorRiskLevel::Medium);
        assert_eq!(determine_risk_level(50), InvestorRiskLevel::Medium);
    }

    #[test]
    fn test_risk_level_high() {
        assert_eq!(determine_risk_level(51), InvestorRiskLevel::High);
        assert_eq!(determine_risk_level(75), InvestorRiskLevel::High);
    }

    #[test]
    fn test_risk_level_very_high() {
        assert_eq!(determine_risk_level(76), InvestorRiskLevel::VeryHigh);
        assert_eq!(determine_risk_level(100), InvestorRiskLevel::VeryHigh);
    }

    // ============================================================================
    // calculate_investment_limit Tests
    // ============================================================================

    #[test]
    fn test_investment_limit_vip_low() {
        let limit = calculate_investment_limit(&InvestorTier::VIP, &InvestorRiskLevel::Low, 100000);
        assert_eq!(limit, 1000000, "VIP + Low should be 10x at 100%");
    }

    #[test]
    fn test_investment_limit_vip_medium() {
        let limit =
            calculate_investment_limit(&InvestorTier::VIP, &InvestorRiskLevel::Medium, 100000);
        assert_eq!(limit, 750000, "VIP + Medium should be 10x at 75%");
    }

    #[test]
    fn test_investment_limit_vip_high() {
        let limit =
            calculate_investment_limit(&InvestorTier::VIP, &InvestorRiskLevel::High, 100000);
        assert_eq!(limit, 500000, "VIP + High should be 10x at 50%");
    }

    #[test]
    fn test_investment_limit_vip_very_high() {
        let limit =
            calculate_investment_limit(&InvestorTier::VIP, &InvestorRiskLevel::VeryHigh, 100000);
        assert_eq!(limit, 250000, "VIP + VeryHigh should be 10x at 25%");
    }

    #[test]
    fn test_investment_limit_platinum_low() {
        let limit =
            calculate_investment_limit(&InvestorTier::Platinum, &InvestorRiskLevel::Low, 100000);
        assert_eq!(limit, 500000, "Platinum + Low should be 5x at 100%");
    }

    #[test]
    fn test_investment_limit_platinum_medium() {
        let limit =
            calculate_investment_limit(&InvestorTier::Platinum, &InvestorRiskLevel::Medium, 100000);
        assert_eq!(limit, 375000, "Platinum + Medium should be 5x at 75%");
    }

    #[test]
    fn test_investment_limit_platinum_high() {
        let limit =
            calculate_investment_limit(&InvestorTier::Platinum, &InvestorRiskLevel::High, 100000);
        assert_eq!(limit, 250000, "Platinum + High should be 5x at 50%");
    }

    #[test]
    fn test_investment_limit_platinum_very_high() {
        let limit = calculate_investment_limit(
            &InvestorTier::Platinum,
            &InvestorRiskLevel::VeryHigh,
            100000,
        );
        assert_eq!(limit, 125000, "Platinum + VeryHigh should be 5x at 25%");
    }

    #[test]
    fn test_investment_limit_gold_low() {
        let limit =
            calculate_investment_limit(&InvestorTier::Gold, &InvestorRiskLevel::Low, 100000);
        assert_eq!(limit, 300000, "Gold + Low should be 3x at 100%");
    }

    #[test]
    fn test_investment_limit_gold_medium() {
        let limit =
            calculate_investment_limit(&InvestorTier::Gold, &InvestorRiskLevel::Medium, 100000);
        assert_eq!(limit, 225000, "Gold + Medium should be 3x at 75%");
    }

    #[test]
    fn test_investment_limit_gold_high() {
        let limit =
            calculate_investment_limit(&InvestorTier::Gold, &InvestorRiskLevel::High, 100000);
        assert_eq!(limit, 150000, "Gold + High should be 3x at 50%");
    }

    #[test]
    fn test_investment_limit_gold_very_high() {
        let limit =
            calculate_investment_limit(&InvestorTier::Gold, &InvestorRiskLevel::VeryHigh, 100000);
        assert_eq!(limit, 75000, "Gold + VeryHigh should be 3x at 25%");
    }

    #[test]
    fn test_investment_limit_silver_low() {
        let limit =
            calculate_investment_limit(&InvestorTier::Silver, &InvestorRiskLevel::Low, 100000);
        assert_eq!(limit, 200000, "Silver + Low should be 2x at 100%");
    }

    #[test]
    fn test_investment_limit_silver_medium() {
        let limit =
            calculate_investment_limit(&InvestorTier::Silver, &InvestorRiskLevel::Medium, 100000);
        assert_eq!(limit, 150000, "Silver + Medium should be 2x at 75%");
    }

    #[test]
    fn test_investment_limit_silver_high() {
        let limit =
            calculate_investment_limit(&InvestorTier::Silver, &InvestorRiskLevel::High, 100000);
        assert_eq!(limit, 100000, "Silver + High should be 2x at 50%");
    }

    #[test]
    fn test_investment_limit_silver_very_high() {
        let limit =
            calculate_investment_limit(&InvestorTier::Silver, &InvestorRiskLevel::VeryHigh, 100000);
        assert_eq!(limit, 50000, "Silver + VeryHigh should be 2x at 25%");
    }

    #[test]
    fn test_investment_limit_basic_low() {
        let limit =
            calculate_investment_limit(&InvestorTier::Basic, &InvestorRiskLevel::Low, 100000);
        assert_eq!(limit, 100000, "Basic + Low should be 1x at 100%");
    }

    #[test]
    fn test_investment_limit_basic_medium() {
        let limit =
            calculate_investment_limit(&InvestorTier::Basic, &InvestorRiskLevel::Medium, 100000);
        assert_eq!(limit, 75000, "Basic + Medium should be 1x at 75%");
    }

    #[test]
    fn test_investment_limit_basic_high() {
        let limit =
            calculate_investment_limit(&InvestorTier::Basic, &InvestorRiskLevel::High, 100000);
        assert_eq!(limit, 50000, "Basic + High should be 1x at 50%");
    }

    #[test]
    fn test_investment_limit_basic_very_high() {
        let limit =
            calculate_investment_limit(&InvestorTier::Basic, &InvestorRiskLevel::VeryHigh, 100000);
        assert_eq!(limit, 25000, "Basic + VeryHigh should be 1x at 25%");
    }

    #[test]
    fn test_investment_limit_zero_base() {
        let limit = calculate_investment_limit(&InvestorTier::VIP, &InvestorRiskLevel::Low, 0);
        assert_eq!(limit, 0, "Zero base limit should return 0");
    }

    #[test]
    fn test_investment_limit_saturation() {
        let limit =
            calculate_investment_limit(&InvestorTier::VIP, &InvestorRiskLevel::Low, i128::MAX);
        assert!(limit > 0, "Should handle max i128 without overflow");
    }
}
