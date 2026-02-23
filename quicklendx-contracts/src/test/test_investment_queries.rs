use soroban_sdk::{Env, Address, Symbol};

#[test]
fn test_empty_investment_queries_do_not_panic() {
    let env = Env::default();
    env.mock_all_auths();

    let investor = Address::generate(&env);
    let contract_id = env.current_contract_address();

    let result: Vec<u64> = env.invoke_contract(
        &contract_id,
        &Symbol::short("get_investments_by_investor"),
        (investor,),
    );

    assert_eq!(result.len(), 0);
}
