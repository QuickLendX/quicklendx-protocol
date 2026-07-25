use crate::errors::QuickLendXError;
use crate::governance::{Governable, ProposalStatus};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    Address, BytesN, Env,
};

struct TestGovernance;

impl Governable for TestGovernance {
    fn quorum() -> u64 {
        2
    }

    fn voting_period_ledgers() -> u32 {
        3
    }

    fn execute_proposal(env: &Env, proposal_id: &BytesN<32>) -> Result<(), QuickLendXError> {
        let key = (symbol_short!("exec"), proposal_id.clone());
        env.storage().instance().set(&key, &true);
        Ok(())
    }
}

fn submit_proposal(
    env: &Env,
    contract_id: &Address,
    proposer: &Address,
    proposal_id: &BytesN<32>,
) -> crate::governance::Proposal {
    env.as_contract(contract_id, || {
        TestGovernance::submit_proposal(env, proposer, proposal_id.clone()).unwrap()
    })
}

fn was_executed(env: &Env, contract_id: &Address, proposal_id: &BytesN<32>) -> bool {
    env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .get(&(symbol_short!("exec"), proposal_id.clone()))
            .unwrap_or(false)
    })
}

#[test]
fn test_open_proposal_is_blocked_from_execution() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = Address::generate(&env);
    let proposer = Address::generate(&env);
    let proposal_id = BytesN::from_array(&env, &[1u8; 32]);

    let proposal = submit_proposal(&env, &contract_id, &proposer, &proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Active);

    let result = env.as_contract(&contract_id, || {
        TestGovernance::run_proposal(&env, &proposal_id)
    });
    assert_eq!(result, Err(QuickLendXError::InvalidStatus));

    let stored = env.as_contract(&contract_id, || {
        TestGovernance::get_proposal(&env, &proposal_id).unwrap()
    });
    assert_eq!(stored.status, ProposalStatus::Active);
    assert!(!was_executed(&env, &contract_id, &proposal_id));
}

#[test]
fn test_closed_proposal_executes_once_it_passes() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = Address::generate(&env);
    let proposer = Address::generate(&env);
    let proposal_id = BytesN::from_array(&env, &[2u8; 32]);
    let voter_one = Address::generate(&env);
    let voter_two = Address::generate(&env);

    let _proposal = submit_proposal(&env, &contract_id, &proposer, &proposal_id);
    env.as_contract(&contract_id, || {
        TestGovernance::cast_vote(&env, &voter_one, &proposal_id, true).unwrap();
        TestGovernance::cast_vote(&env, &voter_two, &proposal_id, true).unwrap();
    });

    env.ledger().set_sequence_number(10);

    let result = env.as_contract(&contract_id, || {
        TestGovernance::run_proposal(&env, &proposal_id)
    });
    assert_eq!(result, Ok(()));

    let stored = env.as_contract(&contract_id, || {
        TestGovernance::get_proposal(&env, &proposal_id).unwrap()
    });
    assert_eq!(stored.status, ProposalStatus::Executed);
    assert!(was_executed(&env, &contract_id, &proposal_id));
}

#[test]
fn test_rejected_or_executed_proposal_cannot_be_rerun() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = Address::generate(&env);
    let proposer = Address::generate(&env);
    let rejected_id = BytesN::from_array(&env, &[3u8; 32]);
    let voter = Address::generate(&env);

    let _proposal = submit_proposal(&env, &contract_id, &proposer, &rejected_id);
    env.as_contract(&contract_id, || {
        TestGovernance::cast_vote(&env, &voter, &rejected_id, false).unwrap();
    });
    env.ledger().set_sequence_number(10);

    let rejected_status = env.as_contract(&contract_id, || {
        TestGovernance::finalize_proposal(&env, &rejected_id).unwrap()
    });
    assert_eq!(rejected_status, ProposalStatus::Rejected);

    let rejected_result = env.as_contract(&contract_id, || {
        TestGovernance::run_proposal(&env, &rejected_id)
    });
    assert_eq!(rejected_result, Err(QuickLendXError::InvalidStatus));

    let executed_id = BytesN::from_array(&env, &[4u8; 32]);
    let _executed_proposal = submit_proposal(&env, &contract_id, &proposer, &executed_id);
    let executed_voter_one = Address::generate(&env);
    let executed_voter_two = Address::generate(&env);
    env.as_contract(&contract_id, || {
        TestGovernance::cast_vote(&env, &executed_voter_one, &executed_id, true).unwrap();
        TestGovernance::cast_vote(&env, &executed_voter_two, &executed_id, true).unwrap();
    });
    env.ledger().set_sequence_number(10);

    let execute_result = env.as_contract(&contract_id, || {
        TestGovernance::run_proposal(&env, &executed_id)
    });
    assert_eq!(execute_result, Ok(()));

    let rerun_result = env.as_contract(&contract_id, || {
        TestGovernance::run_proposal(&env, &executed_id)
    });
    assert_eq!(rerun_result, Err(QuickLendXError::InvalidStatus));
}
