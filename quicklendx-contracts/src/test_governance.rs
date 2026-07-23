#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

use crate::errors::QuickLendXError;
use crate::governance::{Governable, ProposalStatus};

/// Test governance implementor.
///
/// Quorum = 3, voting period = 10 ledgers.
/// Execution marks a storage key to prove it was called.
struct TestGovernance;

impl Governable for TestGovernance {
    fn quorum() -> u64 {
        3
    }

    fn voting_period_ledgers() -> u32 {
        10
    }

    fn execute_proposal(
        env: &Env,
        proposal_id: &BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        env.storage()
            .instance()
            .set(&crate::admin::ADMIN_KEY, proposal_id);
        Ok(())
    }
}

fn setup() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_sequence_number(1000);
    env
}

fn proposal_id(env: &Env, n: u8) -> BytesN<32> {
    let mut id = [0u8; 32];
    id[0] = n;
    BytesN::from_array(env, &id)
}

// ============================================================================
// Proposal submission
// ============================================================================

#[test]
fn submit_proposal_creates_active_proposal() {
    let env = setup();
    let proposer = Address::generate(&env);
    let id = proposal_id(&env, 1);

    let proposal = TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();

    assert_eq!(proposal.id, id);
    assert_eq!(proposal.proposer, proposer);
    assert_eq!(proposal.status, ProposalStatus::Active);
    assert_eq!(proposal.votes_for, 0);
    assert_eq!(proposal.votes_against, 0);
    assert_eq!(proposal.voting_ends_at_ledger, 1010);
}

#[test]
fn submit_proposal_rejects_duplicate() {
    let env = setup();
    let proposer = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    assert_eq!(
        TestGovernance::submit_proposal(&env, &proposer, id),
        Err(QuickLendXError::OperationNotAllowed)
    );
}

// ============================================================================
// Vote casting
// ============================================================================

#[test]
fn cast_vote_in_favour_tally() {
    let env = setup();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    TestGovernance::cast_vote(&env, &voter, &id, true).unwrap();

    let proposal = TestGovernance::get_proposal(&env, &id).unwrap();
    assert_eq!(proposal.votes_for, 1);
    assert_eq!(proposal.votes_against, 0);
}

#[test]
fn cast_vote_against_tally() {
    let env = setup();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    TestGovernance::cast_vote(&env, &voter, &id, false).unwrap();

    let proposal = TestGovernance::get_proposal(&env, &id).unwrap();
    assert_eq!(proposal.votes_for, 0);
    assert_eq!(proposal.votes_against, 1);
}

#[test]
fn cast_vote_rejects_double_vote() {
    let env = setup();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    TestGovernance::cast_vote(&env, &voter, &id, true).unwrap();
    assert_eq!(
        TestGovernance::cast_vote(&env, &voter, &id, false),
        Err(QuickLendXError::OperationNotAllowed)
    );
}

#[test]
fn cast_vote_rejects_after_window_closed() {
    let env = setup();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    env.ledger().set_sequence_number(1011);
    assert_eq!(
        TestGovernance::cast_vote(&env, &voter, &id, true),
        Err(QuickLendXError::OperationNotAllowed)
    );
}

#[test]
fn cast_vote_rejects_non_active_proposal() {
    let env = setup();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    env.ledger().set_sequence_number(1011);
    TestGovernance::finalize_proposal(&env, &id).unwrap();
    assert_eq!(
        TestGovernance::cast_vote(&env, &voter, &id, true),
        Err(QuickLendXError::InvalidStatus)
    );
}

#[test]
fn cast_vote_rejects_nonexistent_proposal() {
    let env = setup();
    let voter = Address::generate(&env);
    let id = proposal_id(&env, 99);

    assert_eq!(
        TestGovernance::cast_vote(&env, &voter, &id, true),
        Err(QuickLendXError::StorageKeyNotFound)
    );
}

// ============================================================================
// Finalization
// ============================================================================

#[test]
fn finalize_proposal_to_passed_when_quorum_met_and_majority_for() {
    let env = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    TestGovernance::cast_vote(&env, &voter1, &id, true).unwrap();
    TestGovernance::cast_vote(&env, &voter2, &id, true).unwrap();
    TestGovernance::cast_vote(&env, &voter3, &id, true).unwrap();

    env.ledger().set_sequence_number(1011);
    let status = TestGovernance::finalize_proposal(&env, &id).unwrap();
    assert_eq!(status, ProposalStatus::Passed);
}

#[test]
fn finalize_proposal_to_rejected_when_majority_against() {
    let env = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    TestGovernance::cast_vote(&env, &voter1, &id, false).unwrap();
    TestGovernance::cast_vote(&env, &voter2, &id, false).unwrap();
    TestGovernance::cast_vote(&env, &voter3, &id, true).unwrap();

    env.ledger().set_sequence_number(1011);
    let status = TestGovernance::finalize_proposal(&env, &id).unwrap();
    assert_eq!(status, ProposalStatus::Rejected);
}

#[test]
fn finalize_proposal_to_rejected_when_quorum_not_met() {
    let env = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    TestGovernance::cast_vote(&env, &voter1, &id, true).unwrap();

    env.ledger().set_sequence_number(1011);
    let status = TestGovernance::finalize_proposal(&env, &id).unwrap();
    assert_eq!(status, ProposalStatus::Rejected);
}

#[test]
fn finalize_proposal_rejects_when_window_still_open() {
    let env = setup();
    let proposer = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    assert_eq!(
        TestGovernance::finalize_proposal(&env, &id),
        Err(QuickLendXError::OperationNotAllowed)
    );
}

#[test]
fn finalize_proposal_rejects_non_active_proposal() {
    let env = setup();
    let proposer = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    env.ledger().set_sequence_number(1011);
    TestGovernance::finalize_proposal(&env, &id).unwrap();
    assert_eq!(
        TestGovernance::finalize_proposal(&env, &id),
        Err(QuickLendXError::InvalidStatus)
    );
}

// ============================================================================
// Run proposal (execute)
// ============================================================================

#[test]
fn run_proposal_auto_finalizes_and_executes() {
    let env = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    TestGovernance::cast_vote(&env, &voter1, &id, true).unwrap();
    TestGovernance::cast_vote(&env, &voter2, &id, true).unwrap();
    TestGovernance::cast_vote(&env, &voter3, &id, true).unwrap();

    env.ledger().set_sequence_number(1011);
    TestGovernance::run_proposal(&env, &id).unwrap();

    let proposal = TestGovernance::get_proposal(&env, &id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);

    let stored: BytesN<32> = env
        .storage()
        .instance()
        .get(&crate::admin::ADMIN_KEY)
        .unwrap();
    assert_eq!(stored, id);
}

#[test]
fn run_proposal_rejects_non_passed_proposal() {
    let env = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    TestGovernance::cast_vote(&env, &voter1, &id, true).unwrap();

    env.ledger().set_sequence_number(1011);
    assert_eq!(
        TestGovernance::run_proposal(&env, &id),
        Err(QuickLendXError::InvalidStatus)
    );
}

// ============================================================================
// Query
// ============================================================================

#[test]
fn get_proposal_returns_correct_state() {
    let env = setup();
    let proposer = Address::generate(&env);
    let id = proposal_id(&env, 1);

    TestGovernance::submit_proposal(&env, &proposer, id.clone()).unwrap();
    let proposal = TestGovernance::get_proposal(&env, &id).unwrap();

    assert_eq!(proposal.id, id);
    assert_eq!(proposal.proposer, proposer);
    assert_eq!(proposal.status, ProposalStatus::Active);
}

#[test]
fn get_proposal_rejects_nonexistent() {
    let env = setup();
    let id = proposal_id(&env, 99);
    assert_eq!(
        TestGovernance::get_proposal(&env, &id),
        Err(QuickLendXError::StorageKeyNotFound)
    );
}
