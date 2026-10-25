#![cfg(test)]

use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, BytesN, Env, String};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    (client, admin)
}

#[test]
fn manual_pause_exposes_reason_and_unpause_clears_it() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    assert_eq!(client.pause_reason(), None);
    client.pause(&admin);
    assert_eq!(
        client.pause_reason(),
        Some(crate::pause::PauseReason::Manual)
    );
    client.unpause(&admin);
    assert_eq!(client.pause_reason(), None);
}

#[test]
fn incident_pause_exposes_reason() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.enter_incident_mode(&admin, &String::from_str(&env, "incident"));
    assert_eq!(
        client.pause_reason(),
        Some(crate::pause::PauseReason::Incident)
    );
}

#[test]
fn pending_upgrade_pause_exposes_reason() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.schedule_upgrade(&admin, &BytesN::from_array(&env, &[7; 32]));
    assert_eq!(
        client.pause_reason(),
        Some(crate::pause::PauseReason::PendingUpgrade)
    );
}

#[test]
fn manual_pause_overwrites_an_existing_reason() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.enter_incident_mode(&admin, &String::from_str(&env, "incident"));
    client.pause(&admin);

    assert_eq!(
        client.pause_reason(),
        Some(crate::pause::PauseReason::Manual)
    );
}

#[test]
fn entrypoint_pause_list_matches_guarded_entrypoints() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let guarded = [
        "store_invoice",
        "verify_invoice",
        "place_bid",
        "accept_bid",
        "verify_business",
        "verify_investor",
        "create_dispute",
        "resolve_dispute",
    ];

    for name in guarded {
        let name = String::from_str(&env, name);
        assert!(!client.is_entrypoint_paused(&name));
    }
    client.pause(&admin);
    for name in guarded {
        let name = String::from_str(&env, name);
        assert!(client.is_entrypoint_paused(&name));
    }
    for name in ["upload_invoice", "get_invoice"] {
        assert!(!client.is_entrypoint_paused(&String::from_str(&env, name)));
    }
}

#[test]
fn is_paused_has_no_side_effects_after_seven_days() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    client.pause(&admin);

    let original_flag = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get::<_, bool>(&crate::pause::PAUSED_KEY)
            .unwrap()
    });
    env.ledger().set_timestamp(env.ledger().timestamp() + 7 * 24 * 3600 + 1);
    assert!(client.is_paused());
    let flag_after_read = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get::<_, bool>(&crate::pause::PAUSED_KEY)
            .unwrap()
    });
    assert_eq!(flag_after_read, original_flag);
}
