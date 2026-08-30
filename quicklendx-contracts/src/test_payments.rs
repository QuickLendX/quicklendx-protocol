//! Direct unit tests for the payments module.
//!
//! These tests verify token transfer prechecks and escrow operations in isolation,
//! ensuring insufficient balance and allowance are rejected before any token call,
//! and no partial state updates persist on failure.

use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env};

use crate::errors::QuickLendXError;
use crate::payments::{
    create_escrow, refund_escrow, release_escrow, transfer_funds, Escrow, EscrowStatus,
    EscrowStorage,
};
use crate::QuickLendXContract;

// ============================================================================
// Helpers
// ============================================================================

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

/// Register a SAC token, mint to addresses, and optionally approve the contract.
fn setup_token(
    env: &Env,
    contract_id: &Address,
    mint_to: &[(Address, i128)],
    approve: &[(Address, i128)],
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let token_client = token::Client::new(env, &currency);

    for (addr, amount) in mint_to {
        sac_client.mint(addr, amount);
    }

    let expiration = env.ledger().sequence() + 10_000;
    for (addr, amount) in approve {
        token_client.approve(addr, contract_id, amount, &expiration);
    }

    currency
}

// ============================================================================
// transfer_funds - negative tests
// ============================================================================

#[test]
fn test_transfer_funds_zero_amount() {
    let (env, contract_id) = setup();
    let currency = setup_token(&env, &contract_id, &[], &[]);
    let from = Address::generate(&env);
    let to = Address::generate(&env);

    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &from, &to, 0)
    });

    assert_eq!(result, Err(QuickLendXError::InvalidAmount));
}

#[test]
fn test_transfer_funds_same_address_fails() {
    let (env, contract_id) = setup();
    let currency = setup_token(&env, &contract_id, &[], &[]);
    let addr = Address::generate(&env);

    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &addr, &addr, 1_000)
    });

    assert_eq!(result, Err(QuickLendXError::SelfTransfer));
}

#[test]
fn test_transfer_funds_insufficient_balance() {
    let (env, contract_id) = setup();
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let currency = setup_token(
        &env,
        &contract_id,
        &[(from.clone(), 500)],
        &[(from.clone(), 1_000)],
    );
    let token_client = token::Client::new(&env, &currency);

    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &from, &to, 1_000)
    });

    assert_eq!(result, Err(QuickLendXError::InsufficientFunds));
    assert_eq!(token_client.balance(&from), 500);
    assert_eq!(token_client.balance(&to), 0);
}

#[test]
fn test_transfer_funds_zero_allowance() {
    let (env, contract_id) = setup();
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let currency = setup_token(
        &env,
        &contract_id,
        &[(from.clone(), 10_000)],
        &[], // no allowances
    );
    let token_client = token::Client::new(&env, &currency);

    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &from, &to, 1_000)
    });

    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    assert_eq!(token_client.balance(&from), 10_000);
    assert_eq!(token_client.balance(&to), 0);
}

#[test]
fn test_transfer_funds_partial_allowance() {
    let (env, contract_id) = setup();
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let currency = setup_token(
        &env,
        &contract_id,
        &[(from.clone(), 10_000)],
        &[(from.clone(), 400)], // partial allowance
    );
    let token_client = token::Client::new(&env, &currency);

    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &from, &to, 1_000)
    });

    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    assert_eq!(token_client.balance(&from), 10_000);
    assert_eq!(token_client.balance(&to), 0);
}

#[test]
fn test_transfer_funds_contract_sender_insufficient_balance() {
    let (env, contract_id) = setup();
    let to = Address::generate(&env);
    let currency = setup_token(&env, &contract_id, &[], &[]);
    let token_client = token::Client::new(&env, &currency);

    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &contract_id, &to, 1_000)
    });

    assert_eq!(result, Err(QuickLendXError::InsufficientFunds));
    assert_eq!(token_client.balance(&contract_id), 0);
    assert_eq!(token_client.balance(&to), 0);
}

// ============================================================================
// transfer_funds - positive tests
// ============================================================================

#[test]
fn test_transfer_funds_contract_sender_success() {
    let (env, contract_id) = setup();
    let to = Address::generate(&env);
    let currency = setup_token(&env, &contract_id, &[(contract_id.clone(), 5_000)], &[]);
    let token_client = token::Client::new(&env, &currency);

    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &contract_id, &to, 3_000)
    });

    assert_eq!(result, Ok(()));
    assert_eq!(token_client.balance(&contract_id), 2_000);
    assert_eq!(token_client.balance(&to), 3_000);
}

#[test]
fn test_transfer_funds_investor_success() {
    let (env, contract_id) = setup();
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let currency = setup_token(
        &env,
        &contract_id,
        &[(from.clone(), 5_000)],
        &[(from.clone(), 5_000)],
    );
    let token_client = token::Client::new(&env, &currency);

    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &from, &to, 3_000)
    });

    assert_eq!(result, Ok(()));
    assert_eq!(token_client.balance(&from), 2_000);
    assert_eq!(token_client.balance(&to), 3_000);
}

// ============================================================================
// create_escrow - negative tests
// ============================================================================

#[test]
fn test_create_escrow_invalid_amount() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    let currency = setup_token(&env, &contract_id, &[], &[]);

    let result = env.as_contract(&contract_id, || {
        create_escrow(&env, &invoice_id, &investor, &business, 0, &currency)
    });

    assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    assert!(env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
    }));
}

#[test]
fn test_create_escrow_insufficient_balance_no_state_change() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[2u8; 32]);
    let currency = setup_token(
        &env,
        &contract_id,
        &[(investor.clone(), 500)], // insufficient balance
        &[(investor.clone(), 10_000)],
    );

    let counter_before: u64 = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("esc_cnt"))
            .unwrap_or(0)
    });

    let result = env.as_contract(&contract_id, || {
        create_escrow(&env, &invoice_id, &investor, &business, 1_000, &currency)
    });

    assert_eq!(result, Err(QuickLendXError::InsufficientFunds));
    assert!(env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
    }));

    let counter_after: u64 = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("esc_cnt"))
            .unwrap_or(0)
    });
    assert_eq!(counter_after, counter_before);
}

#[test]
fn test_create_escrow_insufficient_allowance_no_state_change() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[3u8; 32]);
    let currency = setup_token(
        &env,
        &contract_id,
        &[(investor.clone(), 10_000)],
        &[(investor.clone(), 500)], // insufficient allowance
    );

    let counter_before: u64 = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("esc_cnt"))
            .unwrap_or(0)
    });

    let result = env.as_contract(&contract_id, || {
        create_escrow(&env, &invoice_id, &investor, &business, 1_000, &currency)
    });

    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    assert!(env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
    }));

    let counter_after: u64 = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("esc_cnt"))
            .unwrap_or(0)
    });
    assert_eq!(counter_after, counter_before);
}

// ============================================================================
// create_escrow - boundary tests: zero, max, overflow, invalid token
// ============================================================================

#[test]
fn test_create_escrow_zero_amount_returns_invalid_amount() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[0x10; 32]);
    let currency = setup_token(&env, &contract_id, &[], &[]);

    let result = env.as_contract(&contract_id, || {
        create_escrow(&env, &invoice_id, &investor, &business, 0, &currency)
    });
    assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    assert!(env.as_contract(&contract_id, || {
        crate::payments::EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
    }));
}

#[test]
fn test_create_escrow_negative_amount_returns_invalid_amount() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[0x11; 32]);
    let currency = setup_token(&env, &contract_id, &[], &[]);

    let result = env.as_contract(&contract_id, || {
        create_escrow(&env, &invoice_id, &investor, &business, -1, &currency)
    });
    assert_eq!(result, Err(QuickLendXError::InvalidAmount));
    assert!(env.as_contract(&contract_id, || {
        crate::payments::EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
    }));
}

#[test]
fn test_create_escrow_max_amount_with_zero_balance_returns_insufficient_funds() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&investor, &0);
    let expiry = env.ledger().sequence() + 10_000;
    tok.approve(&investor, &contract_id, &i128::MAX, &expiry);

    let invoice_id = BytesN::from_array(&env, &[0x12; 32]);

    let result = env.as_contract(&contract_id, || {
        create_escrow(
            &env,
            &invoice_id,
            &investor,
            &Address::generate(&env),
            crate::protocol_limits::MAX_INVOICE_AMOUNT,
            &currency,
        )
    });
    assert_eq!(result, Err(QuickLendXError::InsufficientFunds));
    assert_eq!(tok.balance(&contract_id), 0);
    assert!(env.as_contract(&contract_id, || {
        crate::payments::EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
    }));
}

#[test]
fn test_create_escrow_max_amount_with_sufficient_balance_succeeds() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    sac.mint(&investor, &crate::protocol_limits::MAX_INVOICE_AMOUNT);
    let expiry = env.ledger().sequence() + 10_000;
    tok.approve(
        &investor,
        &contract_id,
        &crate::protocol_limits::MAX_INVOICE_AMOUNT,
        &expiry,
    );

    let invoice_id = BytesN::from_array(&env, &[0x13; 32]);

    let result = env.as_contract(&contract_id, || {
        create_escrow(
            &env,
            &invoice_id,
            &investor,
            &Address::generate(&env),
            crate::protocol_limits::MAX_INVOICE_AMOUNT,
            &currency,
        )
    });
    assert!(
        result.is_ok(),
        "max-amount escrow must succeed with sufficient balance"
    );
    assert_eq!(tok.balance(&investor), 0);
    assert_eq!(
        tok.balance(&contract_id),
        crate::protocol_limits::MAX_INVOICE_AMOUNT
    );

    let escrow = env.as_contract(&contract_id, || {
        crate::payments::EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).unwrap()
    });
    assert_eq!(escrow.amount, crate::protocol_limits::MAX_INVOICE_AMOUNT);
    assert_eq!(escrow.status, crate::payments::EscrowStatus::Held);
}

/// Passing an address that is *not* a registered token contract causes a
/// host-level panic (soroban-sdk 25.x behaviour). The operation must not
/// silently succeed and must leave escrow storage untouched.
#[test]
#[ignore = "pre-existing: panics in newer Soroban env with Abort"]
fn test_create_escrow_unregistered_token_address_does_not_write_escrow() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[0x14; 32]);

    let real_token_admin = Address::generate(&env);
    let real_currency = env
        .register_stellar_asset_contract_v2(real_token_admin.clone())
        .address();
    let real_sac = token::StellarAssetClient::new(&env, &real_currency);
    let real_tok = token::Client::new(&env, &real_currency);
    real_sac.mint(&investor, &10_000);
    let expiry = env.ledger().sequence() + 10_000;
    real_tok.approve(&investor, &contract_id, &10_000, &expiry);

    let bogus_currency = Address::generate(&env);
    let investor_bal = real_tok.balance(&investor);
    let contract_bal = real_tok.balance(&contract_id);

    let result = env.as_contract(&contract_id, || {
        create_escrow(
            &env,
            &invoice_id,
            &investor,
            &business,
            10_000,
            &bogus_currency,
        )
    });

    assert!(
        result.is_err(),
        "unregistered token address must not succeed"
    );
    assert_eq!(real_tok.balance(&investor), investor_bal);
    assert_eq!(real_tok.balance(&contract_id), contract_bal);
    assert!(env.as_contract(&contract_id, || {
        crate::payments::EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_none()
    }));
}

// ============================================================================
// create_escrow - positive happy-path test
// ============================================================================

#[test]
fn test_create_escrow_success() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[4u8; 32]);
    let currency = setup_token(
        &env,
        &contract_id,
        &[(investor.clone(), 10_000)],
        &[(investor.clone(), 10_000)],
    );
    let token_client = token::Client::new(&env, &currency);

    let investor_before = token_client.balance(&investor);
    let contract_before = token_client.balance(&contract_id);

    let escrow_id = env.as_contract(&contract_id, || {
        create_escrow(&env, &invoice_id, &investor, &business, 5_000, &currency).unwrap()
    });

    // Funds moved
    assert_eq!(token_client.balance(&investor), investor_before - 5_000);
    assert_eq!(token_client.balance(&contract_id), contract_before + 5_000);

    // Escrow stored
    let escrow = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow(&env, &escrow_id).unwrap()
    });
    assert_eq!(escrow.invoice_id, invoice_id);
    assert_eq!(escrow.investor, investor);
    assert_eq!(escrow.business, business);
    assert_eq!(escrow.amount, 5_000);
    assert_eq!(escrow.status, EscrowStatus::Held);
}

// ============================================================================
// release_escrow - negative and positive tests
// ============================================================================

#[test]
fn test_release_escrow_insufficient_contract_balance_state_unchanged() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[5u8; 32]);
    let escrow_id = BytesN::from_array(&env, &[6u8; 32]);
    let currency = setup_token(&env, &contract_id, &[], &[]);

    env.as_contract(&contract_id, || {
        let escrow = Escrow {
            escrow_id: escrow_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            business: business.clone(),
            amount: 5_000,
            currency: currency.clone(),
            created_at: env.ledger().timestamp(),
            status: EscrowStatus::Held,
        };
        EscrowStorage::store_escrow(&env, &escrow);
    });

    let result = env.as_contract(&contract_id, || release_escrow(&env, &invoice_id));

    assert_eq!(result, Err(QuickLendXError::InsufficientFunds));

    let stored = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).unwrap()
    });
    assert_eq!(stored.status, EscrowStatus::Held);
}

#[test]
fn test_release_escrow_success() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[7u8; 32]);
    let currency = setup_token(
        &env,
        &contract_id,
        &[(investor.clone(), 10_000)],
        &[(investor.clone(), 10_000)],
    );
    let token_client = token::Client::new(&env, &currency);

    // Create escrow first
    env.as_contract(&contract_id, || {
        create_escrow(&env, &invoice_id, &investor, &business, 5_000, &currency).unwrap()
    });

    let contract_before = token_client.balance(&contract_id);
    let business_before = token_client.balance(&business);

    let result = env.as_contract(&contract_id, || release_escrow(&env, &invoice_id));

    assert_eq!(result, Ok(()));

    // Funds moved to business
    assert_eq!(token_client.balance(&contract_id), contract_before - 5_000);
    assert_eq!(token_client.balance(&business), business_before + 5_000);

    // Status updated
    let stored = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).unwrap()
    });
    assert_eq!(stored.status, EscrowStatus::Released);
}

// ============================================================================
// refund_escrow - negative and positive tests
// ============================================================================

#[test]
fn test_refund_escrow_insufficient_contract_balance_state_unchanged() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[8u8; 32]);
    let escrow_id = BytesN::from_array(&env, &[9u8; 32]);
    let currency = setup_token(&env, &contract_id, &[], &[]);

    env.as_contract(&contract_id, || {
        let escrow = Escrow {
            escrow_id: escrow_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            business: business.clone(),
            amount: 5_000,
            currency: currency.clone(),
            created_at: env.ledger().timestamp(),
            status: EscrowStatus::Held,
        };
        EscrowStorage::store_escrow(&env, &escrow);
    });

    let result = env.as_contract(&contract_id, || refund_escrow(&env, &invoice_id));

    assert_eq!(result, Err(QuickLendXError::InsufficientFunds));

    let stored = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).unwrap()
    });
    assert_eq!(stored.status, EscrowStatus::Held);
}

#[test]
fn test_refund_escrow_success() {
    let (env, contract_id) = setup();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[10u8; 32]);
    let currency = setup_token(
        &env,
        &contract_id,
        &[(investor.clone(), 10_000)],
        &[(investor.clone(), 10_000)],
    );
    let token_client = token::Client::new(&env, &currency);

    // Create escrow first
    env.as_contract(&contract_id, || {
        create_escrow(&env, &invoice_id, &investor, &business, 5_000, &currency).unwrap()
    });

    let contract_before = token_client.balance(&contract_id);
    let investor_before = token_client.balance(&investor);

    let result = env.as_contract(&contract_id, || refund_escrow(&env, &invoice_id));

    assert_eq!(result, Ok(()));

    // Funds refunded to investor
    assert_eq!(token_client.balance(&contract_id), contract_before - 5_000);
    assert_eq!(token_client.balance(&investor), investor_before + 5_000);

    // Status updated
    let stored = env.as_contract(&contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).unwrap()
    });
    assert_eq!(stored.status, EscrowStatus::Refunded);
}

// ============================================================================
// Investor Exposure, Available Capacity & Funding Commitments
// ============================================================================

fn setup_verified_investor_record(
    env: &Env,
    contract_id: &Address,
    investor: &Address,
    limit: i128,
) {
    use crate::verification::{
        BusinessVerificationStatus, InvestorRiskLevel, InvestorTier, InvestorVerification,
        InvestorVerificationStorage,
    };
    use soroban_sdk::String;

    env.as_contract(contract_id, || {
        let record = InvestorVerification {
            investor: investor.clone(),
            status: BusinessVerificationStatus::Verified,
            verified_at: Some(env.ledger().timestamp()),
            verified_by: None,
            kyc_data: String::from_str(env, "kyc-data"),
            investment_limit: limit,
            submitted_at: env.ledger().timestamp(),
            tier: InvestorTier::Basic,
            risk_level: InvestorRiskLevel::Low,
            risk_score: 0,
            total_invested: 0,
            total_returns: 0,
            successful_investments: 0,
            defaulted_investments: 0,
            last_activity: env.ledger().timestamp(),
            rejection_reason: None,
            compliance_notes: None,
        };
        InvestorVerificationStorage::store(env, &record);
    });
}

#[test]
fn test_investor_available_capacity_and_commitments() {
    use crate::payments::{
        get_investor_available_capacity, get_investor_exposure, validate_funding_commitment,
    };
    use crate::storage::{BidStorage, InvestmentStorage};
    use crate::types::{Bid, BidStatus, Investment, InvestmentStatus};
    use soroban_sdk::testutils::Ledger as _;

    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let investor = Address::generate(&env);
    let limit = 50_000i128;

    setup_verified_investor_record(&env, &contract_id, &investor, limit);

    // Initial state: 0 exposure, full available capacity
    let initial_exp = env.as_contract(&contract_id, || {
        get_investor_exposure(&env, &investor).unwrap()
    });
    assert_eq!(initial_exp, 0);

    let initial_cap = env.as_contract(&contract_id, || {
        get_investor_available_capacity(&env, &investor).unwrap()
    });
    assert_eq!(initial_cap, limit);

    // Valid commitment within capacity
    let valid_commit = env.as_contract(&contract_id, || {
        validate_funding_commitment(&env, &investor, 20_000)
    });
    assert_eq!(valid_commit, Ok(()));

    // Commitment exceeding capacity is rejected
    let excess_commit = env.as_contract(&contract_id, || {
        validate_funding_commitment(&env, &investor, limit + 1)
    });
    assert_eq!(excess_commit, Err(QuickLendXError::InvalidAmount));

    // Place an active bid
    let bid_id = BytesN::from_array(&env, &[101u8; 32]);
    let invoice_id = BytesN::from_array(&env, &[102u8; 32]);
    env.as_contract(&contract_id, || {
        let bid = Bid {
            bid_id: bid_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            bid_amount: 15_000,
            expected_return: 16_000,
            status: BidStatus::Placed,
            timestamp: env.ledger().timestamp(),
            expiration_timestamp: env.ledger().timestamp() + 3_600,
        };
        BidStorage::store_bid(&env, &bid);
    });

    let exp_after_bid = env.as_contract(&contract_id, || {
        get_investor_exposure(&env, &investor).unwrap()
    });
    assert_eq!(exp_after_bid, 15_000);

    let cap_after_bid = env.as_contract(&contract_id, || {
        get_investor_available_capacity(&env, &investor).unwrap()
    });
    assert_eq!(cap_after_bid, 35_000);

    // Add an active investment
    let investment_id = BytesN::from_array(&env, &[103u8; 32]);
    env.as_contract(&contract_id, || {
        let investment = Investment {
            investment_id: investment_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            amount: 20_000,
            funded_at: env.ledger().timestamp(),
            status: InvestmentStatus::Active,
            insurance: soroban_sdk::Vec::new(&env),
        };
        InvestmentStorage::store_investment(&env, &investment);
    });

    let exp_after_inv = env.as_contract(&contract_id, || {
        get_investor_exposure(&env, &investor).unwrap()
    });
    assert_eq!(exp_after_inv, 35_000);

    let cap_after_inv = env.as_contract(&contract_id, || {
        get_investor_available_capacity(&env, &investor).unwrap()
    });
    assert_eq!(cap_after_inv, 15_000);

    // Completing an investment restores capacity exactly
    env.as_contract(&contract_id, || {
        let mut inv = InvestmentStorage::get_investment(&env, &investment_id).unwrap();
        inv.status = InvestmentStatus::Completed;
        InvestmentStorage::update_investment(&env, &inv);
    });

    let exp_after_complete = env.as_contract(&contract_id, || {
        get_investor_exposure(&env, &investor).unwrap()
    });
    assert_eq!(exp_after_complete, 15_000);

    let cap_after_complete = env.as_contract(&contract_id, || {
        get_investor_available_capacity(&env, &investor).unwrap()
    });
    assert_eq!(cap_after_complete, 35_000);
}

// ============================================================================
// Payment Rate Limiter & Throttling Recovery
// ============================================================================

#[test]
fn test_payment_rate_limiter_burst_and_recovery() {
    use crate::payments::{
        PaymentRateLimiter, MAX_PAYMENTS_PER_WINDOW, PAYMENT_RATE_LIMIT_WINDOW_SECS,
    };
    use soroban_sdk::testutils::Ledger as _;

    let (env, contract_id) = setup();
    env.ledger().set_timestamp(1_000);
    let investor_a = Address::generate(&env);
    let investor_b = Address::generate(&env);

    // Initial check passes for account A up to MAX_PAYMENTS_PER_WINDOW
    for i in 0..MAX_PAYMENTS_PER_WINDOW {
        let res = env.as_contract(&contract_id, || {
            PaymentRateLimiter::check_and_record(&env, &investor_a)
        });
        assert_eq!(res, Ok(()), "iteration {i} must succeed under rate limit");
    }

    // Exceeding burst limit is throttled with OperationNotAllowed
    let throttled = env.as_contract(&contract_id, || {
        PaymentRateLimiter::check_and_record(&env, &investor_a)
    });
    assert_eq!(throttled, Err(QuickLendXError::OperationNotAllowed));

    // Independent account B is unaffected by Account A's throttling
    let res_b = env.as_contract(&contract_id, || {
        PaymentRateLimiter::check_and_record(&env, &investor_b)
    });
    assert_eq!(
        res_b,
        Ok(()),
        "Account B must not be throttled by Account A"
    );

    // Advance ledger timestamp beyond PAYMENT_RATE_LIMIT_WINDOW_SECS (window resets)
    env.ledger()
        .set_timestamp(1_000 + PAYMENT_RATE_LIMIT_WINDOW_SECS + 1);

    // Account A recovers after window reset
    let recovered = env.as_contract(&contract_id, || {
        PaymentRateLimiter::check_and_record(&env, &investor_a)
    });
    assert_eq!(
        recovered,
        Ok(()),
        "Account A must recover after rate limit window elapses"
    );
}

// ============================================================================
// Resource Bounds: MAX_INVOICE_AMOUNT & Dust Transfers
// ============================================================================

#[test]
fn test_transfer_funds_max_invoice_amount_boundary() {
    use crate::protocol_limits::MAX_INVOICE_AMOUNT;

    let (env, contract_id) = setup();
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let currency = setup_token(
        &env,
        &contract_id,
        &[(from.clone(), i128::MAX)],
        &[(from.clone(), i128::MAX)],
    );

    // Exactly MAX_INVOICE_AMOUNT succeeds
    let at_max = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &from, &to, MAX_INVOICE_AMOUNT)
    });
    assert_eq!(at_max, Ok(()));

    // Exceeding MAX_INVOICE_AMOUNT is rejected with InvalidAmount
    let over_max = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &from, &to, MAX_INVOICE_AMOUNT + 1)
    });
    assert_eq!(over_max, Err(QuickLendXError::InvalidAmount));
}
