#![no_std]

use soroban_sdk::{contract, contractimpl, Env};

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    pub fn hello(env: Env) ->کور { // or your actual contract methods
        // Replace with your actual implementation
    }
}
