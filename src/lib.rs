/// QuickLendX Smart Contract Library
///
/// This crate contains the core arithmetic modules for the QuickLendX
/// invoice-financing protocol built on Stellar's Soroban platform.
///
/// ## Modules
///
/// - [`settlement`]    — Invoice settlement payout computation
/// - [`fees`]          — Protocol fee calculations (origination, servicing, default, early-repayment)
/// - [`profits`]       — Investor return metrics and platform revenue aggregation
/// - [`verification`]  — Centralized guards preventing unverified actors from restricted actions
///
/// ## Safety Philosophy
///
/// All financial arithmetic uses `u128` with `checked_*` operations.
/// Any computation that would overflow returns `None`; callers must handle
/// this as an error condition. This eliminates silent wrapping overflow,
/// underflow, and sign-extension bugs.
///
/// The verification module enforces a **deny-by-default** policy: every
/// restricted action requires the caller to prove verified status through
/// a guard function.  Pending, rejected, and unknown actors are blocked.
pub mod admin;
pub mod errors;
pub mod events;
pub mod fees;
pub mod init;
pub mod pause;
pub mod profits;
pub mod settlement;
pub mod storage_types;
pub mod verification;
pub mod payments;
pub mod invariants;

pub mod types;

use soroban_sdk::{contract, contractimpl, Env, Address};
use types::{DataKey, ProtocolConfig};

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    pub fn init(env: Env, admin: Address, fee: u32, min_holding: u64) {
        // Prevent re-initialization by checking if Admin is already set
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract is already initialized");
        }

        // Set the administrator address
        env.storage().instance().set(&DataKey::Admin, &admin);

        // Store the protocol configuration parameters
        let config = ProtocolConfig {
            fee_percentage: fee,
            min_holding_period: min_holding,
        };
        env.storage().instance().set(&DataKey::Config, &config);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::Env;
    use soroban_sdk::testutils::Address as _; // Brings the mock address generator trait into scope

    #[test]
    fn test_initialization() {
        let env = Env::default();
        let contract_id = env.register_contract(None, QuickLendXContract);
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let fee = 300; // 3%
        let min_holding = 86400; // 1 day

        // Initialize the contract cleanly
        client.init(&admin, &fee, &min_holding);

        // Directly query the contract state using storage lookups to satisfy 
        // test assertions and code coverage without causing an OS abort loop
        let stored_admin: Address = env.as_contract(&contract_id, || {
            env.storage().instance().get(&DataKey::Admin).unwrap()
        });
        
        let stored_config: ProtocolConfig = env.as_contract(&contract_id, || {
            env.storage().instance().get(&DataKey::Config).unwrap()
        });

#[cfg(test)]
mod test_solvency_invariant;
