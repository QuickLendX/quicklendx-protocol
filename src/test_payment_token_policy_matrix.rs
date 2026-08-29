#[cfg(test)]
mod matrix {
    use crate::payment_token_policy::*;
    const ADMIN: TokenPolicyAdmin = TokenPolicyAdmin { address: [9; 32], version: 1 };
    fn token(n: u8) -> TokenAddress { [n; 32] }
    fn config(n: u8, enabled: bool, version: u32) -> TokenConfig { TokenConfig { token: token(n), enabled, version } }

    #[test] fn matrix_missing_is_rejected() { assert_eq!(authorize_token(None), Err(TokenPolicyError::Unsupported)); }
    #[test] fn matrix_zero_address_is_not_a_valid_admin() { assert_eq!(apply_token_update([0; 32], ADMIN, None, token(1), true, 0), Err(TokenPolicyError::InvalidAdmin)); }
    #[test] fn matrix_wrong_admin_is_not_authorized() { assert_eq!(apply_token_update(token(8), ADMIN, None, token(1), true, 0), Err(TokenPolicyError::Unauthorized)); }
    #[test] fn matrix_first_enable_has_version_one() { assert_eq!(apply_token_update(ADMIN.address, ADMIN, None, token(1), true, 0).unwrap().0.version, 1); }
    #[test] fn matrix_first_disable_has_version_one() { assert_eq!(apply_token_update(ADMIN.address, ADMIN, None, token(1), false, 0).unwrap().0.version, 1); }
    #[test] fn matrix_enable_to_disable_is_monotonic() { let r = apply_token_update(ADMIN.address, ADMIN, Some(config(1, true, 5)), token(1), false, 5).unwrap(); assert_eq!(r.0.version, 6); }
    #[test] fn matrix_disable_to_enable_is_monotonic() { let r = apply_token_update(ADMIN.address, ADMIN, Some(config(1, false, 5)), token(1), true, 5).unwrap(); assert_eq!(r.0.version, 6); }
    #[test] fn matrix_wrong_expected_version_fails() { assert!(apply_token_update(ADMIN.address, ADMIN, Some(config(1, true, 5)), token(1), false, 4).is_err()); }
    #[test] fn matrix_wrong_token_fails() { assert!(apply_token_update(ADMIN.address, ADMIN, Some(config(1, true, 5)), token(2), false, 5).is_err()); }
    #[test] fn matrix_event_contains_disable() { let (_, e) = apply_token_update(ADMIN.address, ADMIN, Some(config(1, true, 5)), token(1), false, 5).unwrap(); assert_eq!((e.old_enabled, e.new_enabled), (true, false)); }
    #[test] fn matrix_event_contains_enable() { let (_, e) = apply_token_update(ADMIN.address, ADMIN, Some(config(1, false, 5)), token(1), true, 5).unwrap(); assert_eq!((e.old_enabled, e.new_enabled), (false, true)); }
    #[test] fn matrix_event_contains_old_version() { let (_, e) = apply_token_update(ADMIN.address, ADMIN, Some(config(1, true, 12)), token(1), false, 12).unwrap(); assert_eq!(e.old_version, 12); }
    #[test] fn matrix_event_contains_new_version() { let (_, e) = apply_token_update(ADMIN.address, ADMIN, Some(config(1, true, 12)), token(1), false, 12).unwrap(); assert_eq!(e.new_version, 13); }
    #[test] fn matrix_event_contains_address() { let (_, e) = apply_token_update(ADMIN.address, ADMIN, Some(config(1, true, 1)), token(1), false, 1).unwrap(); assert_eq!(e.token, token(1)); }
    #[test] fn matrix_funded_enabled_succeeds() { assert!(authorize_payment(Some(config(1, true, 1)), true).is_ok()); }
    #[test] fn matrix_funded_disabled_succeeds() { assert!(authorize_payment(Some(config(1, false, 1)), true).is_ok()); }
    #[test] fn matrix_new_enabled_succeeds() { assert!(authorize_payment(Some(config(1, true, 1)), false).is_ok()); }
    #[test] fn matrix_new_disabled_fails() { assert_eq!(authorize_payment(Some(config(1, false, 1)), false), Err(TokenPolicyError::Removed)); }
    #[test] fn matrix_missing_funded_fails() { assert_eq!(authorize_payment(None, true), Err(TokenPolicyError::Unsupported)); }
    #[test] fn matrix_missing_new_fails() { assert_eq!(authorize_payment(None, false), Err(TokenPolicyError::Unsupported)); }
    #[test] fn matrix_funded_version_is_preserved() { assert_eq!(authorize_payment(Some(config(1, false, 33)), true).unwrap().version, 33); }
    #[test] fn matrix_new_version_is_preserved() { assert_eq!(authorize_payment(Some(config(1, true, 33)), false).unwrap().version, 33); }
    #[test] fn matrix_allowlist_insert_and_find() { let mut list = TokenAllowlist::empty(); list.insert(config(1, true, 1)).unwrap(); assert_eq!(list.find(token(1)), Some(config(1, true, 1))); }
    #[test] fn matrix_allowlist_find_unknown() { let mut list = TokenAllowlist::empty(); list.insert(config(1, true, 1)).unwrap(); assert_eq!(list.find(token(2)), None); }
    #[test] fn matrix_allowlist_insert_two() { let mut list = TokenAllowlist::empty(); list.insert(config(1, true, 1)).unwrap(); list.insert(config(2, true, 1)).unwrap(); assert_eq!(list.length, 2); }
    #[test] fn matrix_allowlist_duplicate_is_not_overwritten() { let mut list = TokenAllowlist::empty(); list.insert(config(1, true, 1)).unwrap(); assert!(list.insert(config(1, false, 2)).is_err()); assert_eq!(list.find(token(1)).unwrap().enabled, true); }
    #[test] fn matrix_allowlist_replace_disable() { let mut list = TokenAllowlist::empty(); list.insert(config(1, true, 1)).unwrap(); list.replace(config(1, false, 2)).unwrap(); assert_eq!(list.find(token(1)).unwrap(), config(1, false, 2)); }
    #[test] fn matrix_allowlist_replace_enable() { let mut list = TokenAllowlist::empty(); list.insert(config(1, false, 1)).unwrap(); list.replace(config(1, true, 2)).unwrap(); assert_eq!(list.find(token(1)).unwrap(), config(1, true, 2)); }
    #[test] fn matrix_allowlist_replace_replay_fails() { let mut list = TokenAllowlist::empty(); list.insert(config(1, true, 1)).unwrap(); assert!(list.replace(config(1, true, 1)).is_err()); }
    #[test] fn matrix_allowlist_replace_future_fails() { let mut list = TokenAllowlist::empty(); list.insert(config(1, true, 1)).unwrap(); assert!(list.replace(config(1, false, 4)).is_err()); }
    #[test] fn matrix_allowlist_unknown_replace_fails() { let mut list = TokenAllowlist::empty(); assert!(list.replace(config(1, true, 1)).is_err()); }
    #[test] fn matrix_allowlist_capacity_is_bounded() { let mut list = TokenAllowlist::empty(); for n in 1..=8 { assert!(list.insert(config(n, true, 1)).is_ok()); } assert_eq!(list.length, 8); }
    #[test] fn matrix_allowlist_ninth_insert_fails() { let mut list = TokenAllowlist::empty(); for n in 1..=8 { list.insert(config(n, true, 1)).unwrap(); } assert!(list.insert(config(9, true, 1)).is_err()); }
    #[test] fn matrix_canonical_addresses_round_trip() { for n in 1..=20 { assert_eq!(canonical_token(token(n)), token(n)); } }
    #[test] fn matrix_replay_helper_checks_config() { assert!(replay_update_is_noop(Some(config(1, true, 2)), config(1, true, 2), 2)); assert!(!replay_update_is_noop(Some(config(1, false, 2)), config(1, true, 2), 2)); }
    #[test] fn matrix_replay_helper_checks_version() { assert!(!replay_update_is_noop(Some(config(1, true, 2)), config(1, true, 2), 3)); }
    #[test] fn matrix_replay_helper_checks_token() { assert!(!replay_update_is_noop(Some(config(1, true, 2)), config(2, true, 2), 2)); }
}
