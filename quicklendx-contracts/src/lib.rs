#![no_std]

use soroban_sdk::{contract, contractimpl, Env, Symbol, symbol_short};

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    pub fn hello(env: Env) -> Symbol {
        symbol_short!("A1")
    }
}
