//! Direct unit tests for the payments module.
//!
//! These tests verify token transfer prechecks and escrow operations in isolation,
//! ensuring insufficient balance and allowance are rejected before any token call,
//! and no partial state updates persist on failure.

use soroban_sdk::{
    testutils::Address as _,
    token, Address, BytesN, Env,
};

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
// transfer_funds — negative tests
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
fn test_transfer_funds_same_address_no_op() {
    let (env, contract_id) = setup();
    let currency = setup_token(&env, &contract_id, &[], &[]);
    let addr = Address::generate(&env);

    let result = env.as_contract(&contract_id, || {
        transfer_funds(&env, &currency, &addr, &addr, 1_000)
    });

    assert_eq!(result, Ok(()));
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
// transfer_funds — positive tests
// ============================================================================

#[test]
fn test_transfer_funds_contract_sender_success() {
    let (env, contract_id) = setup();
    let to = Address::generate(&env);
    let currency = setup_token(
        &env,
        &contract_id,
        &[(contract_id.clone(), 5_000)],
        &[],
    );
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
// create_escrow — negative tests
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
// create_escrow — positive test
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
// release_escrow — negative and positive tests
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

    let result = env.as_contract(&contract_id, || {
        release_escrow(&env, &invoice_id)
    });

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

    let result = env.as_contract(&contract_id, || {
        release_escrow(&env, &invoice_id)
    });

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
// refund_escrow — negative and positive tests
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

    let result = env.as_contract(&contract_id, || {
        refund_escrow(&env, &invoice_id)
    });

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

    let result = env.as_contract(&contract_id, || {
        refund_escrow(&env, &invoice_id)
    });

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
