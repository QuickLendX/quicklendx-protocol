pub fn compute_investor_tier(
    total_invested: i128,
    successful_investments: u32,
    defaulted_investments: u32,
    risk_score: u32,
) -> InvestorTier {
    // Logic remains the same
    if total_invested > 5_000_000 && successful_investments > 50 && defaulted_investments < 5 {
        InvestorTier::Vip
    } else if total_invested > 1_000_000 && successful_investments > 20 {
        InvestorTier::Platinum
    } else if total_invested > 100_000 && successful_investments > 10 {
        InvestorTier::Gold
    } else if total_invested > 10_000 && successful_investments > 3 {
        InvestorTier::Silver
    } else {
        InvestorTier::Basic
    }
}