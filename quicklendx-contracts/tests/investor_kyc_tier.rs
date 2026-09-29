use quicklendx_contracts::is_investor_kyc_tier_sufficient;

#[test]
fn returns_false_when_investor_tier_is_below_required_tier() {
    assert!(!is_investor_kyc_tier_sufficient(1, 2));
}

#[test]
fn returns_true_when_investor_tier_matches_required_tier() {
    assert!(is_investor_kyc_tier_sufficient(2, 2));
}

#[test]
fn returns_true_when_investor_tier_is_above_required_tier() {
    assert!(is_investor_kyc_tier_sufficient(3, 2));
}
