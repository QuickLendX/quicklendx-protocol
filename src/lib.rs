/// QuickLendX Soroban smart-contract crate root.
#![no_std]

pub mod admin;
pub mod errors;
pub mod init;
pub mod storage_types;

#[cfg(test)]
#[cfg(test)]
pub mod test_admin;

pub use admin::{AdminContract, AdminContractClient, FeeConfigDiff, ProtocolConfigDiff};
pub use errors::ContractError;
pub use storage_types::{DataKey, FeeConfig, ProtocolConfig};
