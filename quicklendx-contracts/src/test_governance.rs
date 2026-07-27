#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};

use crate::errors::QuickLendXError;
use crate::governance::{Governable, Proposal, ProposalStatus};
use crate::QuickLendXContract;

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

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_sequence_number(1000);
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

fn proposal_id(env: &Env, n: u8) -> BytesN<32> {
    let mut id = [0u8; 32];
    id[0] = n;
    BytesN::from_array(env, &id)
}

fn submit_proposal(
    env: &Env,
    contract_id: &Address,
    proposer: &Address,
    id: BytesN<32>,
) -> Result<Proposal, QuickLendXError> {
    env.as_contract(contract_id, || {
        TestGovernance::submit_proposal(env, proposer, id)
    })
}

fn cast_vote(
    env: &Env,
    contract_id: &Address,
    voter: &Address,
    id: &BytesN<32>,
    in_favour: bool,
) -> Result<(), QuickLendXError> {
    env.as_contract(contract_id, || {
        TestGovernance::cast_vote(env, voter, id, in_favour)
    })
}

fn finalize_proposal(
    env: &Env,
    contract_id: &Address,
    id: &BytesN<32>,
) -> Result<ProposalStatus, QuickLendXError> {
    env.as_contract(contract_id, || {
        TestGovernance::finalize_proposal(env, id)
    })
}

fn run_proposal(
    env: &Env,
    contract_id: &Address,
    id: &BytesN<32>,
) -> Result<(), QuickLendXError> {
    env.as_contract(contract_id, || {
        TestGovernance::run_proposal(env, id)
    })
}

fn get_proposal(
    env: &Env,
    contract_id: &Address,
    id: &BytesN<32>,
) -> Result<Proposal, QuickLendXError> {
    env.as_contract(contract_id, || {
        TestGovernance::get_proposal(env, id)
    })
}

// ============================================================================
// Proposal submission
// ============================================================================

#[test]
fn submit_proposal_creates_active_proposal() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let id = proposal_id(&env, 1);

    let proposal = submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();

    assert!(proposal.id == id);
    assert!(proposal.proposer == proposer);
    assert!(proposal.status == ProposalStatus::Active);
    assert_eq!(proposal.votes_for, 0);
    assert_eq!(proposal.votes_against, 0);
    assert_eq!(proposal.voting_ends_at_ledger, 1010);
}

#[test]
fn submit_proposal_rejects_duplicate() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    let err = submit_proposal(&env, &contract_id, &proposer, id).unwrap_err();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

// ============================================================================
// Vote casting
// ============================================================================

#[test]
fn cast_vote_in_favour_tally() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter, &id, true).unwrap();

    let proposal = get_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(proposal.votes_for, 1);
    assert_eq!(proposal.votes_against, 0);
}

#[test]
fn cast_vote_against_tally() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter, &id, false).unwrap();

    let proposal = get_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(proposal.votes_for, 0);
    assert_eq!(proposal.votes_against, 1);
}

#[test]
fn cast_vote_rejects_double_vote() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter, &id, true).unwrap();
    let err = cast_vote(&env, &contract_id, &voter, &id, false).unwrap_err();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn cast_vote_rejects_after_window_closed() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    env.ledger().set_sequence_number(1011);
    let err = cast_vote(&env, &contract_id, &voter, &id, true).unwrap_err();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn cast_vote_rejects_non_active_proposal() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    env.ledger().set_sequence_number(1011);
    finalize_proposal(&env, &contract_id, &id).unwrap();
    let err = cast_vote(&env, &contract_id, &voter, &id, true).unwrap_err();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

#[test]
fn cast_vote_rejects_nonexistent_proposal() {
    let (env, contract_id) = setup();
    let voter = Address::generate(&env);
    let id = proposal_id(&env, 99);

    let err = cast_vote(&env, &contract_id, &voter, &id, true).unwrap_err();
    assert_eq!(err, QuickLendXError::StorageKeyNotFound);
}

// ============================================================================
// Finalization
// ============================================================================

#[test]
fn finalize_proposal_to_passed_when_quorum_met_and_majority_for() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter1, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter2, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter3, &id, true).unwrap();

    env.ledger().set_sequence_number(1011);
    let status = finalize_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(status, ProposalStatus::Passed);
}

#[test]
fn finalize_proposal_to_rejected_when_majority_against() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter1, &id, false).unwrap();
    cast_vote(&env, &contract_id, &voter2, &id, false).unwrap();
    cast_vote(&env, &contract_id, &voter3, &id, true).unwrap();

    env.ledger().set_sequence_number(1011);
    let status = finalize_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(status, ProposalStatus::Rejected);
}

#[test]
fn finalize_proposal_to_rejected_when_quorum_not_met() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter1, &id, true).unwrap();

    env.ledger().set_sequence_number(1011);
    let status = finalize_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(status, ProposalStatus::Rejected);
}

#[test]
fn finalize_proposal_rejects_when_window_still_open() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    let err = finalize_proposal(&env, &contract_id, &id).unwrap_err();
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}

#[test]
fn finalize_proposal_rejects_non_active_proposal() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    env.ledger().set_sequence_number(1011);
    finalize_proposal(&env, &contract_id, &id).unwrap();
    let err = finalize_proposal(&env, &contract_id, &id).unwrap_err();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

// ============================================================================
// Quorum boundary edge cases
// ============================================================================

#[test]
fn finalize_proposal_passes_when_exactly_at_quorum_with_majority_for() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter1, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter2, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter3, &id, false).unwrap();

    env.ledger().set_sequence_number(1011);
    let status = finalize_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(status, ProposalStatus::Passed);
}

#[test]
fn finalize_proposal_rejects_when_exactly_at_quorum_with_majority_against() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter1, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter2, &id, false).unwrap();
    cast_vote(&env, &contract_id, &voter3, &id, false).unwrap();

    env.ledger().set_sequence_number(1011);
    let status = finalize_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(status, ProposalStatus::Rejected);
}

#[test]
fn finalize_proposal_rejects_when_one_vote_below_quorum_even_with_majority_for() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter1, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter2, &id, true).unwrap();

    env.ledger().set_sequence_number(1011);
    let status = finalize_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(status, ProposalStatus::Rejected);
}

#[test]
fn finalize_proposal_rejects_when_one_vote_below_quorum_with_split_votes() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter1, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter2, &id, false).unwrap();

    env.ledger().set_sequence_number(1011);
    let status = finalize_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(status, ProposalStatus::Rejected);
}

#[test]
fn finalize_proposal_passes_when_one_vote_above_quorum_with_majority_for() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);
    let voter4 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter1, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter2, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter3, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter4, &id, false).unwrap();

    env.ledger().set_sequence_number(1011);
    let status = finalize_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(status, ProposalStatus::Passed);
}

#[test]
fn finalize_proposal_rejects_when_one_vote_above_quorum_but_tied() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);
    let voter4 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter1, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter2, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter3, &id, false).unwrap();
    cast_vote(&env, &contract_id, &voter4, &id, false).unwrap();

    env.ledger().set_sequence_number(1011);
    let status = finalize_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(status, ProposalStatus::Rejected);
}

// ============================================================================
// Run proposal (execute)
// ============================================================================

#[test]
fn run_proposal_auto_finalizes_and_executes() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let voter3 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter1, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter2, &id, true).unwrap();
    cast_vote(&env, &contract_id, &voter3, &id, true).unwrap();

    env.ledger().set_sequence_number(1011);
    run_proposal(&env, &contract_id, &id).unwrap();

    let proposal = get_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);

    let stored: BytesN<32> = env
        .as_contract(&contract_id, || {
            env.storage().instance().get(&crate::admin::ADMIN_KEY)
        })
        .unwrap();
    assert_eq!(stored, id);
}

#[test]
fn run_proposal_rejects_non_passed_proposal() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    cast_vote(&env, &contract_id, &voter1, &id, true).unwrap();

    env.ledger().set_sequence_number(1011);
    let err = run_proposal(&env, &contract_id, &id).unwrap_err();
    assert_eq!(err, QuickLendXError::InvalidStatus);
}

// ============================================================================
// Query
// ============================================================================

#[test]
fn get_proposal_returns_correct_state() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let id = proposal_id(&env, 1);

    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();
    let proposal = get_proposal(&env, &contract_id, &id).unwrap();

    assert!(proposal.id == id);
    assert!(proposal.proposer == proposer);
    assert!(proposal.status == ProposalStatus::Active);
}

#[test]
fn get_proposal_rejects_nonexistent() {
    let (env, contract_id) = setup();
    let id = proposal_id(&env, 99);
    let err = get_proposal(&env, &contract_id, &id).unwrap_err();
    assert_eq!(err, QuickLendXError::StorageKeyNotFound);
}

// ============================================================================
// Guard
// ============================================================================

#[test]
fn require_no_open_governance_proposal_blocks_destructive_ops() {
    let (env, contract_id) = setup();
    let proposer = Address::generate(&env);
    let id = proposal_id(&env, 1);

    // Should succeed when no active proposals
    env.as_contract(&contract_id, || {
        let res = crate::governance::require_no_open_governance_proposal(&env);
        assert!(res.is_ok());
    });

    // Create an active proposal
    submit_proposal(&env, &contract_id, &proposer, id.clone()).unwrap();

    // Guard should now fail
    env.as_contract(&contract_id, || {
        let err = crate::governance::require_no_open_governance_proposal(&env).unwrap_err();
        assert_eq!(err, QuickLendXError::PendingGovernanceProposal);
    });

    // Finalize the proposal (rejected because 0 votes)
    env.ledger().set_sequence_number(1011);
    finalize_proposal(&env, &contract_id, &id).unwrap();

    // Guard should succeed again
    env.as_contract(&contract_id, || {
        let res = crate::governance::require_no_open_governance_proposal(&env);
        assert!(res.is_ok());
    });
}
