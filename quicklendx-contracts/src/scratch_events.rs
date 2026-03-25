use soroban_sdk::{Env, testutils::Events as _};

#[test]
fn test_diagnose_events() {
    let env = Env::default();
    let all = env.events().all();
    let _ = all.events().len();
}
