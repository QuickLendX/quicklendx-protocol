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