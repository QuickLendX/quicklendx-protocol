#![cfg(test)]

use crate::protocol_limits::ProtocolLimitsContract;
use crate::QuickLendXError;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_validate_invoice_below_minimum() {
    let env = Env::default();
    
    // Default minimum is 1000 in test mode
    let result = ProtocolLimitsContract::validate_invoice(
        env.clone(),
        999, // Below minimum
        env.ledger().timestamp() + 86400,
    );
    
    assert_eq!(result, Err(QuickLendXError::InvalidAmount));
}

#[test]
fn test_validate_invoice_at_minimum() {
    let env = Env::default();
    
    // Default minimum is 1000 in test mode
    let result = ProtocolLimitsContract::validate_invoice(
        env.clone(),
        1000, // At minimum
        env.ledger().timestamp() + 86400,
    );
    
    assert!(result.is_ok());
}

#[test]
fn test_validate_invoice_above_minimum() {
    let env = Env::default();
    
    let result = ProtocolLimitsContract::validate_invoice(
        env.clone(),
        5000, // Above minimum
        env.ledger().timestamp() + 86400,
    );
    
    assert!(result.is_ok());
}
