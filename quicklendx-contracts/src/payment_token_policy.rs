use crate::errors::QuickLendXError;
use soroban_sdk::{Address, Env};

pub struct PaymentTokenPolicy;

impl PaymentTokenPolicy {
    /// Authorizes a mutation on a protected resource, ensuring the actor
    /// matches the expected tenant and the identity is not stale.
    pub fn authorize_mutation(
        _env: &Env,
        actor: &Address,
        expected_tenant: &Address,
        is_active: bool,
    ) -> Result<(), QuickLendXError> {
        actor.require_auth();

        if actor != expected_tenant {
            return Err(QuickLendXError::Unauthorized);
        }

        if !is_active {
            return Err(QuickLendXError::InvalidStatus);
        }

        Ok(())
    }

    /// Enforces bounds on fee configurations to prevent malicious or accidental
    /// extreme values from drifting into insolvency.
    pub fn validate_fee_configuration(
        origination_bps: u128,
        servicing_bps: u128,
        default_penalty_bps: u128,
        early_repayment_bps: u128,
    ) -> Result<(), QuickLendXError> {
        // Maximums based on fees.rs constants
        if origination_bps > 500 {
            return Err(QuickLendXError::InvalidAmount);
        }
        if servicing_bps > 300 {
            return Err(QuickLendXError::InvalidAmount);
        }
        if default_penalty_bps > 2000 {
            return Err(QuickLendXError::InvalidAmount);
        }
        if early_repayment_bps > 500 {
            return Err(QuickLendXError::InvalidAmount);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_authorize_mutation_success() {
        let env = Env::default();
        let actor = Address::generate(&env);
        
        let result = PaymentTokenPolicy::authorize_mutation(&env, &actor, &actor, true);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_authorize_mutation_unauthorized() {
        let env = Env::default();
        let actor = Address::generate(&env);
        let expected = Address::generate(&env);
        
        let result = PaymentTokenPolicy::authorize_mutation(&env, &actor, &expected, true);
        assert_eq!(result, Err(QuickLendXError::Unauthorized));
    }

    #[test]
    fn test_authorize_mutation_stale() {
        let env = Env::default();
        let actor = Address::generate(&env);
        
        let result = PaymentTokenPolicy::authorize_mutation(&env, &actor, &actor, false);
        assert_eq!(result, Err(QuickLendXError::InvalidStatus));
    }

    #[test]
    fn test_validate_fee_configuration() {
        assert_eq!(PaymentTokenPolicy::validate_fee_configuration(500, 300, 2000, 500), Ok(()));
        assert_eq!(PaymentTokenPolicy::validate_fee_configuration(501, 300, 2000, 500), Err(QuickLendXError::InvalidAmount));
        assert_eq!(PaymentTokenPolicy::validate_fee_configuration(500, 301, 2000, 500), Err(QuickLendXError::InvalidAmount));
        assert_eq!(PaymentTokenPolicy::validate_fee_configuration(500, 300, 2001, 500), Err(QuickLendXError::InvalidAmount));
        assert_eq!(PaymentTokenPolicy::validate_fee_configuration(500, 300, 2000, 501), Err(QuickLendXError::InvalidAmount));
    }
}
