use soroban_sdk::{Address, Env, String};

use crate::storage::InvoiceStorage;
use crate::storage::CurrencyStorage;
use crate::storage::TreasuryStorage;
use crate::storage::FeeStorage;
use crate::storage::ProtocolStateStorage;

use crate::types::ProtocolHealth;

pub fn get_protocol_health(env: Env) -> ProtocolHealth {
    let state = ProtocolStateStorage::load(&env);
    let config = TreasuryStorage::load(&env);
    let fee = FeeStorage::get_fee_bps(&env);

    ProtocolHealth {
        version: String::from_str(&env, "v1.0.0"),

        initialized: state.initialized,
        paused: state.paused,
        emergency_withdraw_pending: state.emergency_withdraw_pending,

        treasury: config.treasury,
        fee_bps: fee,

        total_invoice_count: InvoiceStorage::get_total_count(&env),
        currency_count: CurrencyStorage::get_total_count(&env),
    }
}