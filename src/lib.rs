#![no_std]
#![allow(unexpected_cfgs)] 

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

        assert_eq!(stored_admin, admin);
        assert_eq!(stored_config.fee_percentage, fee);
        assert_eq!(stored_config.min_holding_period, min_holding);
    }
}