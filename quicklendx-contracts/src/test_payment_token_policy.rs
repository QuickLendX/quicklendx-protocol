use crate::errors::QuickLendXError;
use crate::payment_token_policy::PaymentTokenPolicy;
use crate::QuickLendXContract;
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn mutation_is_allowed_for_active_matching_tenant() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);

    let result = env.as_contract(&contract_id, || {
        PaymentTokenPolicy::authorize_mutation(&env, &admin, &admin, true)
    });

    assert_eq!(result, Ok(()));
}

#[test]
fn mutation_rejects_non_tenant_actor_with_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let non_admin = Address::generate(&env);
    let admin = Address::generate(&env);

    let result = env.as_contract(&contract_id, || {
        PaymentTokenPolicy::authorize_mutation(&env, &non_admin, &admin, true)
    });

    assert_eq!(result, Err(QuickLendXError::Unauthorized));
}

#[test]
fn mutation_rejects_inactive_tenant_with_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let tenant = Address::generate(&env);

    let result = env.as_contract(&contract_id, || {
        PaymentTokenPolicy::authorize_mutation(&env, &tenant, &tenant, false)
    });

    assert_eq!(result, Err(QuickLendXError::InvalidStatus));
}

#[test]
fn fee_configuration_accepts_zero_and_each_maximum_boundary() {
    assert_eq!(
        PaymentTokenPolicy::validate_fee_configuration(0, 0, 0, 0),
        Ok(())
    );
    assert_eq!(
        PaymentTokenPolicy::validate_fee_configuration(500, 300, 2_000, 500),
        Ok(())
    );
    assert_eq!(
        PaymentTokenPolicy::validate_fee_configuration(499, 299, 1_999, 499),
        Ok(())
    );
}

#[test]
fn fee_configuration_rejects_each_value_above_its_maximum() {
    let invalid_configurations = [
        (501, 300, 2_000, 500),
        (500, 301, 2_000, 500),
        (500, 300, 2_001, 500),
        (500, 300, 2_000, 501),
    ];

    for (origination_bps, servicing_bps, default_penalty_bps, early_repayment_bps) in
        invalid_configurations
    {
        assert_eq!(
            PaymentTokenPolicy::validate_fee_configuration(
                origination_bps,
                servicing_bps,
                default_penalty_bps,
                early_repayment_bps,
            ),
            Err(QuickLendXError::InvalidAmount)
        );
    }
}
