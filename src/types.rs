use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Config,
    Balance(Address),
    Invoice(u64), // Lookup key for an invoice ID
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolConfig {
    pub fee_percentage: u32,
    pub min_holding_period: u64,
}