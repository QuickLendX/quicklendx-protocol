use quicklendx_contracts::{
    errors::QuickLendXError,
    notifications::{NotificationPriority, NotificationSystem, NotificationType},
    QuickLendXContract,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

const SECONDS_PER_DAY: u32 = 86_400;

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

fn with_contract<R>(env: &Env, contract_id: &Address, f: impl FnOnce(&Env) -> R) -> R {
    env.as_contract(contract_id, || f(env))
}

fn create_general_notification(
    env: &Env,
    recipient: &Address,
    priority: NotificationPriority,
) -> Result<soroban_sdk::BytesN<32>, QuickLendXError> {
    NotificationSystem::create_notification(
        env,
        recipient.clone(),
        NotificationType::General,
        priority,
        String::from_str(env, "Quiet hours"),
        String::from_str(env, "Preference test"),
        None,
    )
}

fn quiet_window(env: &Env, start_seconds: u32, end_seconds: u32) -> Vec<u32> {
    let mut window = Vec::new(env);
    window.push_back(start_seconds);
    window.push_back(end_seconds);
    window
}

fn assert_preferences_updated(result: Result<(), QuickLendXError>) {
    assert!(
        result.is_ok(),
        "valid quiet-hours preferences should update"
    );
}

#[test]
fn quiet_hours_default_to_none() {
    let (env, contract_id) = setup();
    with_contract(&env, &contract_id, |env| {
        let user = Address::generate(env);
        let prefs = NotificationSystem::get_user_preferences(env, &user);
        assert_eq!(prefs.quiet_window, None);
    });
}

#[test]
fn quiet_hours_block_non_critical_priority_inside_standard_window() {
    let (env, contract_id) = setup();
    with_contract(&env, &contract_id, |env| {
        env.ledger().set_timestamp(9 * 60 * 60);
        let user = Address::generate(env);

        let mut prefs = NotificationSystem::get_user_preferences(env, &user);
        prefs.general = true;
        prefs.quiet_window = Some(quiet_window(env, 8 * 60 * 60, 10 * 60 * 60));
        assert_preferences_updated(NotificationSystem::update_user_preferences(
            env, &user, prefs,
        ));

        let result = create_general_notification(env, &user, NotificationPriority::High);
        assert!(matches!(result, Err(QuickLendXError::NotificationBlocked)));
    });
}

#[test]
fn quiet_hours_allow_notification_outside_window() {
    let (env, contract_id) = setup();
    with_contract(&env, &contract_id, |env| {
        env.ledger().set_timestamp(11 * 60 * 60);
        let user = Address::generate(env);

        let mut prefs = NotificationSystem::get_user_preferences(env, &user);
        prefs.general = true;
        prefs.quiet_window = Some(quiet_window(env, 8 * 60 * 60, 10 * 60 * 60));
        assert_preferences_updated(NotificationSystem::update_user_preferences(
            env, &user, prefs,
        ));

        let result = create_general_notification(env, &user, NotificationPriority::High);
        assert!(result.is_ok());
    });
}

#[test]
fn quiet_hours_wraparound_window_blocks_after_start() {
    let (env, contract_id) = setup();
    with_contract(&env, &contract_id, |env| {
        env.ledger().set_timestamp(23 * 60 * 60);
        let user = Address::generate(env);

        let mut prefs = NotificationSystem::get_user_preferences(env, &user);
        prefs.general = true;
        prefs.quiet_window = Some(quiet_window(env, 22 * 60 * 60, 6 * 60 * 60));
        assert_preferences_updated(NotificationSystem::update_user_preferences(
            env, &user, prefs,
        ));

        let result = create_general_notification(env, &user, NotificationPriority::Medium);
        assert!(matches!(result, Err(QuickLendXError::NotificationBlocked)));
    });
}

#[test]
fn quiet_hours_allow_critical_priority_inside_window() {
    let (env, contract_id) = setup();
    with_contract(&env, &contract_id, |env| {
        env.ledger().set_timestamp(9 * 60 * 60);
        let user = Address::generate(env);

        let mut prefs = NotificationSystem::get_user_preferences(env, &user);
        prefs.general = true;
        prefs.quiet_window = Some(quiet_window(env, 8 * 60 * 60, 10 * 60 * 60));
        assert_preferences_updated(NotificationSystem::update_user_preferences(
            env, &user, prefs,
        ));

        let result = create_general_notification(env, &user, NotificationPriority::Critical);
        assert!(result.is_ok());
    });
}

#[test]
fn quiet_hours_reject_bounds_at_day_length() {
    let (env, contract_id) = setup();
    with_contract(&env, &contract_id, |env| {
        let user = Address::generate(env);

        let mut prefs = NotificationSystem::get_user_preferences(env, &user);
        prefs.quiet_window = Some(quiet_window(env, 0, SECONDS_PER_DAY));

        let result = NotificationSystem::update_user_preferences(env, &user, prefs);
        assert!(matches!(result, Err(QuickLendXError::InvalidTimestamp)));
    });
}

#[test]
fn quiet_hours_reject_wrong_window_arity() {
    let (env, contract_id) = setup();
    with_contract(&env, &contract_id, |env| {
        let user = Address::generate(env);

        let mut window = Vec::new(env);
        window.push_back(8 * 60 * 60);

        let mut prefs = NotificationSystem::get_user_preferences(env, &user);
        prefs.quiet_window = Some(window);

        let result = NotificationSystem::update_user_preferences(env, &user, prefs);
        assert!(matches!(result, Err(QuickLendXError::InvalidTimestamp)));
    });
}
