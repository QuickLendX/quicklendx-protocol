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
use crate::types::{DataKey, ProtocolConfig};

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    pub fn init(env: Env, admin: Address, fee: u32, min_holding: u64) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract is already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
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
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_initialization() {
        let env = Env::default();
        let contract_id = env.register_contract(None, QuickLendXContract);
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let fee = 300;
        let min_holding = 86400;

        client.init(&admin, &fee, &min_holding);
        
        let stored_admin: Address = env.as_contract(&contract_id, || {
            env.storage().instance().get(&DataKey::Admin).unwrap()
        });
        assert_eq!(stored_admin, admin);
    }
}