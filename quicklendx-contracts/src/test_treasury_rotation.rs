use crate::QuickLendXContract;
use crate::QuickLendXContractClient;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

fn setup(env: &Env) -> (QuickLendXContractClient, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    client.initialize_fee_system(&admin);
    (client, admin)
}

// ============================================================================
// Initiation
// ============================================================================

#[test]
fn test_initiate_rotation_stores_pending_request() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    let req = client.initiate_treasury_rotation(&new_treasury);

    assert_eq!(req.new_address, new_treasury);
    assert!(req.confirmation_deadline > req.initiated_at);
}

#[test]
fn test_get_pending_rotation_returns_stored_request() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    client.initiate_treasury_rotation(&new_treasury);

    let pending = client.get_pending_treasury_rotation();
    assert!(pending.is_some());
    assert_eq!(pending.unwrap().new_address, new_treasury);
}

#[test]
fn test_no_pending_rotation_initially() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let pending = client.get_pending_treasury_rotation();
    assert!(pending.is_none());
}

#[test]
fn test_initiate_rotation_records_correct_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    let req = client.initiate_treasury_rotation(&new_treasury);

    let expected_ttl: u64 = 604_800;
    assert_eq!(
        req.confirmation_deadline,
        req.initiated_at + expected_ttl
    );
}

#[test]
fn test_initiate_rotation_rejects_duplicate() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);

    client.initiate_treasury_rotation(&addr_a);

    let result = client.try_initiate_treasury_rotation(&addr_b);
    assert!(result.is_err());
}

#[test]
fn test_initiate_rotation_rejects_same_current_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let treasury = Address::generate(&env);

    client.configure_treasury(&treasury);

    let result = client.try_initiate_treasury_rotation(&treasury);
    assert!(result.is_err());
}

// ============================================================================
// Confirmation
// ============================================================================

#[test]
fn test_confirm_rotation_updates_treasury_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    client.initiate_treasury_rotation(&new_treasury);
    let confirmed = client.confirm_treasury_rotation(&new_treasury);

    assert_eq!(confirmed, new_treasury);
    assert_eq!(client.get_treasury_address().unwrap(), new_treasury);
}

#[test]
fn test_confirm_rotation_clears_pending_request() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    client.initiate_treasury_rotation(&new_treasury);
    client.confirm_treasury_rotation(&new_treasury);

    assert!(client.get_pending_treasury_rotation().is_none());
}

#[test]
fn test_confirm_rotation_fails_with_wrong_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let new_treasury = Address::generate(&env);
    let wrong_addr = Address::generate(&env);

    client.initiate_treasury_rotation(&new_treasury);

    let result = client.try_confirm_treasury_rotation(&wrong_addr);
    assert!(result.is_err());
}

#[test]
fn test_confirm_rotation_fails_with_no_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let addr = Address::generate(&env);

    let result = client.try_confirm_treasury_rotation(&addr);
    assert!(result.is_err());
}

#[test]
fn test_confirm_rotation_fails_after_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    client.initiate_treasury_rotation(&new_treasury);

    // Advance past 7-day deadline
    let new_ts = env.ledger().timestamp() + 604_801;
    env.ledger().set_timestamp(new_ts);

    let result = client.try_confirm_treasury_rotation(&new_treasury);
    assert!(result.is_err());
}

#[test]
fn test_confirm_expired_rotation_clears_pending_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    client.initiate_treasury_rotation(&new_treasury);
    let new_ts = env.ledger().timestamp() + 604_801;
    env.ledger().set_timestamp(new_ts);
    let _ = client.try_confirm_treasury_rotation(&new_treasury);

    assert!(client.get_pending_treasury_rotation().is_none());
}

#[test]
fn test_confirm_rotation_at_exact_deadline_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    let req = client.initiate_treasury_rotation(&new_treasury);
    env.ledger().set_timestamp(req.confirmation_deadline);

    let confirmed = client.confirm_treasury_rotation(&new_treasury);
    assert_eq!(confirmed, new_treasury);
}

// ============================================================================
// Cancellation
// ============================================================================

#[test]
fn test_cancel_rotation_removes_pending_request() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    client.initiate_treasury_rotation(&new_treasury);
    client.cancel_treasury_rotation();

    assert!(client.get_pending_treasury_rotation().is_none());
}

#[test]
fn test_cancel_rotation_does_not_change_current_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let existing = Address::generate(&env);
    let new_treasury = Address::generate(&env);

    client.configure_treasury(&existing);
    client.initiate_treasury_rotation(&new_treasury);
    client.cancel_treasury_rotation();

    assert_eq!(client.get_treasury_address().unwrap(), existing);
}

#[test]
fn test_cancel_rotation_fails_with_nothing_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    let result = client.try_cancel_treasury_rotation();
    assert!(result.is_err());
}

// ============================================================================
// Full lifecycle
// ============================================================================

#[test]
fn test_full_rotation_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let treasury_v1 = Address::generate(&env);
    let treasury_v2 = Address::generate(&env);

    // Set initial treasury
    client.configure_treasury(&treasury_v1);
    assert_eq!(client.get_treasury_address().unwrap(), treasury_v1);

    // Initiate rotation to v2
    let req = client.initiate_treasury_rotation(&treasury_v2);
    assert_eq!(req.new_address, treasury_v2);

    // Confirm as new treasury
    let result = client.confirm_treasury_rotation(&treasury_v2);
    assert_eq!(result, treasury_v2);
    assert_eq!(client.get_treasury_address().unwrap(), treasury_v2);

    // No pending rotation remains
    assert!(client.get_pending_treasury_rotation().is_none());
}

#[test]
fn test_can_rotate_again_after_successful_rotation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);

    client.configure_treasury(&addr_a);
    client.initiate_treasury_rotation(&addr_b);
    client.confirm_treasury_rotation(&addr_b);

    let addr_c = Address::generate(&env);
    let req = client.initiate_treasury_rotation(&addr_c);
    assert_eq!(req.new_address, addr_c);
}

#[test]
fn test_can_initiate_after_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);

    client.initiate_treasury_rotation(&addr_a);
    client.cancel_treasury_rotation();

    let req = client.initiate_treasury_rotation(&addr_b);
    assert_eq!(req.new_address, addr_b);
}

#[test]
fn test_cancel_then_new_rotation_is_independent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);

    client.initiate_treasury_rotation(&addr_a);
    client.cancel_treasury_rotation();
    client.initiate_treasury_rotation(&addr_b);
    client.confirm_treasury_rotation(&addr_b);

    assert_eq!(client.get_treasury_address().unwrap(), addr_b);
}

// ============================================================================
// Fee routing with rotated treasury
// ============================================================================

#[test]
fn test_fee_config_reflects_new_treasury_after_rotation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    client.initiate_treasury_rotation(&new_treasury);
    client.confirm_treasury_rotation(&new_treasury);

    let config = client.get_platform_fee_config();
    assert_eq!(config.treasury_address.unwrap(), new_treasury);
}

#[test]
fn test_rotation_preserves_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let new_treasury = Address::generate(&env);

    client.set_platform_fee(&500i128);
    client.initiate_treasury_rotation(&new_treasury);
    client.confirm_treasury_rotation(&new_treasury);

    assert_eq!(client.get_platform_fee().fee_bps, 500);
}

// ============================================================================
// Event-driven verification (no false-positive state after error paths)
// ============================================================================

#[test]
fn test_failed_confirm_does_not_update_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let old_treasury = Address::generate(&env);
    let new_treasury = Address::generate(&env);
    let wrong = Address::generate(&env);

    client.configure_treasury(&old_treasury);
    client.initiate_treasury_rotation(&new_treasury);
    let _ = client.try_confirm_treasury_rotation(&wrong);

    assert_eq!(client.get_treasury_address().unwrap(), old_treasury);
}

#[test]
fn test_expired_rotation_does_not_update_treasury() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let old_treasury = Address::generate(&env);
    let new_treasury = Address::generate(&env);

    client.configure_treasury(&old_treasury);
    client.initiate_treasury_rotation(&new_treasury);
    let new_ts = env.ledger().timestamp() + 700_000;
    env.ledger().set_timestamp(new_ts);
    let _ = client.try_confirm_treasury_rotation(&new_treasury);

    assert_eq!(client.get_treasury_address().unwrap(), old_treasury);
}
