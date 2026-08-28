#[cfg(test)]
mod tests {
    use crate::payment_token_policy::*;
    const A: TokenAddress = [1; 32];
    const B: TokenAddress = [2; 32];
    const ADMIN: TokenPolicyAdmin = TokenPolicyAdmin { address: [9; 32], version: 1 };
    fn active(t: TokenAddress, v: u32) -> Option<TokenConfig> { Some(TokenConfig { token: t, enabled: true, version: v }) }
    fn removed(t: TokenAddress, v: u32) -> Option<TokenConfig> { Some(TokenConfig { token: t, enabled: false, version: v }) }
    #[test] fn missing_new_payment_fails() { assert!(authorize_payment(None, false).is_err()); }
    #[test] fn missing_funded_record_fails_closed() { assert!(authorize_payment(None, true).is_err()); }
    #[test] fn active_new_payment_succeeds() { assert!(authorize_payment(active(A, 1), false).is_ok()); }
    #[test] fn active_funded_payment_succeeds() { assert!(authorize_payment(active(A, 1), true).is_ok()); }
    #[test] fn removed_new_payment_fails() { assert_eq!(authorize_payment(removed(A, 1), false), Err(TokenPolicyError::Removed)); }
    #[test] fn removed_funded_payment_succeeds() { assert!(authorize_payment(removed(A, 1), true).is_ok()); }
    #[test] fn add_event_contains_token() { let (_, e) = apply_token_update(ADMIN.address, ADMIN, None, A, true, 0).unwrap(); assert_eq!(e.token, A); }
    #[test] fn add_event_contains_versions() { let (_, e) = apply_token_update(ADMIN.address, ADMIN, None, A, true, 0).unwrap(); assert_eq!((e.old_version, e.new_version), (0, 1)); }
    #[test] fn disable_event_contains_versions() { let (_, e) = apply_token_update(ADMIN.address, ADMIN, active(A, 7), A, false, 7).unwrap(); assert_eq!((e.old_version, e.new_version), (7, 8)); }
    #[test] fn unauthorized_remove_cannot_change_state() { assert!(apply_token_update(B, ADMIN, active(A, 1), A, false, 1).is_err()); }
    #[test] fn replayed_add_is_stale() { assert!(apply_token_update(ADMIN.address, ADMIN, active(A, 1), A, true, 0).is_err()); }
    #[test] fn future_version_is_stale() { assert!(apply_token_update(ADMIN.address, ADMIN, active(A, 1), A, false, 2).is_err()); }
    #[test] fn different_token_cannot_share_record() { assert!(apply_token_update(ADMIN.address, ADMIN, active(A, 1), B, true, 1).is_err()); }
    #[test] fn stable_replay_preserves_enabled() { let c = TokenConfig { token: A, enabled: true, version: 2 }; assert!(replay_update_is_noop(Some(c), c, 2)); }
    #[test] fn stable_replay_preserves_removed() { let c = TokenConfig { token: A, enabled: false, version: 2 }; assert!(replay_update_is_noop(Some(c), c, 2)); }
    #[test] fn canonical_a_is_a() { assert_eq!(canonical_token(A), A); }
    #[test] fn canonical_b_is_b() { assert_eq!(canonical_token(B), B); }
    #[test] fn max_version_can_be_read() { assert!(authorize_payment(active(A, u32::MAX), true).is_ok()); }
    #[test] fn max_version_update_fails() { assert_eq!(apply_token_update(ADMIN.address, ADMIN, active(A, u32::MAX), A, false, u32::MAX), Err(TokenPolicyError::InvalidVersion)); }
    #[test] fn funded_removed_record_keeps_version() { assert_eq!(authorize_payment(removed(A, 9), true).unwrap().version, 9); }
    #[test] fn disabled_token_never_authorizes_new_path() { for version in 0..5 { assert!(authorize_payment(removed(A, version), false).is_err()); } }
    #[test] fn enabled_token_authorizes_new_path() { for version in 0..5 { assert!(authorize_payment(active(A, version), false).is_ok()); } }
}
