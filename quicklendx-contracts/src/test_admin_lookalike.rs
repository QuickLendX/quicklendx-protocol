
use soroban_sdk::testutils::Address as _;
#[test]
fn test_generated_address_exists() {
    let env = Env::default();
    let addr = Address::generate(&env);
    println!("Address exists: {}", addr.exists());
}
