#![cfg(test)]

use crate::protocol_limits::ProtocolLimitsContract;
use crate::QuickLendXError;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_validate_invoice_below_minimum() {
    let env = Env::default();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let result = env.as_contract(&contract_id, || {
        // Default minimum is 10 in current protocol defaults.
        ProtocolLimitsContract::validate_invoice(
            env.clone(),
            9, // Below minimum
            env.ledger().timestamp() + 86400,
        )
    });

    assert_eq!(result, Err(QuickLendXError::InvalidAmount));
}

#[test]
fn test_validate_invoice_at_minimum() {
    let env = Env::default();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let result = env.as_contract(&contract_id, || {
        // Default minimum is 10 in current protocol defaults.
        ProtocolLimitsContract::validate_invoice(
            env.clone(),
            10, // At minimum
            env.ledger().timestamp() + 86400,
        )
    });

    assert!(result.is_ok());
}

#[test]
fn test_validate_invoice_above_minimum() {
    let env = Env::default();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let result = env.as_contract(&contract_id, || {
        ProtocolLimitsContract::validate_invoice(
            env.clone(),
            5000, // Above minimum
            env.ledger().timestamp() + 86400,
        )
    });

    assert!(result.is_ok());
}
