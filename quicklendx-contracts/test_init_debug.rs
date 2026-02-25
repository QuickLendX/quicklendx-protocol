use soroban_sdk::{Env, Address, testutils::Address as _};
pub fn test() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    println!("hello");
}
