#![allow(clippy::disallowed_methods)]
#![no_std]
use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum QuickLendXError {
    AccountIsFrozen = 1,
}

#[contracttype]
pub enum DataKey {
    Frozen(Address),
    Invoice(u64),
}

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    pub fn freeze(env: Env, admin: Address, target: Address) {
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Frozen(target.clone()), &true);
    }

    pub fn unfreeze(env: Env, admin: Address, target: Address) {
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::Frozen(target.clone()), &false);
    }

    pub fn is_frozen(env: Env, target: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Frozen(target))
            .unwrap_or(false)
    }

    pub fn create_invoice(
        env: Env,
        issuer: Address,
        invoice_id: u64,
    ) -> Result<(), QuickLendXError> {
        issuer.require_auth();
        if Self::is_frozen(env.clone(), issuer.clone()) {
            return Err(QuickLendXError::AccountIsFrozen);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Invoice(invoice_id), &issuer);
        Ok(())
    }
}
