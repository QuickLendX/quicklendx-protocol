#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, BytesN, Env};

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::governance::{Governable, Proposal, ProposalStatus};
use crate::storage::{InvoiceStorage, StorageManager};
use crate::QuickLendXContract;

struct TestGovernance;

impl Governable for TestGovernance {
    fn quorum() -> u64 {
        3
    }

    fn voting_period_ledgers() -> u32 {
        10
    }

    fn execute_proposal(env: &Env, proposal_id: &BytesN<32>) -> Result<(), QuickLendXError> {
        env.storage()
            .instance()
            .set(&crate::admin::ADMIN_KEY, proposal_id);
        Ok(())
    }

    // The existing lifecycle tests use an explicitly open test electorate.
    // The secure defaults are exercised by SecureGovernance below.
    fn is_eligible_proposer(_env: &Env, _proposer: &Address) -> bool {
        true
    }

    fn is_eligible_voter(_env: &Env, _voter: &Address) -> bool {
        true
    }
}

struct SecureGovernance;

impl Governable for SecureGovernance {
    fn quorum() -> u64 {
        3
    }

    fn voting_period_ledgers() -> u32 {
        10
    }

    fn execute_proposal(env: &Env, proposal_id: &BytesN<32>) -> Result<(), QuickLendXError> {
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

fn setup_with_admin() -> (Env, Address, Address) {
    let (env, contract_id) = setup();
    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || AdminStorage::initialize(&env, &admin).unwrap());
    (env, contract_id, admin)
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
    env.as_contract(contract_id, || TestGovernance::finalize_proposal(env, id))
}

fn run_proposal(env: &Env, contract_id: &Address, id: &BytesN<32>) -> Result<(), QuickLendXError> {
    env.as_contract(contract_id, || TestGovernance::run_proposal(env, id))
}

fn get_proposal(
    env: &Env,
    contract_id: &Address,
    id: &BytesN<32>,
) -> Result<Proposal, QuickLendXError> {
    env.as_contract(contract_id, || TestGovernance::get_proposal(env, id))
}

fn submit_secure_proposal(
    env: &Env,
    contract_id: &Address,
    proposer: &Address,
    id: BytesN<32>,
) -> Result<Proposal, QuickLendXError> {
    env.as_contract(contract_id, || SecureGovernance::submit_proposal(env, proposer, id))
}

fn cast_secure_vote(
    env: &Env,
    contract_id: &Address,
    voter: &Address,
    id: &BytesN<32>,
    in_favour: bool,
) -> Result<(), QuickLendXError> {
    env.as_contract(contract_id, || {
        SecureGovernance::cast_vote(env, voter, id, in_favour)
    })
}

fn finalize_secure_proposal(
    env: &Env,
    contract_id: &Address,
    id: &BytesN<32>,
) -> Result<ProposalStatus, QuickLendXError> {
    env.as_contract(contract_id, || SecureGovernance::finalize_proposal(env, id))
}

fn secure_governance_guard(env: &Env, contract_id: &Address) -> Result<(), QuickLendXError> {
    env.as_contract(contract_id, || crate::governance::require_no_open_governance_proposal(env))
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

#[test]
fn open_proposal_blocks_guard() {
    let (env, contract_id, admin) = setup_with_admin();
    let id = proposal_id(&env, 21);

    submit_secure_proposal(&env, &contract_id, &admin, id.clone()).unwrap();
    assert_eq!(
        secure_governance_guard(&env, &contract_id),
        Err(QuickLendXError::PendingGovernanceProposal),
    );

    env.ledger().set_sequence_number(1011);
    finalize_secure_proposal(&env, &contract_id, &id).unwrap();
    assert_eq!(secure_governance_guard(&env, &contract_id), Ok(()));
}

#[test]
fn counter_tracks_multiple() {
    let (env, contract_id, admin) = setup_with_admin();
    let ids = [proposal_id(&env, 31), proposal_id(&env, 32), proposal_id(&env, 33)];
    for id in &ids {
        submit_secure_proposal(&env, &contract_id, &admin, id.clone()).unwrap();
    }

    env.ledger().set_sequence_number(1011);
    finalize_secure_proposal(&env, &contract_id, &ids[0]).unwrap();
    finalize_secure_proposal(&env, &contract_id, &ids[1]).unwrap();
    assert_eq!(
        finalize_secure_proposal(&env, &contract_id, &ids[0]),
        Err(QuickLendXError::InvalidStatus),
    );
    assert_eq!(
        secure_governance_guard(&env, &contract_id),
        Err(QuickLendXError::PendingGovernanceProposal),
    );

    finalize_secure_proposal(&env, &contract_id, &ids[2]).unwrap();
    assert_eq!(secure_governance_guard(&env, &contract_id), Ok(()));
}

#[test]
fn random_address_cannot_propose() {
    let (env, contract_id, _admin) = setup_with_admin();
    let random_address = Address::generate(&env);

    assert_eq!(
        submit_secure_proposal(&env, &contract_id, &random_address, proposal_id(&env, 41))
            .unwrap_err(),
        QuickLendXError::NotAdmin,
    );
    assert_eq!(secure_governance_guard(&env, &contract_id), Ok(()));
}

#[test]
fn ineligible_voter_rejected() {
    let (env, contract_id, admin) = setup_with_admin();
    let id = proposal_id(&env, 51);
    let random_voter = Address::generate(&env);
    submit_secure_proposal(&env, &contract_id, &admin, id.clone()).unwrap();

    assert_eq!(
        cast_secure_vote(&env, &contract_id, &random_voter, &id, true),
        Err(QuickLendXError::NotAdmin),
    );
    let proposal = env.as_contract(&contract_id, || SecureGovernance::get_proposal(&env, &id))
        .unwrap();
    assert_eq!(proposal.votes_for, 0);
    assert_eq!(proposal.votes_against, 0);
}

#[test]
fn clear_all_with_open_proposal_errors() {
    let (env, contract_id, admin) = setup_with_admin();
    submit_secure_proposal(&env, &contract_id, &admin, proposal_id(&env, 61)).unwrap();

    assert_eq!(
        env.as_contract(&contract_id, || InvoiceStorage::clear_all(&env)),
        Err(QuickLendXError::PendingGovernanceProposal),
    );
    assert_eq!(
        env.as_contract(&contract_id, || StorageManager::clear_all_mappings(&env)),
        Err(QuickLendXError::PendingGovernanceProposal),
    );
    assert_eq!(
        secure_governance_guard(&env, &contract_id),
        Err(QuickLendXError::PendingGovernanceProposal),
    );
}
