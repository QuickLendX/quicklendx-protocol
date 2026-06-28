#![cfg(test)]

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env, IntoVal,
};

use crate::{QuickLendXContract, QuickLendXContractClient};

fn setup_env<'a>(
    env: &'a Env,
) -> (
    QuickLendXContractClient<'a>,
    TokenClient<'a>,
    Address,
) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);

    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = TokenClient::new(env, &token_id.address());
    let sac = StellarAssetClient::new(env, &token_id.address());

    client.initialize(&token.address);

    (client, token, contract_id)
}

fn fund_user(
    env: &Env,
    client: &QuickLendXContractClient,
    token: &TokenClient,
    sac: &StellarAssetClient,
    user: &Address,
    amount: i128,
) {
    env.mock_all_auths();
    sac.mint(user, &amount);

    env.mock_all_auths();
    client.deposit(user, &amount);
}

#[test]
fn two_actors_each_withdraw_their_full_balance_both_succeed() {
    let env = Env::default();
    let (client, token, contract_id) = setup_env(&env);

    let sac = StellarAssetClient::new(&env, &token.address());

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    let alice_deposit: i128 = 1_000;
    let bob_deposit: i128 = 500;

    fund_user(&env, &client, &token, &sac, &alice, alice_deposit);
    fund_user(&env, &client, &token, &sac, &bob, bob_deposit);

    env.mock_all_auths();
    client.withdraw(&alice, &alice_deposit);

    env.mock_all_auths();
    client.withdraw(&bob, &bob_deposit);

    assert_eq!(
        client.balance_of(&alice),
        0,
        "alice: protocol balance should be zero after full withdrawal"
    );
    assert_eq!(
        client.balance_of(&bob),
        0,
        "bob: protocol balance should be zero after full withdrawal"
    );

    assert_eq!(
        token.balance(&alice),
        alice_deposit,
        "alice: token balance should equal withdrawn amount"
    );
    assert_eq!(
        token.balance(&bob),
        bob_deposit,
        "bob: token balance should equal withdrawn amount"
    );
}

#[test]
fn two_actors_partial_withdraw_residual_balances_are_correct() {
    let env = Env::default();
    let (client, token, _) = setup_env(&env);
    let sac = StellarAssetClient::new(&env, &token.address());

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    fund_user(&env, &client, &token, &sac, &alice, 2_000);
    fund_user(&env, &client, &token, &sac, &bob, 1_000);

    env.mock_all_auths();
    client.withdraw(&alice, &800);

    env.mock_all_auths();
    client.withdraw(&bob, &300);

    assert_eq!(client.balance_of(&alice), 1_200);
    assert_eq!(client.balance_of(&bob), 700);
    assert_eq!(token.balance(&alice), 800);
    assert_eq!(token.balance(&bob), 300);
}

#[test]
#[should_panic]
fn withdraw_more_than_own_balance_is_rejected_even_when_contract_has_funds() {
    let env = Env::default();
    let (client, token, _) = setup_env(&env);
    let sac = StellarAssetClient::new(&env, &token.address());

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    fund_user(&env, &client, &token, &sac, &alice, 200);
    fund_user(&env, &client, &token, &sac, &bob, 5_000);

    env.mock_all_auths();
    client.withdraw(&alice, &201);
}

#[test]
#[should_panic]
fn withdraw_with_zero_balance_is_rejected() {
    let env = Env::default();
    let (client, token, _) = setup_env(&env);
    let sac = StellarAssetClient::new(&env, &token.address());

    let rich = Address::generate(&env);
    fund_user(&env, &client, &token, &sac, &rich, 10_000);

    let nobody = Address::generate(&env);
    env.mock_all_auths();
    client.withdraw(&nobody, &1);
}

#[test]
fn first_withdrawal_does_not_corrupt_second_actors_balance_record() {
    let env = Env::default();
    let (client, token, _) = setup_env(&env);
    let sac = StellarAssetClient::new(&env, &token.address());

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    fund_user(&env, &client, &token, &sac, &alice, 1_000);
    fund_user(&env, &client, &token, &sac, &bob, 750);

    env.mock_all_auths();
    client.withdraw(&alice, &1_000);

    assert_eq!(
        client.balance_of(&bob),
        750,
        "bob: balance must be unchanged after alice's withdrawal"
    );
}