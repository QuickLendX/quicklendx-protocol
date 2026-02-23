//! Reentrancy guard tests and Soroban security model documentation
//!
//! # Why Callback-Based Reentrancy is NOT Possible in Soroban
//!
//! Soroban's execution model differs fundamentally from Ethereum:
//!
//! 1. **No fallback functions**: Contracts cannot define code that runs
//!    when receiving tokens (no `receive()` or `fallback()` equivalents)
//!
//! 2. **SAC transfers are atomic**: `token.transfer()` updates balances
//!    without executing any recipient code - it's a pure state change
//!
//! 3. **No mid-execution callbacks**: Cross-contract calls complete fully
//!    before returning control to the caller
//!
//! # Why We Still Use Guards (Defense in Depth)
//!
//! - Protects against potential future Soroban model changes
//! - Prevents accidental nested calls within same transaction
//! - Documents security-critical code paths
//! - Standard security pattern for financial operations
//!
//! # Test Coverage
//!
//! Integration tests (via contract client):
//!   1. Guard blocks when lock is already held
//!   2. Lock is released after successful operation
//!   3. Lock is released after failed operation
//!   4. Sequential protected operations work correctly
//!
//! Unit tests (directly exercising `with_payment_guard`):
//!   5. Initial lock state is absent/false before any operation
//!   6. Guard returns the closure's value on success
//!   7. Error variant is specifically `OperationNotAllowed`
//!   8. Guard handles `Err` returned by the closure, releases lock
//!   9. Multiple lock/release cycles complete without deadlock
//!  10. Guard with a non-unit return type passes value through

use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::reentrancy::with_payment_guard;
use soroban_sdk::{
    symbol_short, testutils::Address as _, token, Address, BytesN, Env, String, Vec,
};

// ============================================================================
// Test Context - Efficient setup with shared resources
// ============================================================================

struct TestContext<'a> {
    env: Env,
    client: QuickLendXContractClient<'a>,
    contract_id: Address,
    admin: Address,
    currency: Address,
    sac_client: token::StellarAssetClient<'a>,
    token_client: token::Client<'a>,
}

impl<'a> TestContext<'a> {
    fn new(
        env: Env,
        client: QuickLendXContractClient<'a>,
        contract_id: Address,
        admin: Address,
        currency: Address,
    ) -> Self {
        let sac_client = token::StellarAssetClient::new(&env, &currency);
        let token_client = token::Client::new(&env, &currency);
        Self {
            env,
            client,
            contract_id,
            admin,
            currency,
            sac_client,
            token_client,
        }
    }
}

fn setup_context() -> TestContext<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    TestContext::new(env, client, contract_id, admin, currency)
}

fn setup_business(ctx: &TestContext, business: &Address) {
    ctx.client
        .submit_kyc_application(business, &String::from_str(&ctx.env, "Business KYC"));
    ctx.client.verify_business(&ctx.admin, business);
}

fn setup_investor(ctx: &TestContext, investor: &Address, limit: i128) {
    ctx.sac_client.mint(investor, &(limit * 10));
    let expiration = ctx.env.ledger().sequence() + 100_000;
    ctx.token_client
        .approve(investor, &ctx.contract_id, &(limit * 10), &expiration);
    ctx.client
        .submit_investor_kyc(investor, &String::from_str(&ctx.env, "Investor KYC"));
    ctx.client.verify_investor(investor, &limit);
}

/// Create a verified invoice with a placed bid (ready to accept).
fn create_invoice_with_bid(
    ctx: &TestContext,
    business: &Address,
    investor: &Address,
    amount: i128,
) -> (BytesN<32>, BytesN<32>) {
    let due_date = ctx.env.ledger().timestamp() + 86_400;

    let invoice_id = ctx.client.store_invoice(
        business,
        &amount,
        &ctx.currency,
        &due_date,
        &String::from_str(&ctx.env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&ctx.env),
    );

    ctx.client.verify_invoice(&invoice_id);

    let bid_id = ctx
        .client
        .place_bid(investor, &invoice_id, &amount, &(amount + 100));

    (invoice_id, bid_id)
}

// ============================================================================
// Integration Tests (via contract client)
// ============================================================================

/// Test 1: Guard blocks when lock is already set.
///
/// Simulates a reentrant call by manually setting `pay_lock` before
/// calling a protected function. Verifies the call is rejected.
#[test]
fn test_guard_blocks_when_lock_is_set() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 50_000);

    let (invoice_id, bid_id) = create_invoice_with_bid(&ctx, &business, &investor, 1_000);

    // Manually hold the lock before calling the protected function.
    ctx.env.as_contract(&ctx.contract_id, || {
        ctx.env
            .storage()
            .instance()
            .set(&symbol_short!("pay_lock"), &true);
    });

    let result = ctx.client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_err(), "Should fail when lock is already set");

    // Clean up so the env can be reused safely.
    ctx.env.as_contract(&ctx.contract_id, || {
        ctx.env
            .storage()
            .instance()
            .set(&symbol_short!("pay_lock"), &false);
    });
}

/// Test 2: Guard releases lock after successful operation.
///
/// Verifies that after a successful protected operation completes,
/// the lock is set back to false.
#[test]
fn test_guard_releases_lock_after_success() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 50_000);

    let (invoice_id, bid_id) = create_invoice_with_bid(&ctx, &business, &investor, 1_000);

    let result = ctx.client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_ok(), "accept_bid should succeed");

    let lock_value: bool = ctx.env.as_contract(&ctx.contract_id, || {
        ctx.env
            .storage()
            .instance()
            .get(&symbol_short!("pay_lock"))
            .unwrap_or(false)
    });

    assert!(!lock_value, "Lock must be false after successful operation");
}

/// Test 3: Guard releases lock after failed operation.
///
/// Verifies that even when the protected closure panics/errors,
/// the lock is still cleared to prevent deadlock.
#[test]
fn test_guard_releases_lock_after_failure() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 50_000);

    let (invoice_id, _) = create_invoice_with_bid(&ctx, &business, &investor, 1_000);

    // Use a bid_id that does not exist so the call fails inside the guard.
    let fake_bid_id = BytesN::from_array(&ctx.env, &[99u8; 32]);
    let result = ctx.client.try_accept_bid(&invoice_id, &fake_bid_id);
    assert!(result.is_err(), "Should fail for non-existent bid");

    let lock_value: bool = ctx.env.as_contract(&ctx.contract_id, || {
        ctx.env
            .storage()
            .instance()
            .get(&symbol_short!("pay_lock"))
            .unwrap_or(false)
    });

    assert!(!lock_value, "Lock must be false even after failed operation");
}

/// Test 4: Sequential protected calls succeed.
///
/// Verifies that multiple protected operations can run one after the other,
/// proving the lock is released between each call.
#[test]
fn test_sequential_protected_calls_succeed() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 100_000);

    let (invoice_1, bid_1) = create_invoice_with_bid(&ctx, &business, &investor, 1_000);
    let (invoice_2, bid_2) = create_invoice_with_bid(&ctx, &business, &investor, 2_000);

    let result_1 = ctx.client.try_accept_bid(&invoice_1, &bid_1);
    assert!(result_1.is_ok(), "First accept_bid should succeed");

    let result_2 = ctx.client.try_accept_bid(&invoice_2, &bid_2);
    assert!(
        result_2.is_ok(),
        "Second accept_bid should succeed (lock released after first)"
    );

    let lock_value: bool = ctx.env.as_contract(&ctx.contract_id, || {
        ctx.env
            .storage()
            .instance()
            .get(&symbol_short!("pay_lock"))
            .unwrap_or(false)
    });

    assert!(!lock_value, "Lock should be false after all operations");
}

// ============================================================================
// Unit Tests (directly exercising `with_payment_guard`)
// ============================================================================

/// Test 5: Initial lock state is absent before any operation.
///
/// Before `with_payment_guard` has ever been called, the storage key
/// should not exist — `unwrap_or(false)` must treat it as unlocked.
#[test]
fn test_initial_lock_state_is_absent() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    let lock_before: bool = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&symbol_short!("pay_lock"))
            .unwrap_or(false)
    });

    assert!(
        !lock_before,
        "Lock must be absent (treated as false) before any guard call"
    );
}

/// Test 6: Guard passes the closure's return value through on success.
///
/// `with_payment_guard` must return whatever the closure returns, not just `()`.
#[test]
fn test_guard_returns_closure_value() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    let outcome: Result<u32, QuickLendXError> = env.as_contract(&contract_id, || {
        with_payment_guard(&env, || Ok(42u32))
    });

    assert_eq!(
        outcome,
        Ok(42u32),
        "Guard must pass the closure's Ok value through unchanged"
    );
}

/// Test 7: Error variant is specifically `OperationNotAllowed`.
///
/// When the lock is held, the returned error must be exactly
/// `QuickLendXError::OperationNotAllowed`, not a generic/unknown variant.
#[test]
fn test_guard_error_variant_is_operation_not_allowed() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    env.as_contract(&contract_id, || {
        // Hold the lock manually.
        env.storage()
            .instance()
            .set(&symbol_short!("pay_lock"), &true);

        let result: Result<(), QuickLendXError> =
            with_payment_guard(&env, || Ok(()));

        assert_eq!(
            result,
            Err(QuickLendXError::OperationNotAllowed),
            "Must return OperationNotAllowed when lock is held"
        );

        // Release so env stays clean.
        env.storage()
            .instance()
            .set(&symbol_short!("pay_lock"), &false);
    });
}

/// Test 8: Guard releases lock when the closure itself returns `Err`.
///
/// If the user's closure returns `Err`, the guard must still clear the lock
/// before propagating the error.
#[test]
fn test_guard_releases_lock_when_closure_returns_err() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    env.as_contract(&contract_id, || {
        // Guard runs, closure returns Err.
        let result: Result<(), QuickLendXError> = with_payment_guard(&env, || {
            Err(QuickLendXError::InvoiceNotFound)
        });

        assert_eq!(
            result,
            Err(QuickLendXError::InvoiceNotFound),
            "Closure error must be propagated"
        );

        // Lock must be released despite the closure error.
        let lock: bool = env
            .storage()
            .instance()
            .get(&symbol_short!("pay_lock"))
            .unwrap_or(false);

        assert!(!lock, "Lock must be false after closure returns Err");
    });
}

// ============================================================================
// Unit Tests for with_payment_guard
// ============================================================================

/// Test 5: with_payment_guard returns Ok on success
///
/// Directly invokes with_payment_guard with a closure that succeeds.
#[test]
fn test_guard_unit_success() {
    let ctx = setup_context();

    let result: Result<i32, _> = ctx.env.as_contract(&ctx.contract_id, || {
        crate::reentrancy::with_payment_guard(&ctx.env, || Ok(42))
    });

    assert_eq!(result.unwrap(), 42);

    // Lock must be cleared
    let lock_value: bool = ctx.env.as_contract(&ctx.contract_id, || {
        let key = symbol_short!("pay_lock");
        ctx.env.storage().instance().get(&key).unwrap_or(false)
    });
    assert!(!lock_value, "Lock should be false after successful guard");
}

/// Test 6: with_payment_guard clears lock after inner error
///
/// Verifies the guard releases the lock even when the closure returns Err.
#[test]
fn test_guard_unit_failure_clears_lock() {
    let ctx = setup_context();

    let result: Result<(), _> = ctx.env.as_contract(&ctx.contract_id, || {
        crate::reentrancy::with_payment_guard(&ctx.env, || {
            Err(crate::errors::QuickLendXError::InvalidStatus)
        })
    });

    assert!(result.is_err());

    // Lock must be cleared despite error
    let lock_value: bool = ctx.env.as_contract(&ctx.contract_id, || {
        let key = symbol_short!("pay_lock");
        ctx.env.storage().instance().get(&key).unwrap_or(false)
    });
    assert!(
        !lock_value,
        "Lock should be false after guard failure"
    );
}

/// Test 7: with_payment_guard blocks reentrant calls
///
/// Manually sets lock then calls guard — should return OperationNotAllowed.
#[test]
fn test_guard_unit_reentrant_blocked() {
    let ctx = setup_context();

    let result: Result<(), _> = ctx.env.as_contract(&ctx.contract_id, || {
        // Set lock manually
        let key = symbol_short!("pay_lock");
        ctx.env.storage().instance().set(&key, &true);

        crate::reentrancy::with_payment_guard(&ctx.env, || Ok(()))
    });

    assert!(result.is_err(), "Should return OperationNotAllowed");

    // Clean up
    ctx.env.as_contract(&ctx.contract_id, || {
        let key = symbol_short!("pay_lock");
        ctx.env.storage().instance().set(&key, &false);
    });
}

/// Test 8: Lock defaults to false on fresh contract
///
/// Verifies pay_lock is not set on a fresh contract (defaults to false).
#[test]
fn test_lock_not_set_on_fresh_contract() {
    let ctx = setup_context();

    let lock_value: bool = ctx.env.as_contract(&ctx.contract_id, || {
        let key = symbol_short!("pay_lock");
        ctx.env.storage().instance().get(&key).unwrap_or(false)
    });

    assert!(!lock_value, "pay_lock should default to false");
}

// ============================================================================
// Guard Blocking on All Guarded Endpoints
// ============================================================================

/// Test 9: accept_bid_and_fund blocked by pre-set lock
#[test]
fn test_accept_bid_and_fund_guard_blocks() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 50_000);

    let (invoice_id, bid_id) = create_invoice_with_bid(&ctx, &business, &investor, 1_000);

    // Set lock before calling
    ctx.env.as_contract(&ctx.contract_id, || {
        let key = symbol_short!("pay_lock");
        ctx.env.storage().instance().set(&key, &true);
    });

    let result = ctx.client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    assert!(result.is_err(), "accept_bid_and_fund should fail when lock is set");

    // Clean up
    ctx.env.as_contract(&ctx.contract_id, || {
        let key = symbol_short!("pay_lock");
        ctx.env.storage().instance().set(&key, &false);
    });
}

/// Test 10: release_escrow_funds blocked by pre-set lock
#[test]
fn test_release_escrow_guard_blocks() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 50_000);

    let (invoice_id, bid_id) = create_invoice_with_bid(&ctx, &business, &investor, 1_000);

    // Accept bid to create escrow
    let result = ctx.client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_ok(), "accept_bid should succeed first");

    // Set lock before calling release
    ctx.env.as_contract(&ctx.contract_id, || {
        let key = symbol_short!("pay_lock");
        ctx.env.storage().instance().set(&key, &true);
    });

    let result = ctx.client.try_release_escrow_funds(&invoice_id);
    assert!(result.is_err(), "release_escrow_funds should fail when lock is set");

    // Clean up
    ctx.env.as_contract(&ctx.contract_id, || {
        let key = symbol_short!("pay_lock");
        ctx.env.storage().instance().set(&key, &false);
    });
}

/// Test 11: refund_escrow_funds blocked by pre-set lock
#[test]
fn test_refund_escrow_guard_blocks() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 50_000);

    let (invoice_id, bid_id) = create_invoice_with_bid(&ctx, &business, &investor, 1_000);

    // Accept bid to create escrow
    let result = ctx.client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_ok(), "accept_bid should succeed first");

    // Set lock before calling refund
    ctx.env.as_contract(&ctx.contract_id, || {
        let key = symbol_short!("pay_lock");
        ctx.env.storage().instance().set(&key, &true);
    });

    let result = ctx.client.try_refund_escrow_funds(&invoice_id, &ctx.admin);
    assert!(result.is_err(), "refund_escrow_funds should fail when lock is set");

    // Clean up
    ctx.env.as_contract(&ctx.contract_id, || {
        let key = symbol_short!("pay_lock");
        ctx.env.storage().instance().set(&key, &false);
    });
}

/// Test 12: Mixed sequential calls to different guarded endpoints
///
/// Verifies that calling different guarded functions sequentially works
/// because the lock is released between each call.
#[test]
fn test_mixed_sequential_endpoints() {
    let ctx = setup_context();

    let business = Address::generate(&ctx.env);
    let investor = Address::generate(&ctx.env);

    setup_business(&ctx, &business);
    setup_investor(&ctx, &investor, 100_000);

    // Create two invoices with bids
    let (invoice_1, bid_1) = create_invoice_with_bid(&ctx, &business, &investor, 1_000);
    let (invoice_2, bid_2) = create_invoice_with_bid(&ctx, &business, &investor, 2_000);

    // First: accept_bid (guarded)
    let r1 = ctx.client.try_accept_bid(&invoice_1, &bid_1);
    assert!(r1.is_ok(), "First accept_bid should succeed");

    // Second: accept_bid_and_fund (different guarded endpoint)
    let r2 = ctx.client.try_accept_bid_and_fund(&invoice_2, &bid_2);
    assert!(r2.is_ok(), "accept_bid_and_fund should succeed after accept_bid");

    // Lock should be clear after both
    let lock_value: bool = ctx.env.as_contract(&ctx.contract_id, || {
        let key = symbol_short!("pay_lock");
        ctx.env.storage().instance().get(&key).unwrap_or(false)
    });
    assert!(!lock_value, "Lock should be released after mixed sequential calls");
}
/// Test 9: Multiple lock/release cycles complete without deadlock.
///
/// Calls `with_payment_guard` five times in sequence — each must find
/// the lock free and release it cleanly for the next.
#[test]
fn test_multiple_lock_release_cycles() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    env.as_contract(&contract_id, || {
        for i in 0u32..5 {
            let result: Result<u32, QuickLendXError> =
                with_payment_guard(&env, || Ok(i));

            assert_eq!(
                result,
                Ok(i),
                "Cycle {i} must succeed — lock should be free between calls"
            );

            let lock: bool = env
                .storage()
                .instance()
                .get(&symbol_short!("pay_lock"))
                .unwrap_or(false);

            assert!(!lock, "Lock must be false after cycle {i}");
        }
    });
}

/// Test 10: Guard with explicit `Some(false)` in storage treats it as unlocked.
///
/// `unwrap_or(false)` handles absent keys, but the key may also be explicitly
/// stored as `false` (e.g., after a previous run). Guard must allow entry in
/// that case too.
#[test]
fn test_guard_allows_entry_when_lock_is_explicitly_false() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    env.as_contract(&contract_id, || {
        // Explicitly write false — simulates state after a previous guard run.
        env.storage()
            .instance()
            .set(&symbol_short!("pay_lock"), &false);

        let result: Result<u64, QuickLendXError> =
            with_payment_guard(&env, || Ok(99u64));

        assert_eq!(
            result,
            Ok(99u64),
            "Guard must allow entry when lock is explicitly false"
        );

        let lock: bool = env
            .storage()
            .instance()
            .get(&symbol_short!("pay_lock"))
            .unwrap_or(false);

        assert!(!lock, "Lock must remain false after successful call");
    });
}
