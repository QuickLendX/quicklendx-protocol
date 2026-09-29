use soroban_sdk::{Env, Address, String};

use crate::storage_types::{
    ProtocolHealth,
    DataKey, // assuming you use an enum for storage keys
};

fn get_bool(env: &Env, key: DataKey) -> bool {
    env.storage()
        .instance()
        .get(&key)
        .unwrap_or(false)
}

fn get_u32(env: &Env, key: DataKey) -> u32 {
    env.storage()
        .instance()
        .get(&key)
        .unwrap_or(0)
}

fn get_option_address(env: &Env, key: DataKey) -> Option<Address> {
    env.storage()
        .instance()
        .get(&key)
}

fn get_string(env: &Env, key: DataKey) -> String {
    env.storage()
        .instance()
        .get(&key)
        .unwrap_or_else(|| String::from_str(env, "v0.0.0"))
}

/// Canonical protocol health view
pub fn get_protocol_health(env: &Env) -> ProtocolHealth {
    ProtocolHealth {
        version: get_string(env, DataKey::Version),
        initialized: get_bool(env, DataKey::Initialized),
        paused: get_bool(env, DataKey::Paused),
        emergency_withdraw_pending: get_bool(env, DataKey::EmergencyWithdrawPending),
        treasury: get_option_address(env, DataKey::Treasury),
        fee_bps: get_u32(env, DataKey::FeeBps),
        total_invoice_count: get_u32(env, DataKey::TotalInvoiceCount),
        currency_count: get_u32(env, DataKey::CurrencyCount),
    }
}