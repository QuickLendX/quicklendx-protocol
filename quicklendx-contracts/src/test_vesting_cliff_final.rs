#![cfg(test)]
//! Exact-ledger boundary tests for vesting cliff and final-vest.
//!
//! Locks the contract behaviour at the two most regression-prone boundaries:
//!   * cliff-at-exact-ledger  — the transition from 0 to >0 releasable
//!   * final-vest-at-exact-ledger — the transition from partial to total releasable

use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env};

const ADMIN_BALANCE: i128 = 10_000_000;

fn setup() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address, // admin
    Address, // beneficiary
    Address, // token_id
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    client.initialize_admin(&admin);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let sac = token::StellarAssetClient::new(&env, &token_id);
    let token_client = token::Client::new(&env, &token_id);
    sac.mint(&admin, &ADMIN_BALANCE);
    token_client.approve(
        &admin,
        &contract_id,
        &ADMIN_BALANCE,
        &(env.ledger().sequence() + 10_000),
    );

    (env, client, admin, beneficiary, token_id)
}

// ---------------------------------------------------------------------------
// CLIFF-AT-EXACT-LEDGER
// ---------------------------------------------------------------------------

/// One ledger before cliff_time → releasable must be zero (cliff gate not yet open).
#[test]
fn releasable_is_zero_one_ledger_before_cliff() {
    let (env, client, admin, beneficiary, token_id) = setup();

    // schedule: start=1000, cliff=500s → cliff_time=1500, end=3000
    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &1_000, &1_000, &500, &3_000);

    env.ledger().set_timestamp(1_499); // cliff_time - 1
    assert_eq!(client.get_vesting_releasable(&id).unwrap(), 0);
}

/// At exact cliff_time → releasable must be strictly positive.
#[test]
fn releasable_is_positive_at_exact_cliff_ledger() {
    let (env, client, admin, beneficiary, token_id) = setup();

    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &1_000, &1_000, &500, &3_000);

    env.ledger().set_timestamp(1_500); // exactly cliff_time
    assert!(
        client.get_vesting_releasable(&id).unwrap() > 0,
        "releasable must be >0 at exact cliff ledger"
    );
}

/// At exact cliff_time the releasable value matches the linear formula:
///   total * (cliff_time - start_time) / (end_time - start_time)
#[test]
fn releasable_at_exact_cliff_matches_linear_formula() {
    let (env, client, admin, beneficiary, token_id) = setup();

    let total: i128 = 2_000;
    let start: u64 = 1_000;
    let cliff_seconds: u64 = 500;
    let end: u64 = 3_000;
    // cliff_time = 1500, duration = 2000, elapsed-at-cliff = 500
    // expected = 2000 * 500 / 2000 = 500

    let id = client.create_vesting_schedule(
        &admin, &token_id, &beneficiary, &total, &start, &cliff_seconds, &end,
    );

    env.ledger().set_timestamp(start + cliff_seconds);
    let expected = total * cliff_seconds as i128 / (end - start) as i128;
    assert_eq!(
        client.get_vesting_releasable(&id).unwrap(),
        expected,
        "releasable at cliff must equal total * elapsed / duration"
    );
}

/// release() called at exact cliff_time must succeed and return >0.
#[test]
fn release_succeeds_at_exact_cliff_ledger() {
    let (env, client, admin, beneficiary, token_id) = setup();

    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &1_000, &1_000, &500, &3_000);

    env.ledger().set_timestamp(1_500);
    let released = client.release_vested_tokens(&beneficiary, &id);
    assert!(released > 0, "release at exact cliff must transfer tokens");
}

/// release() called one ledger before cliff must return an error.
#[test]
fn release_fails_one_ledger_before_cliff() {
    let (env, client, admin, beneficiary, token_id) = setup();

    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &1_000, &1_000, &500, &3_000);

    env.ledger().set_timestamp(1_499); // cliff_time - 1
    assert!(
        client.try_release_vested_tokens(&beneficiary, &id).is_err(),
        "release before cliff must be rejected"
    );
}

// ---------------------------------------------------------------------------
// FINAL-VEST-AT-EXACT-LEDGER
// ---------------------------------------------------------------------------

/// One ledger before end_time → releasable must be strictly less than total.
#[test]
fn releasable_is_less_than_total_one_ledger_before_end() {
    let (env, client, admin, beneficiary, token_id) = setup();

    let total: i128 = 1_000;
    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &total, &1_000, &500, &3_000);

    env.ledger().set_timestamp(2_999); // end_time - 1
    assert!(
        client.get_vesting_releasable(&id).unwrap() < total,
        "releasable must be < total one ledger before end"
    );
}

/// At exact end_time → releasable must equal total_amount.
#[test]
fn releasable_equals_total_at_exact_end_ledger() {
    let (env, client, admin, beneficiary, token_id) = setup();

    let total: i128 = 1_000;
    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &total, &1_000, &500, &3_000);

    env.ledger().set_timestamp(3_000); // exactly end_time
    assert_eq!(
        client.get_vesting_releasable(&id).unwrap(),
        total,
        "releasable must equal total at exact end ledger"
    );
}

/// release() at exact end_time delivers every remaining token (no rounding dust left behind).
#[test]
fn release_at_exact_end_ledger_delivers_full_remaining_amount() {
    let (env, client, admin, beneficiary, token_id) = setup();

    let total: i128 = 1_001; // odd amount to expose truncation dust
    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &total, &1_000, &500, &3_000);

    // Partial release at cliff to leave some already-released
    env.ledger().set_timestamp(1_500);
    client.release_vested_tokens(&beneficiary, &id);

    // Full release at end
    env.ledger().set_timestamp(3_000);
    client.release_vested_tokens(&beneficiary, &id);

    let schedule = client.get_vesting_schedule(&id).unwrap();
    assert_eq!(
        schedule.released_amount, total,
        "released_amount must equal total at end_time, no dust left"
    );
}

/// released_amount must never exceed total_amount, even after repeated calls at end_time.
#[test]
fn released_amount_never_exceeds_total_at_end_ledger() {
    let (env, client, admin, beneficiary, token_id) = setup();

    let total: i128 = 500;
    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &total, &1_000, &0, &2_000);

    env.ledger().set_timestamp(2_000);
    client.release_vested_tokens(&beneficiary, &id);
    // Second call must return 0 and not corrupt released_amount.
    let extra = client.release_vested_tokens(&beneficiary, &id);
    assert_eq!(extra, 0);

    let schedule = client.get_vesting_schedule(&id).unwrap();
    assert_eq!(schedule.released_amount, total);
}

// ---------------------------------------------------------------------------
// OFF-BY-ONE SYMMETRY (cliff and end together)
// ---------------------------------------------------------------------------

/// The step from (cliff - 1) to cliff must be the only transition from 0 to >0.
/// The step from (end - 1) to end must be the only transition to full amount.
#[test]
fn off_by_one_symmetry_cliff_and_end() {
    let (env, client, admin, beneficiary, token_id) = setup();

    let total: i128 = 1_000;
    let id =
        client.create_vesting_schedule(&admin, &token_id, &beneficiary, &total, &1_000, &500, &3_000);

    // cliff boundary
    env.ledger().set_timestamp(1_499);
    let before_cliff = client.get_vesting_releasable(&id).unwrap();
    env.ledger().set_timestamp(1_500);
    let at_cliff = client.get_vesting_releasable(&id).unwrap();
    assert_eq!(before_cliff, 0);
    assert!(at_cliff > 0);

    // end boundary (fresh schedule, nothing released yet)
    env.ledger().set_timestamp(2_999);
    let before_end = client.get_vesting_releasable(&id).unwrap();
    env.ledger().set_timestamp(3_000);
    let at_end = client.get_vesting_releasable(&id).unwrap();
    assert!(before_end < total);
    assert_eq!(at_end, total);
}
