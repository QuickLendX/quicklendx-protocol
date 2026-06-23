//! Differential golden-vector harness for profit and fee math.
//!
//! Loads a frozen corpus from `tests/fixtures/profit_fee_corpus.json` and
//! re-evaluates each vector against the live implementation. Any divergence
//! fails CI and requires an explicit corpus refresh PR (admin-reviewed bless).
//!
//! To regenerate the corpus (intentional semantic change only):
//! ```bash
//! ALLOW_PROFIT_FEE_CORPUS_REFRESH=1 ./scripts/refresh-profit-fee-corpus.sh
//! ```

use quicklendx_contracts::fees::FeeManager;
use quicklendx_contracts::profits::{
    calculate_treasury_split, verify_no_dust, PlatformFee,
};
use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use serde::{Deserialize, Serialize};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};
use std::fs;
use std::path::PathBuf;

const CORPUS_PATH: &str = "tests/fixtures/profit_fee_corpus.json";

/// One frozen input/output pair in the golden corpus.
///
/// Bless semantics: updating expected outputs requires
/// `ALLOW_PROFIT_FEE_CORPUS_REFRESH=1` and a dedicated PR review.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct GoldenVector {
    investment_amount: String,
    payment_amount: String,
    fee_bps: i64,
    tier: String,
    volume: String,
    treasury_share_bps: i64,
    transaction_amount: String,
    is_early_payment: bool,
    is_late_payment: bool,
    investor_return: String,
    platform_fee: String,
    treasury: String,
    dust: String,
    transaction_fee: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GoldenCorpus {
    version: u32,
    vectors: Vec<GoldenVector>,
}

fn parse_i128(s: &str) -> i128 {
    s.parse::<i128>().expect("invalid i128 in corpus")
}

fn format_i128(v: i128) -> String {
    v.to_string()
}

fn tier_volume(tier: &str) -> i128 {
    match tier {
        "Platinum" => 1_000_000_000_000,
        "Gold" => 500_000_000_000,
        "Silver" => 100_000_000_000,
        _ => 0,
    }
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(CORPUS_PATH)
}

fn load_corpus() -> GoldenCorpus {
    let raw = fs::read_to_string(corpus_path()).expect("corpus file must exist");
    serde_json::from_str(&raw).expect("corpus JSON must be valid")
}

fn evaluate_transaction_fee(
    tier: &str,
    transaction_amount: i128,
    is_early_payment: bool,
    is_late_payment: bool,
) -> i128 {
    if transaction_amount <= 0 {
        return 0;
    }

    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _ = client.initialize_admin(&admin);

    env.as_contract(&contract_id, || {
        FeeManager::initialize(&env, &admin).unwrap();
        let volume = tier_volume(tier);
        if volume > 0 {
            FeeManager::update_user_volume(&env, &user, volume).unwrap();
        }
        FeeManager::calculate_total_fees(
            &env,
            &user,
            transaction_amount,
            is_early_payment,
            is_late_payment,
        )
        .unwrap_or(0)
    })
}

fn evaluate_vector(vector: &GoldenVector) -> GoldenVector {
    let investment = parse_i128(&vector.investment_amount);
    let payment = parse_i128(&vector.payment_amount).max(0);
    let fee_bps = vector.fee_bps as i128;
    let treasury_share_bps = vector.treasury_share_bps as i128;
    let transaction_amount = parse_i128(&vector.transaction_amount);

    let (investor_return, platform_fee) =
        PlatformFee::calculate_with_fee_bps(investment, payment, fee_bps);
    let (treasury, _remaining) = calculate_treasury_split(platform_fee, treasury_share_bps);
    let dust = payment
        .saturating_sub(investor_return)
        .saturating_sub(platform_fee);
    let transaction_fee = evaluate_transaction_fee(
        &vector.tier,
        transaction_amount,
        vector.is_early_payment,
        vector.is_late_payment,
    );

    GoldenVector {
        investment_amount: format_i128(investment),
        payment_amount: format_i128(payment),
        fee_bps: vector.fee_bps,
        tier: vector.tier.clone(),
        volume: format_i128(tier_volume(&vector.tier)),
        treasury_share_bps: vector.treasury_share_bps,
        transaction_amount: format_i128(transaction_amount),
        is_early_payment: vector.is_early_payment,
        is_late_payment: vector.is_late_payment,
        investor_return: format_i128(investor_return),
        platform_fee: format_i128(platform_fee),
        treasury: format_i128(treasury),
        dust: format_i128(dust),
        transaction_fee: format_i128(transaction_fee),
    }
}

fn generate_corpus_vectors() -> Vec<GoldenVector> {
    let mut vectors = Vec::new();

    let investments = [
        0i128, 1, 100, 1_000, 10_000, 1_000_000, i128::MAX / 4, i128::MAX / 2,
    ];
    let payment_offsets = [-1i128, 0, 1, 50, 1_000];
    let fee_bps_values = [0i64, 1, 200, 1000];
    let treasury_shares = [0i64, 5_000];
    let tiers = ["Standard", "Silver", "Gold", "Platinum"];
    let transaction_amounts = [0i128, 1_000];
    let timing_flags = [(false, false), (true, false)];

    for &investment in &investments {
        for &offset in &payment_offsets {
            let payment = (if offset < 0 {
                investment.saturating_sub(offset.unsigned_abs() as i128)
            } else {
                investment.saturating_add(offset)
            })
            .max(0);

            for &fee_bps in &fee_bps_values {
                for &treasury_share_bps in &treasury_shares {
                    for tier in tiers {
                        for &transaction_amount in &transaction_amounts {
                            for &(is_early, is_late) in &timing_flags {
                                let seed = GoldenVector {
                                    investment_amount: format_i128(investment),
                                    payment_amount: format_i128(payment),
                                    fee_bps,
                                    tier: tier.to_string(),
                                    volume: format_i128(tier_volume(tier)),
                                    treasury_share_bps,
                                    transaction_amount: format_i128(transaction_amount),
                                    is_early_payment: is_early,
                                    is_late_payment: is_late,
                                    investor_return: "0".into(),
                                    platform_fee: "0".into(),
                                    treasury: "0".into(),
                                    dust: "0".into(),
                                    transaction_fee: "0".into(),
                                };
                                vectors.push(evaluate_vector(&seed));
                            }
                        }
                    }
                }
            }
        }
    }

    // Explicit edge cases called out in the issue.
    for (investment, payment) in [
        (1_000i128, 0i128),
        (1_000i128, 2_000i128),
        (i128::MAX / 2, i128::MAX / 2 + 1),
        (0i128, i128::MAX / 4),
    ] {
        for tier in tiers {
            let seed = GoldenVector {
                investment_amount: format_i128(investment),
                payment_amount: format_i128(payment),
                fee_bps: 200,
                tier: tier.to_string(),
                volume: format_i128(tier_volume(tier)),
                treasury_share_bps: 5_000,
                transaction_amount: "1000".into(),
                is_early_payment: false,
                is_late_payment: false,
                investor_return: "0".into(),
                platform_fee: "0".into(),
                treasury: "0".into(),
                dust: "0".into(),
                transaction_fee: "0".into(),
            };
            vectors.push(evaluate_vector(&seed));
        }
    }

    vectors.sort_by(|a, b| {
        (
            &a.investment_amount,
            &a.payment_amount,
            a.fee_bps,
            &a.tier,
            &a.transaction_amount,
            a.is_early_payment,
            a.is_late_payment,
        )
            .cmp(&(
                &b.investment_amount,
                &b.payment_amount,
                b.fee_bps,
                &b.tier,
                &b.transaction_amount,
                b.is_early_payment,
                b.is_late_payment,
            ))
    });
    vectors.dedup_by(|a, b| {
        a.investment_amount == b.investment_amount
            && a.payment_amount == b.payment_amount
            && a.fee_bps == b.fee_bps
            && a.tier == b.tier
            && a.transaction_amount == b.transaction_amount
            && a.is_early_payment == b.is_early_payment
            && a.is_late_payment == b.is_late_payment
    });

    vectors
}

#[test]
fn profit_fee_golden_vectors_match_live_implementation() {
    let corpus = load_corpus();
    assert!(
        corpus.vectors.len() >= 500,
        "corpus must contain at least 500 vectors, found {}",
        corpus.vectors.len()
    );

    let mut mismatches = Vec::new();
    for (index, expected) in corpus.vectors.iter().enumerate() {
        let actual = evaluate_vector(expected);
        if &actual != expected {
            mismatches.push((index, expected.clone(), actual));
        }

        let investor_return = parse_i128(&expected.investor_return);
        let platform_fee = parse_i128(&expected.platform_fee);
        let payment = parse_i128(&expected.payment_amount);
        assert!(
            verify_no_dust(investor_return, platform_fee, payment.max(0)),
            "verify_no_dust failed for vector {index}"
        );
        assert_eq!(expected.dust, "0", "dust must be zero for vector {index}");
    }

    assert!(
        mismatches.is_empty(),
        "golden corpus diverged from live implementation ({} mismatches). \
         Run ALLOW_PROFIT_FEE_CORPUS_REFRESH=1 ./scripts/refresh-profit-fee-corpus.sh to bless changes.",
        mismatches.len()
    );
}

#[test]
fn profit_fee_golden_contract_calculate_profit_matches_corpus_subset() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let _ = client.initialize_admin(&admin);

    let corpus = load_corpus();
    for vector in corpus.vectors.iter().take(50) {
        client.set_platform_fee(&(vector.fee_bps as i128));
        let investment = parse_i128(&vector.investment_amount);
        let payment = parse_i128(&vector.payment_amount);
        let (investor_return, platform_fee) = client.calculate_profit(&investment, &payment);
        assert_eq!(investor_return, parse_i128(&vector.investor_return));
        assert_eq!(platform_fee, parse_i128(&vector.platform_fee));
    }
}

#[test]
#[ignore = "admin-only corpus refresh; requires ALLOW_PROFIT_FEE_CORPUS_REFRESH=1"]
fn refresh_profit_fee_corpus() {
    if std::env::var("ALLOW_PROFIT_FEE_CORPUS_REFRESH")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        // intentional bless path
    } else {
        panic!(
            "Corpus refresh blocked. Set ALLOW_PROFIT_FEE_CORPUS_REFRESH=1 to regenerate."
        );
    }

    let vectors = generate_corpus_vectors();
    assert!(
        vectors.len() >= 500,
        "generator must produce at least 500 vectors"
    );

    let corpus = GoldenCorpus {
        version: 1,
        vectors,
    };

    let path = corpus_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixtures dir");
    }
    let json = serde_json::to_string_pretty(&corpus).expect("serialize corpus");
    fs::write(&path, json).expect("write corpus");
}
