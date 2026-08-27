#[cfg(test)]
mod tests {
    use crate::payment_token_policy::*;
    const ADMIN: TokenPolicyAdmin = TokenPolicyAdmin { address: [9; 32], version: 1 };
    fn c(token: u8, enabled: bool, version: u32) -> TokenConfig { TokenConfig { token: [token; 32], enabled, version } }

    #[test]
    fn regression_first_asset_is_visible() {
        let mut list = TokenAllowlist::empty();
        list.insert(c(1, true, 1)).unwrap();
        assert_eq!(list.find([1; 32]), Some(c(1, true, 1)));
    }
    #[test]
    fn regression_second_asset_is_visible() {
        let mut list = TokenAllowlist::empty();
        list.insert(c(2, true, 1)).unwrap();
        assert_eq!(list.find([2; 32]), Some(c(2, true, 1)));
    }
    #[test]
    fn regression_remove_is_not_delete() {
        let mut list = TokenAllowlist::empty();
        list.insert(c(3, true, 1)).unwrap();
        list.remove([3; 32], 1).unwrap();
        assert_eq!(list.length, 1);
    }
    #[test]
    fn regression_remove_requires_identity() {
        let mut list = TokenAllowlist::empty();
        list.insert(c(4, true, 1)).unwrap();
        assert!(list.remove([5; 32], 1).is_err());
    }
    #[test]
    fn regression_replace_requires_sequential_version() {
        let mut list = TokenAllowlist::empty();
        list.insert(c(5, true, 2)).unwrap();
        assert!(list.replace(c(5, false, 3)).is_ok());
    }
    #[test]
    fn regression_replace_rejects_version_gap() {
        let mut list = TokenAllowlist::empty();
        list.insert(c(6, true, 2)).unwrap();
        assert!(list.replace(c(6, false, 4)).is_err());
    }
    #[test]
    fn regression_snapshot_counts_slots() {
        let mut list = TokenAllowlist::empty();
        list.insert(c(7, false, 1)).unwrap();
        assert_eq!(snapshot(&list).configured, 1);
    }
    #[test]
    fn regression_snapshot_excludes_disabled() {
        let mut list = TokenAllowlist::empty();
        list.insert(c(8, false, 1)).unwrap();
        assert_eq!(snapshot(&list).enabled, 0);
    }
    #[test]
    fn regression_snapshot_includes_enabled() {
        let mut list = TokenAllowlist::empty();
        list.insert(c(9, true, 1)).unwrap();
        assert_eq!(snapshot(&list).enabled, 1);
    }
    #[test]
    fn regression_batch_returns_snapshot() {
        let mut list = TokenAllowlist::empty();
        let request = [TokenUpdateRequest { token: [10; 32], enabled: true, expected_version: 0 }];
        let result = apply_batch_updates(ADMIN.address, ADMIN, &mut list, &request).unwrap();
        assert_eq!(result, snapshot(&list));
    }
    #[test]
    fn regression_batch_does_not_accept_zero_asset() {
        let mut list = TokenAllowlist::empty();
        let request = [TokenUpdateRequest { token: [0; 32], enabled: false, expected_version: 0 }];
        assert!(apply_batch_updates(ADMIN.address, ADMIN, &mut list, &request).is_err());
    }
    #[test]
    fn regression_rotation_updates_version() {
        let rotated = rotate_admin(ADMIN.address, ADMIN, [11; 32]).unwrap();
        assert_eq!(rotated.version, ADMIN.version + 1);
    }
    #[test]
    fn regression_rotation_updates_address() {
        let rotated = rotate_admin(ADMIN.address, ADMIN, [12; 32]).unwrap();
        assert_eq!(rotated.address, [12; 32]);
    }
    #[test]
    fn regression_rotation_rejects_same_address() {
        assert!(rotate_admin(ADMIN.address, ADMIN, ADMIN.address).is_err());
    }
    #[test]
    fn regression_rotation_rejects_zero_address() {
        assert!(rotate_admin(ADMIN.address, ADMIN, [0; 32]).is_err());
    }
    #[test]
    fn regression_payment_accepts_uppercase_independent_policy() {
        assert!(authorize_payment(Some(c(13, true, 7)), false).is_ok());
    }
    #[test]
    fn regression_payment_rejects_disabled_policy() {
        assert!(authorize_payment(Some(c(14, false, 7)), false).is_err());
    }
    #[test]
    fn regression_funded_payment_reads_disabled_policy() {
        assert!(authorize_payment(Some(c(15, false, 7)), true).is_ok());
    }
    #[test]
    fn regression_funded_payment_keeps_token() {
        assert_eq!(authorize_payment(Some(c(16, false, 7)), true).unwrap().token, [16; 32]);
    }
    #[test]
    fn regression_allowlist_validation_passes_unique_assets() {
        let mut list = TokenAllowlist::empty();
        list.insert(c(17, true, 1)).unwrap();
        list.insert(c(18, true, 1)).unwrap();
        assert!(list.validate().is_ok());
    }
    #[test]
    fn regression_enabled_count_matches_snapshot() {
        let mut list = TokenAllowlist::empty();
        list.insert(c(19, true, 1)).unwrap();
        list.insert(c(20, false, 1)).unwrap();
        assert_eq!(list.enabled_count(), snapshot(&list).enabled);
    }
}
