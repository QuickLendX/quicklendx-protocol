use soroban_sdk::{testutils::Events as _, Env};

#[test]
fn test_diagnose_events() {
    let env = Env::default();
    let all = env.events().all();
    let _ = all.events().len();
}
