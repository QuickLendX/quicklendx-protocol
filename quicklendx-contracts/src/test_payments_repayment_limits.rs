//! Integration regression tests for the repayment / profit-distribution
//! resource and rate limits (QE-2026-08).
//!
//! NOTE: This module is currently **not** wired into `lib.rs` and **not**
//! validated against the toolchain (MSVC linker unavailable at authoring time).
//! Before wiring via `#[cfg(test)] mod test_payments_repayment_limits;`, run
//! `cargo test` and fix any API drift. It deliberately targets the crate-internal
//! `payments::repay_escrow`/`allocate_repayment` entry points (via
//! `env.as_contract`), NOT the nonexistent generated `try_repay_escrow` client
//! method that blocks the legacy `test_payments_repayment.rs` from compiling.
//!
//! The repayment engine in `payments.rs` (`repay_escrow` + `allocate_repayment`)
//! splits a custodied repayment deterministically among principal release to the
//! business, investor return, and platform/late fees. This suite proves, at the
//! crate integration boundary (real `Env`, real token contracts, real escrow
//! lifecycle):
//!
//! - **Bounded input before expensive work**: out-of-range `late_fee_bps` and
//!   `payment_amount` are rejected up front with actionable errors and leave no
//!   exportable state change (no partial token movement).
//! - **Rate limits**: repeated repayment attempts by one repaying account are
//!   throttled with `MutationLimitExceeded`; the budget is per-account and
//!   recovers after the window elapses.
//! - **Failure safety**: rejected, stale, and repeated operations leave balances
//!   and escrow state unchanged.
//! - **Determinism**: the allocation always satisfies the accounting identity
//!   `investor_return + platform_fee + late_fee == payment`.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::escrow::EscrowStatus;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::{
    allocate_repayment, repay_escrow, RepaymentRateLimiter, MAX_REPAYMENTS_PER_WINDOW,
    REPAYMENT_RATE_LIMIT_WINDOW_SECS,
};
use crate::QuickLendXContract;
use crate::QuickLendXContractClient;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, BytesN, Env, String, Vec};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);
    (env, client, admin)
}

/// Register a token and mint + approve `business`/`investor` with `initial_balance`.
fn setup_token(
    env: &Env,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let initial_balance = 1_000_000i128;
    sac_client.mint(business, &initial_balance);
    sac_client.mint(investor, &initial_balance);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(business, contract_id, &initial_balance, &expiration);
    token_client.approve(investor, contract_id, &initial_balance, &expiration);
    currency
}

fn setup_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn setup_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

/// Store + verify a `Funded`-ready invoice for `business`.
fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        business,
        &amount,
        currency,
        &due_date,
        &String::from_str(env, "Test Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

/// Fund `invoice_id` for `business` with a fresh investor, leaving the escrow `Held`.
fn fund_single_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    invoice_id: &BytesN<32>,
    business: &Address,
    amount: i128,
    currency: &Address,
    investor_limit: i128,
) -> Address {
    let investor = setup_verified_investor(env, client, investor_limit);
    let bid_id = client.place_bid(
        &investor,
        invoice_id,
        &amount,
        &(amount + 1000),
        &BytesN::from_array(env, &[0u8; 32]),
    );
    client.accept_bid(invoice_id, &bid_id);
    assert_eq!(
        client.get_invoice(invoice_id).status,
        InvoiceStatus::Funded
    );
    assert_eq!(
        client.get_escrow_status(invoice_id),
        EscrowStatus::Held
    );
    investor
}

/// Custody `amount` of `currency` inside the contract (simulates the business
/// repayment being received before `repay_escrow` distributes it).
fn custody_in_contract(env: &Env, currency: &Address, contract_id: &Address, amount: i128) {
    let sac_client = token::StellarAssetClient::new(env, currency);
    sac_client.mint(contract_id, &amount);
}

fn contract_id_of(client: &QuickLendXContractClient) -> Address {
    client.address.clone()
}

// ===========================================================================
// Bounded input (adversarial sizes) rejected before any work
// ===========================================================================

#[test]
fn test_repay_rejects_negative_late_fee_bps_no_partial_state() {
    let (env, client, admin) = setup();
    let contract_id = contract_id_of(&client);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 1_000_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000, &currency);
    fund_single_invoice(&env, &client, &admin, &invoice_id, &business, 10_000, &currency, 1_000_000);
    custody_in_contract(&env, &currency, &contract_id, 11_000);

    let token_client = token::Client::new(&env, &currency);
    let investor_before = token_client.balance(&investor);
    let business_before = token_client.balance(&business);
    let contract_before = token_client.balance(&contract_id);

    // Negative BPS is rejected with an actionable error before any transfer.
    let result = env.as_contract(&contract_id, || {
        repay_escrow(&env, &invoice_id, 11_000, -1)
    });
    assert_eq!(result, Err(QuickLendXError::InvalidFeeBasisPoints));

    // No funds moved, escrow still Held.
    assert_eq!(token_client.balance(&investor), investor_before);
    assert_eq!(token_client.balance(&business), business_before);
    assert_eq!(token_client.balance(&contract_id), contract_before);
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

#[test]
fn test_repay_rejects_oversized_late_fee_bps_no_partial_state() {
    let (env, client, admin) = setup();
    let contract_id = contract_id_of(&client);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 1_000_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000, &currency);
    fund_single_invoice(&env, &client, &admin, &invoice_id, &business, 10_000, &currency, 1_000_000);
    custody_in_contract(&env, &currency, &contract_id, 11_000);

    let token_client = token::Client::new(&env, &currency);
    let contract_before = token_client.balance(&contract_id);

    // BPS beyond the denominator no longer silently clamps at the boundary;
    // it is rejected as an out-of-range fee parameter.
    let result = env.as_contract(&contract_id, || {
        repay_escrow(&env, &invoice_id, 11_000, crate::profits::BPS_DENOMINATOR + 1)
    });
    assert_eq!(result, Err(QuickLendXError::InvalidFeeBasisPoints));
    assert_eq!(token_client.balance(&contract_id), contract_before);
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

#[test]
fn test_repay_rejects_oversized_payment_no_partial_state() {
    let (env, client, admin) = setup();
    let contract_id = contract_id_of(&client);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 1_000_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000, &currency);
    fund_single_invoice(&env, &client, &admin, &invoice_id, &business, 10_000, &currency, 1_000_000);

    let token_client = token::Client::new(&env, &currency);
    let contract_before = token_client.balance(&contract_id);

    let oversized = crate::protocol_limits::MAX_INVOICE_AMOUNT + 1;
    let result = env.as_contract(&contract_id, || {
        repay_escrow(&env, &invoice_id, oversized, 0)
    });
    assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    assert_eq!(token_client.balance(&contract_id), contract_before);
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

#[test]
fn test_repay_rejects_negative_payment_no_partial_state() {
    let (env, client, admin) = setup();
    let contract_id = contract_id_of(&client);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 1_000_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000, &currency);
    fund_single_invoice(&env, &client, &admin, &invoice_id, &business, 10_000, &currency, 1_000_000);

    let result = env.as_contract(&contract_id, || {
        repay_escrow(&env, &invoice_id, -100, 0)
    });
    assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

// ===========================================================================
// Failure safety: rejected / stale / repeated operations leave no partial state
// ===========================================================================

#[test]
fn test_repay_insufficient_custody_leaves_no_partial_state() {
    let (env, client, admin) = setup();
    let contract_id = contract_id_of(&client);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 1_000_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000, &currency);
    fund_single_invoice(&env, &client, &admin, &invoice_id, &business, 10_000, &currency, 1_000_000);

    let token_client = token::Client::new(&env, &currency);
    let investor_before = token_client.balance(&investor);
    let business_before = token_client.balance(&business);
    let contract_before = token_client.balance(&contract_id);

    // Only partial custody (short by 1) -> fail with no movement.
    custody_in_contract(&env, &currency, &contract_id, 10_999);

    let result = env.as_contract(&contract_id, || {
        repay_escrow(&env, &invoice_id, 11_000, 0)
    });
    assert_eq!(result, Err(QuickLendXError::InsufficientFunds));
    assert_eq!(token_client.balance(&investor), investor_before);
    assert_eq!(token_client.balance(&business), business_before);
    assert_eq!(token_client.balance(&contract_id), contract_before + 10_999);
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

#[test]
fn test_repay_overcharge_rejected_escrow_stays_held() {
    let (env, client, admin) = setup();
    let contract_id = contract_id_of(&client);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 1_000_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000, &currency);
    fund_single_invoice(&env, &client, &admin, &invoice_id, &business, 10_000, &currency, 1_000_000);

    // payment 10 cannot cover a 100% late fee on the 10000 principal.
    custody_in_contract(&env, &currency, &contract_id, 10);
    let result = env.as_contract(&contract_id, || {
        repay_escrow(&env, &invoice_id, 10, 10_000)
    });
    assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

#[test]
fn test_repay_replay_after_success_is_rejected_without_double_payout() {
    let (env, client, admin) = setup();
    let contract_id = contract_id_of(&client);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 1_000_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);
    let invoice_id = create_verified_invoice(&env, &client, &business, 10_000, &currency);
    fund_single_invoice(&env, &client, &admin, &invoice_id, &business, 10_000, &currency, 1_000_000);
    custody_in_contract(&env, &currency, &contract_id, 11_000);

    let token_client = token::Client::new(&env, &currency);
    let investor_before = token_client.balance(&investor);
    let business_before = token_client.balance(&business);

    let first = env.as_contract(&contract_id, || {
        repay_escrow(&env, &invoice_id, 11_000, 0)
    });
    assert!(first.is_ok());
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Released);

    // Replay on a released escrow is rejected and moves nothing further.
    let investor_after_first = token_client.balance(&investor);
    let business_after_first = token_client.balance(&business);
    let replay = env.as_contract(&contract_id, || {
        repay_escrow(&env, &invoice_id, 11_000, 0)
    });
    assert_eq!(replay, Err(QuickLendXError::InvalidStatus));
    assert_eq!(token_client.balance(&investor), investor_after_first);
    assert_eq!(token_client.balance(&business), business_after_first);
    // The first call did release the principal to the business.
    assert!(business_after_first > business_before);
    assert!(investor_after_first > investor_before);
}

// ===========================================================================
// Determinism of the allocation accounting identity
// ===========================================================================

#[test]
fn test_repay_allocation_accounts_exactly() {
    for (principal, payment) in [
        (10_000, 11_000),
        (10_000, 10_000),
        (10_000, 9_000),
        (1_000, 10_000),
        (10_000, 20_000),
    ] {
        let allocation = allocate_repayment(principal, payment, 200, 0, 10_000).unwrap();
        assert_eq!(
            allocation.investor_return + allocation.platform_fee + allocation.late_fee,
            payment,
            "conservation identity must hold for principal={} payment={}",
            principal,
            payment
        );
        assert!(allocation.investor_return >= 0);
        assert!(allocation.platform_fee >= 0);
        assert!(allocation.late_fee >= 0);
        assert!(allocation.treasury_amount >= 0);
        assert!(allocation.treasury_remaining >= 0);
        assert_eq!(
            allocation.treasury_amount + allocation.treasury_remaining,
            allocation.platform_fee + allocation.late_fee
        );
    }
}

// ===========================================================================
// Rate limits: burst rejection, per-account independence, window recovery
// ===========================================================================

/// Create `n` funded `Held`-escrow invoices for the *same* business so the
/// repayment rate limiter (keyed per repaying business) can be exercised.
fn fund_n_invoices_for_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    currency: &Address,
    n: u32,
) -> Vec<BytesN<32>> {
    let mut ids = Vec::new(env);
    for i in 0..n {
        let amount = 10_000 + i as i128 * 1_000;
        // A fresh investor for each invoice keeps the funder independent.
        let investor = setup_verified_investor(env, client, 1_000_000);
        let invoice_id = create_verified_invoice(env, client, business, amount, currency);
        let bid_id = client.place_bid(
            &investor,
            &invoice_id,
            &amount,
            &(amount + 1000),
            &BytesN::from_array(env, &[(i + 1) as u8; 32]),
        );
        client.accept_bid(&invoice_id, &bid_id);
        assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
        ids.push_back(invoice_id);
    }
    ids
}

#[test]
fn test_repay_burst_is_throttled_per_business() {
    let (env, client, admin) = setup();
    let contract_id = contract_id_of(&client);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 1_000_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);

    // Repay budget is MAX_REPAYMENTS_PER_WINDOW per account; use one invoice
    // per budget slot so the repayment succeeds each time.
    let ids = fund_n_invoices_for_business(&env, &client, &admin, &business, &currency, MAX_REPAYMENTS_PER_WINDOW + 1);

    // Custody each repayment into the contract.
    let mut full_payments: alloc::vec::Vec<i128> = alloc::vec::Vec::new();
    for i in 0..ids.len() {
        let principal = 10_000 + i as i128 * 1_000;
        let payment = principal + 500; // a little profit
        custody_in_contract(&env, &currency, &contract_id, payment);
        full_payments.push(payment);
    }

    // First MAX_REPAYMENTS_PER_WINDOW repayments succeed.
    for i in 0..MAX_REPAYMENTS_PER_WINDOW {
        let id = ids.get(i).unwrap();
        let result = env.as_contract(&contract_id, || {
            repay_escrow(&env, &id, full_payments[i as usize], 0)
        });
        assert!(result.is_ok(), "repayment {i} must succeed within budget");
    }

    // The next repayment from the same business is throttled.
    let overflow_id = ids.get(MAX_REPAYMENTS_PER_WINDOW).unwrap();
    let throttled = env.as_contract(&contract_id, || {
        repay_escrow(&env, &overflow_id, full_payments[MAX_REPAYMENTS_PER_WINDOW as usize], 0)
    });
    assert_eq!(throttled, Err(QuickLendXError::MutationLimitExceeded));
    // The throttled escrow remains Held (no partial distribution).
    assert_eq!(
        client.get_escrow_status(overflow_id),
        EscrowStatus::Held
    );
}

#[test]
fn test_repay_rate_limit_is_per_account() {
    let (env, client, admin) = setup();
    let contract_id = contract_id_of(&client);
    let business_a = setup_verified_business(&env, &client, &admin);
    let business_b = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 1_000_000);
    let currency = setup_token(&env, &business_a, &investor, &contract_id);

    // Exhaust business A's budget.
    let ids_a = fund_n_invoices_for_business(&env, &client, &admin, &business_a, &currency, MAX_REPAYMENTS_PER_WINDOW);
    for i in 0..ids_a.len() {
        let principal = 10_000 + i as i128 * 1_000;
        custody_in_contract(&env, &currency, &contract_id, principal + 500);
        let id = ids_a.get(i).unwrap();
        let result = env.as_contract(&contract_id, || {
            repay_escrow(&env, &id, principal + 500, 0)
        });
        assert!(result.is_ok());
    }

    // Business B is unaffected: fund and repay a fresh invoice.
    let investor_b = setup_verified_investor(&env, &client, 1_000_000);
    let invoice_b = create_verified_invoice(&env, &client, &business_b, 10_000, &currency);
    let bid_b = client.place_bid(
        &investor_b,
        &invoice_b,
        &10_000,
        &11_000,
        &BytesN::from_array(&env, &[0xB0u8; 32]),
    );
    client.accept_bid(&invoice_b, &bid_b);
    custody_in_contract(&env, &currency, &contract_id, 11_000);
    let result_b = env.as_contract(&contract_id, || {
        repay_escrow(&env, &invoice_b, 11_000, 0)
    });
    assert!(result_b.is_ok(), "business B must not be throttled by business A");
}

#[test]
fn test_repay_rate_limit_recovers_after_window() {
    let (env, client, admin) = setup();
    let contract_id = contract_id_of(&client);
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 1_000_000);
    let currency = setup_token(&env, &business, &investor, &contract_id);
    env.ledger().set_timestamp(1_000_000);

    let ids = fund_n_invoices_for_business(&env, &client, &admin, &business, &currency, MAX_REPAYMENTS_PER_WINDOW + 1);
    let mut full_payments: alloc::vec::Vec<i128> = alloc::vec::Vec::new();
    for i in 0..ids.len() {
        let principal = 10_000 + i as i128 * 1_000;
        custody_in_contract(&env, &currency, &contract_id, principal + 500);
        full_payments.push(principal + 500);
    }

    // Exhaust the budget.
    for i in 0..MAX_REPAYMENTS_PER_WINDOW {
        let id = ids.get(i).unwrap();
        let result = env.as_contract(&contract_id, || {
            repay_escrow(&env, &id, full_payments[i as usize], 0)
        });
        assert!(result.is_ok());
    }
    // Throttled.
    let overflow_id = ids.get(MAX_REPAYMENTS_PER_WINDOW).unwrap();
    let throttled = env.as_contract(&contract_id, || {
        repay_escrow(&env, overflow_id, full_payments[MAX_REPAYMENTS_PER_WINDOW as usize], 0)
    });
    assert_eq!(throttled, Err(QuickLendXError::MutationLimitExceeded));

    // Advance past the window -> budget resets, repayment succeeds.
    env.ledger().set_timestamp(1_000_000 + REPAYMENT_RATE_LIMIT_WINDOW_SECS + 1);
    let recovered = env.as_contract(&contract_id, || {
        repay_escrow(&env, overflow_id, full_payments[MAX_REPAYMENTS_PER_WINDOW as usize], 0)
    });
    assert!(recovered.is_ok(), "business must recover after window expiry");
    assert_eq!(
        client.get_escrow_status(overflow_id),
        EscrowStatus::Released
    );
}

#[test]
fn test_repay_rate_limiter_burst_and_per_address() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);
    env.ledger().set_timestamp(1_000);

    // Fill addr_a's budget; each records a repayment.
    for i in 0..MAX_REPAYMENTS_PER_WINDOW {
        env.as_contract(&contract_id, || {
            RepaymentRateLimiter::check_and_record(&env, &addr_a).unwrap();
            assert_eq!(
                RepaymentRateLimiter::get_rate_limit(&env, &addr_a).count,
                i + 1
            );
        });
    }

    // addr_a is now throttled.
    env.as_contract(&contract_id, || {
        assert_eq!(
            RepaymentRateLimiter::check_and_record(&env, &addr_a),
            Err(QuickLendXError::MutationLimitExceeded)
        );
    });

    // addr_b is independent: its budget is untouched.
    assert_eq!(
        env.as_contract(&contract_id, || {
            RepaymentRateLimiter::get_rate_limit(&env, &addr_b).count
        }),
        0
    );
}

#[test]
fn test_repay_rate_limiter_record_counter_readable() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let addr = Address::generate(&env);
    env.ledger().set_timestamp(5_000);

    for i in 0..MAX_REPAYMENTS_PER_WINDOW {
        env.as_contract(&contract_id, || {
            RepaymentRateLimiter::check_and_record(&env, &addr).unwrap();
            assert_eq!(RepaymentRateLimiter::get_rate_limit(&env, &addr).count, i + 1);
        });
    }
    assert_eq!(
        env.as_contract(&contract_id, || {
            RepaymentRateLimiter::check_and_record(&env, &addr)
        }),
        Err(QuickLendXError::MutationLimitExceeded)
    );
}
