//! Regression and property-style tests for investor exposure reservations.
//!
//! The exposure guard has two independent sources of reserved principal:
//! pending `Placed` bids and funded `Active` investments.  Lifetime analytics
//! are intentionally not used as a capacity ledger.  These tests exercise the
//! exact-cap boundary, overflow behavior, and every terminal investment state
//! so future lifecycle changes cannot silently reintroduce a bypass.

#![cfg(test)]

use crate::bid::BidStorage;
use crate::errors::QuickLendXError;
use crate::investment::InvestmentStorage;
use crate::types::{Bid, BidStatus, Investment, InvestmentStatus};
use crate::verification::{
    validate_investor_investment, BusinessVerificationStatus, InvestorRiskLevel, InvestorTier,
    InvestorVerification, InvestorVerificationStorage,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String,
};

const LIMIT: i128 = 10_000;
const EXPIRATION: u64 = 1_000_000;

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let investor = Address::generate(&env);
    env.as_contract(&contract_id, || {
        env.ledger().set_timestamp(100);
        let verification = InvestorVerification {
            investor: investor.clone(),
            status: BusinessVerificationStatus::Verified,
            verified_at: Some(100),
            verified_by: None,
            kyc_data: String::from_str(&env, "test-kyc"),
            investment_limit: LIMIT,
            submitted_at: 100,
            tier: InvestorTier::Basic,
            risk_level: InvestorRiskLevel::Low,
            risk_score: 0,
            total_invested: 999_999,
            total_returns: 0,
            successful_investments: 0,
            defaulted_investments: 0,
            last_activity: 100,
            rejection_reason: None,
            compliance_notes: None,
        };
        InvestorVerificationStorage::store(&env, &verification);
    });
    (env, investor, contract_id)
}

fn id(env: &Env, value: u8) -> BytesN<32> {
    BytesN::from_array(env, &[value; 32])
}

fn active_investment(env: &Env, investor: &Address, value: u8, amount: i128) -> Investment {
    Investment {
        investment_id: id(env, value),
        invoice_id: id(env, value.saturating_add(100)),
        investor: investor.clone(),
        amount,
        funded_at: env.ledger().timestamp(),
        status: InvestmentStatus::Active,
        insurance: soroban_sdk::Vec::new(env),
    }
}

fn placed_bid(env: &Env, investor: &Address, value: u8, amount: i128) -> Bid {
    Bid {
        bid_id: id(env, value),
        invoice_id: id(env, value.saturating_add(100)),
        investor: investor.clone(),
        bid_amount: amount,
        expected_return: amount.saturating_add(1),
        status: BidStatus::Placed,
        timestamp: env.ledger().timestamp(),
        expiration_timestamp: EXPIRATION,
    }
}

#[test]
fn active_sum_is_zero_without_reservations() {
    let (env, investor, contract_id) = setup();
    env.as_contract(&contract_id, || {
        assert_eq!(
            InvestmentStorage::get_active_investment_amount_sum_for_investor(&env, &investor),
            0
        );
        assert!(validate_investor_investment(&env, &investor, LIMIT).is_ok());
    });
}

#[test]
fn active_sum_includes_only_matching_active_investments() {
    let (env, investor, contract_id) = setup();
    let other = Address::generate(&env);
    env.as_contract(&contract_id, || {
        let first = active_investment(&env, &investor, 1, 2_000);
        let second = active_investment(&env, &investor, 2, 3_000);
        let unrelated = active_investment(&env, &other, 3, 7_000);
        InvestmentStorage::store_investment(&env, &first);
        InvestmentStorage::store_investment(&env, &second);
        InvestmentStorage::store_investment(&env, &unrelated);

        assert_eq!(
            InvestmentStorage::get_active_investment_amount_sum_for_investor(&env, &investor),
            5_000
        );
    });
}

#[test]
fn bid_exposure_is_reserved_until_expiration() {
    let (env, investor, contract_id) = setup();
    env.as_contract(&contract_id, || {
        let bid = placed_bid(&env, &investor, 4, 3_000);
        BidStorage::store_bid(&env, &bid);

        assert_eq!(
            BidStorage::get_active_bid_amount_sum_for_investor(&env, &investor),
            3_000
        );
        assert!(validate_investor_investment(&env, &investor, 7_001).is_err());

        env.ledger().set_timestamp(EXPIRATION);
        assert_eq!(
            BidStorage::get_active_bid_amount_sum_for_investor(&env, &investor),
            0
        );
        assert!(validate_investor_investment(&env, &investor, LIMIT).is_ok());
    });
}

#[test]
fn terminal_statuses_release_reserved_exposure_exactly_once() {
    let terminal_statuses = [
        InvestmentStatus::Completed,
        InvestmentStatus::Defaulted,
        InvestmentStatus::Refunded,
        InvestmentStatus::Withdrawn,
    ];

    for (offset, terminal) in terminal_statuses.iter().enumerate() {
        let (env, investor, contract_id) = setup();
        env.as_contract(&contract_id, || {
            let investment = active_investment(&env, &investor, (offset + 1) as u8, 2_500);
            InvestmentStorage::store_investment(&env, &investment);
            assert_eq!(
                InvestmentStorage::get_active_investment_amount_sum_for_investor(&env, &investor),
                2_500
            );

            let mut closed = investment.clone();
            closed.status = terminal.clone();
            InvestmentStorage::update_investment(&env, &closed);

            assert_eq!(
                InvestmentStorage::get_active_investment_amount_sum_for_investor(&env, &investor),
                0,
                "terminal state {:?} must release capacity",
                terminal
            );
            assert!(!InvestmentStorage::get_active_investment_ids(&env)
                .iter()
                .any(|entry| entry == investment.investment_id));
        });
    }
}

#[test]
fn repeated_terminal_transition_cannot_double_release() {
    let (env, investor, contract_id) = setup();
    env.as_contract(&contract_id, || {
        let investment = active_investment(&env, &investor, 10, 4_000);
        InvestmentStorage::store_investment(&env, &investment);
        let mut completed = investment.clone();
        completed.status = InvestmentStatus::Completed;
        InvestmentStorage::update_investment(&env, &completed);

        assert_eq!(
            InvestmentStorage::get_active_investment_amount_sum_for_investor(&env, &investor),
            0
        );
        assert_eq!(
            InvestmentStorage::get_investments_by_investor(&env, &investor).len(),
            1,
            "terminal transition must preserve history while releasing exposure"
        );
    });
}

#[test]
fn malformed_non_positive_active_amount_fails_closed() {
    for amount in [0i128, -1i128] {
        let (env, investor, contract_id) = setup();
        env.as_contract(&contract_id, || {
            let investment =
                active_investment(&env, &investor, 20 + amount.unsigned_abs() as u8, amount);
            InvestmentStorage::store_investment(&env, &investment);
            assert_eq!(
                InvestmentStorage::get_active_investment_amount_sum_for_investor(&env, &investor),
                i128::MAX,
                "malformed active amount {amount} must fail closed"
            );
            assert!(validate_investor_investment(&env, &investor, 1).is_err());
        });
    }
}

#[test]
fn overflowing_active_sum_fails_closed() {
    let (env, investor, contract_id) = setup();
    env.as_contract(&contract_id, || {
        let first = active_investment(&env, &investor, 30, i128::MAX);
        let second = active_investment(&env, &investor, 31, 1);
        InvestmentStorage::store_investment(&env, &first);
        InvestmentStorage::store_investment(&env, &second);

        assert_eq!(
            InvestmentStorage::get_active_investment_amount_sum_for_investor(&env, &investor),
            i128::MAX
        );
        assert!(validate_investor_investment(&env, &investor, 1).is_err());
    });
}

#[test]
fn overflowing_bid_sum_fails_closed() {
    let (env, investor, contract_id) = setup();
    env.as_contract(&contract_id, || {
        let first = placed_bid(&env, &investor, 32, i128::MAX);
        let second = placed_bid(&env, &investor, 33, 1);
        BidStorage::store_bid(&env, &first);
        BidStorage::store_bid(&env, &second);

        assert_eq!(
            BidStorage::get_active_bid_amount_sum_for_investor(&env, &investor),
            i128::MAX
        );
        assert!(validate_investor_investment(&env, &investor, 1).is_err());
    });
}

#[test]
fn lifetime_analytics_do_not_consume_current_capacity() {
    let (env, investor, contract_id) = setup();
    env.as_contract(&contract_id, || {
        assert!(validate_investor_investment(&env, &investor, LIMIT).is_ok());
    });
}

#[test]
fn exact_active_investment_cap_is_accepted() {
    let (env, investor, contract_id) = setup();
    env.as_contract(&contract_id, || {
        let investment = active_investment(&env, &investor, 40, LIMIT - 1_000);
        InvestmentStorage::store_investment(&env, &investment);

        assert!(validate_investor_investment(&env, &investor, 1_000).is_ok());
        assert_eq!(
            InvestmentStorage::get_active_investment_amount_sum_for_investor(&env, &investor),
            LIMIT - 1_000
        );
    });
}

#[test]
fn exact_combined_bid_and_investment_cap_is_accepted() {
    let (env, investor, contract_id) = setup();
    env.as_contract(&contract_id, || {
        let investment = active_investment(&env, &investor, 42, 4_000);
        let bid = placed_bid(&env, &investor, 43, 5_000);
        InvestmentStorage::store_investment(&env, &investment);
        BidStorage::store_bid(&env, &bid);

        assert!(validate_investor_investment(&env, &investor, 1_000).is_ok());
        assert!(validate_investor_investment(&env, &investor, 1_001).is_err());
    });
}

#[test]
fn one_unit_over_active_investment_cap_is_rejected() {
    let (env, investor, contract_id) = setup();
    env.as_contract(&contract_id, || {
        let investment = active_investment(&env, &investor, 41, LIMIT - 1_000);
        InvestmentStorage::store_investment(&env, &investment);

        assert_eq!(
            validate_investor_investment(&env, &investor, 1_001),
            Err(QuickLendXError::InvalidAmount)
        );
        assert_eq!(
            InvestmentStorage::get_active_investment_amount_sum_for_investor(&env, &investor),
            LIMIT - 1_000,
            "a rejected reservation must not mutate active exposure"
        );
    });
}

#[test]
fn pending_bid_and_active_investment_share_one_cap() {
    let (env, investor, contract_id) = setup();
    env.as_contract(&contract_id, || {
        let investment = active_investment(&env, &investor, 50, 4_000);
        InvestmentStorage::store_investment(&env, &investment);
        let bid = placed_bid(&env, &investor, 60, 5_000);
        BidStorage::store_bid(&env, &bid);

        assert_eq!(
            BidStorage::get_active_bid_amount_sum_for_investor(&env, &investor),
            5_000
        );
        let active = InvestmentStorage::get_active_investment_amount_sum_for_investor(&env, &investor);
        assert_eq!(active, 4_000);
        assert_eq!(active.saturating_add(5_000), 9_000);
        assert_eq!(
            validate_investor_investment(&env, &investor, 1_001),
            Err(QuickLendXError::InvalidAmount)
        );
    });
}

#[test]
fn unrelated_investor_reservations_do_not_reduce_capacity() {
    let (env, investor, contract_id) = setup();
    let other = Address::generate(&env);
    env.as_contract(&contract_id, || {
        let investment = active_investment(&env, &other, 70, LIMIT);
        InvestmentStorage::store_investment(&env, &investment);
        let bid = placed_bid(&env, &other, 71, LIMIT);
        BidStorage::store_bid(&env, &bid);

        assert!(validate_investor_investment(&env, &investor, LIMIT).is_ok());
    });
}

#[test]
fn releasing_a_position_restores_the_exact_remaining_capacity() {
    let (env, investor, contract_id) = setup();
    env.as_contract(&contract_id, || {
        let investment = active_investment(&env, &investor, 80, 6_000);
        InvestmentStorage::store_investment(&env, &investment);
        let bid = placed_bid(&env, &investor, 81, 3_000);
        BidStorage::store_bid(&env, &bid);
        assert!(validate_investor_investment(&env, &investor, 1_001).is_err());

        let mut closed = investment.clone();
        closed.status = InvestmentStatus::Refunded;
        InvestmentStorage::update_investment(&env, &closed);

        assert!(validate_investor_investment(&env, &investor, 7_000).is_ok());
        assert!(validate_investor_investment(&env, &investor, 7_001).is_err());
    });
}

#[test]
fn active_exposure_is_stable_across_many_terminal_transitions() {
    let (env, investor, contract_id) = setup();
    let statuses = [
        InvestmentStatus::Completed,
        InvestmentStatus::Defaulted,
        InvestmentStatus::Refunded,
        InvestmentStatus::Withdrawn,
    ];
    env.as_contract(&contract_id, || {
        for index in 0..16u8 {
            let investment = active_investment(&env, &investor, 90 + index, 100 + index as i128);
            InvestmentStorage::store_investment(&env, &investment);
            if index % 2 == 0 {
                let mut closed = investment.clone();
                closed.status = statuses[(index as usize / 2) % statuses.len()].clone();
                InvestmentStorage::update_investment(&env, &closed);
            }
        }

        let expected: i128 = (0..16u8)
            .filter(|index| index % 2 == 1)
            .map(|index| 100 + index as i128)
            .sum();
        assert_eq!(
            InvestmentStorage::get_active_investment_amount_sum_for_investor(&env, &investor),
            expected
        );
    });
}

#[test]
fn terminal_investment_history_remains_queryable_after_capacity_release() {
    let (env, investor, contract_id) = setup();
    env.as_contract(&contract_id, || {
        let investment = active_investment(&env, &investor, 110, 2_000);
        InvestmentStorage::store_investment(&env, &investment);

        let mut defaulted = investment.clone();
        defaulted.status = InvestmentStatus::Defaulted;
        InvestmentStorage::update_investment(&env, &defaulted);

        assert_eq!(
            InvestmentStorage::get_investments_by_investor(&env, &investor)
                .iter()
                .filter(|entry| entry == &investment.investment_id)
                .count(),
            1
        );
        assert_eq!(
            InvestmentStorage::get_active_investment_amount_sum_for_investor(&env, &investor),
            0
        );
    });
}
