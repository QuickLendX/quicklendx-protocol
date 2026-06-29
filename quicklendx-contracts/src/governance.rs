//! Governance trait abstraction for the QuickLendX protocol.
//!
//! Provides the [`Governable`] trait so that any contract module can plug in
//! a consistent proposal/voting/execution lifecycle without reimplementing
//! voting logic from scratch.
//!
//! # Design
//!
//! The trait is intentionally minimal: each method has a clear single
//! responsibility, and the storage keys are namespaced by `proposal_id` so
//! multiple active proposals can coexist.
//!
//! # `#![no_std]` discipline
//!
//! This module uses only `soroban_sdk` primitives; no `std::` types are
//! introduced.  All collections are `soroban_sdk::Vec` / `soroban_sdk::Map`.

use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol, Vec};

use crate::errors::QuickLendXError;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// On-chain status of a governance proposal.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    /// Proposal has been submitted and is open for voting.
    Active,
    /// Quorum reached and more votes in favour than against; ready to execute.
    Passed,
    /// Voting closed with insufficient support or quorum not met.
    Rejected,
    /// Proposal was executed on-chain after passing.
    Executed,
    /// Proposal was cancelled by its proposer or an admin before execution.
    Cancelled,
}

/// A single governance proposal stored on-chain.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Proposal {
    /// Unique 32-byte identifier (derived by the caller / SDK).
    pub id: BytesN<32>,
    /// Address that submitted the proposal.
    pub proposer: Address,
    /// Accumulated votes in favour.
    pub votes_for: u64,
    /// Accumulated votes against.
    pub votes_against: u64,
    /// Ledger sequence after which no more votes are accepted.
    pub voting_ends_at_ledger: u32,
    /// Current lifecycle status.
    pub status: ProposalStatus,
}

// ---------------------------------------------------------------------------
// Storage key helpers
// ---------------------------------------------------------------------------

/// Returns the instance-storage key for a proposal.
fn proposal_key(proposal_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
    (symbol_short!("gov_prop"), proposal_id.clone())
}

/// Returns the instance-storage key for the set of addresses that have already
/// voted on a proposal (prevents double-voting).
fn voted_key(proposal_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
    (symbol_short!("gov_vot"), proposal_id.clone())
}

// ---------------------------------------------------------------------------
// Governable trait
// ---------------------------------------------------------------------------

/// Pluggable governance interface.
///
/// Implement this trait on a unit struct (or directly in a contract module) to
/// give a contract a consistent proposal/vote/execute lifecycle without
/// duplicating vote-counting, quorum, or status-transition logic.
///
/// # Implementor responsibilities
///
/// - Provide `quorum()` and `voting_period_ledgers()` to express the protocol's
///   governance parameters in a single place.
/// - Provide `execute_proposal()` to perform the on-chain action once a
///   proposal has passed; the default methods guard against calling it on an
///   un-passed proposal.
///
/// # Default methods
///
/// `submit_proposal`, `cast_vote`, `finalize_proposal`, and `run_proposal` are
/// fully implemented here; implementors only need to supply the four required
/// methods below.
pub trait Governable {
    /// Minimum combined vote count (`votes_for + votes_against`) required for
    /// a proposal to be considered valid.
    fn quorum() -> u64;

    /// Number of ledgers the voting window stays open after `submit_proposal`.
    fn voting_period_ledgers() -> u32;

    /// Perform the on-chain action described by `proposal_id`.
    ///
    /// Called automatically by `run_proposal` after verifying the proposal
    /// status is `Passed`.  Implementations must be idempotent where possible.
    fn execute_proposal(
        env: &Env,
        proposal_id: &BytesN<32>,
    ) -> Result<(), QuickLendXError>;

    // ------------------------------------------------------------------
    // Default implementations — override only if the protocol requires it
    // ------------------------------------------------------------------

    /// Submit a new proposal.
    ///
    /// Stores the proposal in instance storage keyed by `proposal_id`.
    /// Returns `OperationNotAllowed` if a proposal with the same id already
    /// exists (idempotency guard).
    fn submit_proposal(
        env: &Env,
        proposer: &Address,
        proposal_id: BytesN<32>,
    ) -> Result<Proposal, QuickLendXError> {
        proposer.require_auth();

        let key = proposal_key(&proposal_id);
        if env.storage().instance().has(&key) {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        let voting_ends_at_ledger = env
            .ledger()
            .sequence()
            .saturating_add(Self::voting_period_ledgers());

        let proposal = Proposal {
            id: proposal_id.clone(),
            proposer: proposer.clone(),
            votes_for: 0,
            votes_against: 0,
            voting_ends_at_ledger,
            status: ProposalStatus::Active,
        };

        env.storage().instance().set(&key, &proposal);
        // Initialise empty voter set
        let empty: Vec<Address> = Vec::new(env);
        env.storage().instance().set(&voted_key(&proposal_id), &empty);

        Ok(proposal)
    }

    /// Cast a vote on an active proposal.
    ///
    /// - `in_favour`: `true` → vote for, `false` → vote against.
    /// - Returns `InvalidStatus` if the proposal is not `Active`.
    /// - Returns `OperationNotAllowed` if the voting window has closed or the
    ///   caller has already voted.
    fn cast_vote(
        env: &Env,
        voter: &Address,
        proposal_id: &BytesN<32>,
        in_favour: bool,
    ) -> Result<(), QuickLendXError> {
        voter.require_auth();

        let key = proposal_key(proposal_id);
        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&key)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        if proposal.status != ProposalStatus::Active {
            return Err(QuickLendXError::InvalidStatus);
        }
        if env.ledger().sequence() > proposal.voting_ends_at_ledger {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        // Double-vote guard
        let voted_key = voted_key(proposal_id);
        let mut voters: Vec<Address> = env
            .storage()
            .instance()
            .get(&voted_key)
            .unwrap_or_else(|| Vec::new(env));

        if voters.contains(voter) {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        voters.push_back(voter.clone());
        env.storage().instance().set(&voted_key, &voters);

        if in_favour {
            proposal.votes_for = proposal.votes_for.saturating_add(1);
        } else {
            proposal.votes_against = proposal.votes_against.saturating_add(1);
        }
        env.storage().instance().set(&key, &proposal);

        Ok(())
    }

    /// Close voting and set the final `Passed` / `Rejected` status.
    ///
    /// May be called by anyone after the voting window closes.  Returns
    /// `OperationNotAllowed` if the window is still open or the proposal is
    /// not `Active`.
    fn finalize_proposal(
        env: &Env,
        proposal_id: &BytesN<32>,
    ) -> Result<ProposalStatus, QuickLendXError> {
        let key = proposal_key(proposal_id);
        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&key)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        if proposal.status != ProposalStatus::Active {
            return Err(QuickLendXError::InvalidStatus);
        }
        if env.ledger().sequence() <= proposal.voting_ends_at_ledger {
            return Err(QuickLendXError::OperationNotAllowed);
        }

        let total = proposal.votes_for.saturating_add(proposal.votes_against);
        let passed =
            total >= Self::quorum() && proposal.votes_for > proposal.votes_against;

        proposal.status = if passed {
            ProposalStatus::Passed
        } else {
            ProposalStatus::Rejected
        };

        env.storage().instance().set(&key, &proposal);
        Ok(proposal.status)
    }

    /// Execute a passed proposal.
    ///
    /// Calls `finalize_proposal` if the status is still `Active`, then
    /// delegates to [`Self::execute_proposal`] and marks the proposal
    /// `Executed`.  Returns `InvalidStatus` if the proposal is not `Passed`.
    fn run_proposal(
        env: &Env,
        proposal_id: &BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        let key = proposal_key(proposal_id);
        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&key)
            .ok_or(QuickLendXError::StorageKeyNotFound)?;

        // Auto-finalize if still active and window closed.
        if proposal.status == ProposalStatus::Active
            && env.ledger().sequence() > proposal.voting_ends_at_ledger
        {
            Self::finalize_proposal(env, proposal_id)?;
            proposal = env.storage().instance().get(&key).unwrap();
        }

        if proposal.status != ProposalStatus::Passed {
            return Err(QuickLendXError::InvalidStatus);
        }

        Self::execute_proposal(env, proposal_id)?;

        proposal.status = ProposalStatus::Executed;
        env.storage().instance().set(&key, &proposal);

        Ok(())
    }

    /// Read the current state of a proposal without mutating anything.
    fn get_proposal(
        env: &Env,
        proposal_id: &BytesN<32>,
    ) -> Result<Proposal, QuickLendXError> {
        env.storage()
            .instance()
            .get(&proposal_key(proposal_id))
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }
}
