//! Versioned, admin-controlled payment-token allowlist policy.
pub type TokenAddress = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenConfig { pub token: TokenAddress, pub enabled: bool, pub version: u32, pub request_key: [u8; 32], pub previous_enabled: bool }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenPolicyAdmin { pub address: TokenAddress, pub version: u32 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenConfigEvent { pub token: TokenAddress, pub old_enabled: bool, pub new_enabled: bool, pub old_version: u32, pub new_version: u32 }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenPolicyError { Unsupported, Removed, Unauthorized, VersionConflict, InvalidVersion, InvalidAdmin }

pub fn authorize_token(config: Option<TokenConfig>) -> Result<TokenConfig, TokenPolicyError> {
    let config = config.ok_or(TokenPolicyError::Unsupported)?;
    if config.enabled { Ok(config) } else { Err(TokenPolicyError::Removed) }
}

pub fn authorize_payment(config: Option<TokenConfig>, already_funded: bool) -> Result<TokenConfig, TokenPolicyError> {
    if already_funded { return config.ok_or(TokenPolicyError::Unsupported); }
    authorize_token(config)
}

pub fn apply_token_update(admin: TokenAddress, expected_admin: TokenPolicyAdmin, current: Option<TokenConfig>, token: TokenAddress, enabled: bool, expected_version: u32, request_key: [u8; 32]) -> Result<(TokenConfig, TokenConfigEvent), TokenPolicyError> {
    if admin == [0; 32] || expected_admin.address == [0; 32] { return Err(TokenPolicyError::InvalidAdmin); }
    if admin != expected_admin.address { return Err(TokenPolicyError::Unauthorized); }
    let old = current.unwrap_or(TokenConfig { token, enabled: false, version: 0, request_key: [0; 32], previous_enabled: false });
    
    if old.request_key == request_key && request_key != [0; 32] {
        return Ok((old, TokenConfigEvent { token, old_enabled: old.previous_enabled, new_enabled: old.enabled, old_version: old.version.saturating_sub(1), new_version: old.version }));
    }
    
    if old.token != token || expected_version != old.version { return Err(TokenPolicyError::VersionConflict); }
    let new_version = old.version.checked_add(1).ok_or(TokenPolicyError::InvalidVersion)?;
    let next = TokenConfig { token, enabled, version: new_version, request_key, previous_enabled: old.enabled };
    let event = TokenConfigEvent { token, old_enabled: old.enabled, new_enabled: enabled, old_version: old.version, new_version };
    Ok((next, event))
}


pub fn replay_update_is_noop(current: Option<TokenConfig>, desired: TokenConfig, expected_version: u32) -> bool { current == Some(desired) && desired.version == expected_version }
pub fn canonical_token(token: TokenAddress) -> TokenAddress { token }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenUpdateRequest { pub token: TokenAddress, pub enabled: bool, pub expected_version: u32, pub request_key: [u8; 32] }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenPolicySnapshot { pub configured: u32, pub enabled: u32, pub highest_version: u32, pub digest: u32 }

pub fn validate_update_batch(admin: TokenAddress, expected_admin: TokenPolicyAdmin, updates: &[TokenUpdateRequest]) -> Result<(), TokenPolicyError> {
    if admin == [0; 32] { return Err(TokenPolicyError::InvalidAdmin); }
    if admin != expected_admin.address { return Err(TokenPolicyError::Unauthorized); }
    let mut index = 0;
    while index < updates.len() { if updates[index].token == [0; 32] { return Err(TokenPolicyError::Unsupported); } index += 1; }
    Ok(())
}

pub fn apply_batch_updates(admin: TokenAddress, expected_admin: TokenPolicyAdmin, list: &mut TokenAllowlist, updates: &[TokenUpdateRequest]) -> Result<TokenPolicySnapshot, TokenPolicyError> {
    validate_update_batch(admin, expected_admin, updates)?;
    let mut index = 0;
    while index < updates.len() {
        let update = updates[index];
        let current = list.find(update.token);
        let (next, _) = apply_token_update(admin, expected_admin, current, update.token, update.enabled, update.expected_version, update.request_key)?;
        if current.is_some() { list.replace(next)?; } else { list.insert(next)?; }
        index += 1;
    }
    Ok(snapshot(list))
}

pub fn snapshot(list: &TokenAllowlist) -> TokenPolicySnapshot {
    let mut enabled = 0;
    let mut highest_version = 0;
    let mut digest = 0u32;
    let mut index = 0;
    while index < list.entries.len() { if let Some(config) = list.entries[index] { if config.enabled { enabled += 1; } if config.version > highest_version { highest_version = config.version; } digest = digest.wrapping_add(config.token[0] as u32).wrapping_mul(config.version); } index += 1; }
    TokenPolicySnapshot { configured: list.length, enabled, highest_version, digest }
}

pub fn rotate_admin(caller: TokenAddress, current: TokenPolicyAdmin, next: TokenAddress) -> Result<TokenPolicyAdmin, TokenPolicyError> {
    if caller == [0; 32] || current.address == [0; 32] || next == [0; 32] { return Err(TokenPolicyError::InvalidAdmin); }
    if caller != current.address || caller == next { return Err(TokenPolicyError::Unauthorized); }
    Ok(TokenPolicyAdmin { address: next, version: current.version.checked_add(1).ok_or(TokenPolicyError::InvalidVersion)? })
}

/// Fixed-capacity allowlist adapter suitable for deterministic contract state.
/// The storage layer can map each slot to its token-address record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenAllowlist { pub entries: [Option<TokenConfig>; 8], pub length: u32 }

impl TokenAllowlist {
    pub const fn empty() -> Self { Self { entries: [None; 8], length: 0 } }
    pub fn find(&self, token: TokenAddress) -> Option<TokenConfig> {
        let mut index = 0;
        while index < self.entries.len() { if let Some(config) = self.entries[index] { if config.token == token { return Some(config); } } index += 1; }
        None
    }
    pub fn insert(&mut self, config: TokenConfig) -> Result<(), TokenPolicyError> {
        if self.find(config.token).is_some() { return Err(TokenPolicyError::VersionConflict); }
        let mut index = 0;
        while index < self.entries.len() { if self.entries[index].is_none() { self.entries[index] = Some(config); self.length += 1; return Ok(()); } index += 1; }
        Err(TokenPolicyError::InvalidVersion)
    }
    pub fn replace(&mut self, config: TokenConfig) -> Result<(), TokenPolicyError> {
        let mut index = 0;
        while index < self.entries.len() { if let Some(existing) = self.entries[index] { if existing.token == config.token { if config == existing { return Ok(()); } if config.version != existing.version + 1 { return Err(TokenPolicyError::VersionConflict); } self.entries[index] = Some(config); return Ok(()); } } index += 1; }
        Err(TokenPolicyError::Unsupported)
    }
    pub fn contains_enabled(&self, token: TokenAddress) -> bool { self.find(token).map(|config| config.enabled).unwrap_or(false) }
    pub fn remove(&mut self, token: TokenAddress, expected_version: u32) -> Result<TokenConfig, TokenPolicyError> {
        let current = self.find(token).ok_or(TokenPolicyError::Unsupported)?;
        if current.version != expected_version { return Err(TokenPolicyError::VersionConflict); }
        let next = TokenConfig { token, enabled: false, version: current.version.checked_add(1).ok_or(TokenPolicyError::InvalidVersion)?, request_key: [0xff; 32], previous_enabled: current.enabled };
        self.replace(next)?;
        Ok(next)
    }
    pub fn enabled_count(&self) -> u32 { snapshot(self).enabled }
    pub fn validate(&self) -> Result<(), TokenPolicyError> {
        let mut outer = 0;
        while outer < self.entries.len() { if let Some(left) = self.entries[outer] { let mut inner = outer + 1; while inner < self.entries.len() { if let Some(right) = self.entries[inner] { if left.token == right.token { return Err(TokenPolicyError::VersionConflict); } } inner += 1; } } outer += 1; }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const A: TokenAddress = [1; 32];
    const B: TokenAddress = [2; 32];
    const ADMIN: TokenPolicyAdmin = TokenPolicyAdmin { address: [9; 32], version: 1 };
    #[test] fn missing_is_unsupported() { assert_eq!(authorize_token(None), Err(TokenPolicyError::Unsupported)); }
    #[test] fn enabled_is_supported() { assert!(authorize_token(Some(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false })).is_ok()); }
    #[test] fn disabled_is_removed() { assert_eq!(authorize_token(Some(TokenConfig { token: A, enabled: false, version: 1, request_key: [0; 32], previous_enabled: false })), Err(TokenPolicyError::Removed)); }
    #[test] fn funded_removed_is_readable() { assert!(authorize_payment(Some(TokenConfig { token: A, enabled: false, version: 2, request_key: [0; 32], previous_enabled: false }), true).is_ok()); }
    #[test] fn new_removed_is_rejected() { assert_eq!(authorize_payment(Some(TokenConfig { token: A, enabled: false, version: 2, request_key: [0; 32], previous_enabled: false }), false), Err(TokenPolicyError::Removed)); }
    #[test] fn add_starts_at_one() { assert_eq!(apply_token_update(ADMIN.address, ADMIN, None, A, true, 0, [0; 32]).unwrap().0.version, 1); }
    #[test] fn remove_increments_version() { let (next, _) = apply_token_update(ADMIN.address, ADMIN, Some(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }), A, false, 1, [0; 32]).unwrap(); assert_eq!(next.version, 2); }
    #[test] fn readd_increments_version() { let (next, _) = apply_token_update(ADMIN.address, ADMIN, Some(TokenConfig { token: A, enabled: false, version: 2, request_key: [0; 32], previous_enabled: false }), A, true, 2, [0; 32]).unwrap(); assert_eq!(next.version, 3); }
    #[test] fn wrong_admin_is_rejected() { assert_eq!(apply_token_update(B, ADMIN, None, A, true, 0, [0; 32]), Err(TokenPolicyError::Unauthorized)); }
    #[test] fn zero_admin_is_rejected() { assert_eq!(apply_token_update([0; 32], ADMIN, None, A, true, 0, [0; 32]), Err(TokenPolicyError::InvalidAdmin)); }
    #[test] fn stale_version_is_rejected() { assert_eq!(apply_token_update(ADMIN.address, ADMIN, Some(TokenConfig { token: A, enabled: true, version: 4, request_key: [0; 32], previous_enabled: false }), A, false, 3, [0; 32]), Err(TokenPolicyError::VersionConflict)); }
    #[test] fn wrong_token_is_rejected() { assert_eq!(apply_token_update(ADMIN.address, ADMIN, Some(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }), B, false, 1, [0; 32]), Err(TokenPolicyError::VersionConflict)); }
    #[test] fn old_and_new_values_are_emitted() { let (_, e) = apply_token_update(ADMIN.address, ADMIN, Some(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }), A, false, 1, [0; 32]).unwrap(); assert_eq!((e.old_enabled, e.new_enabled), (true, false)); }
    #[test] fn replay_helper_accepts_exact_record() { let c = TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }; assert!(replay_update_is_noop(Some(c), c, 1)); }
    #[test] fn replay_helper_rejects_version_change() { let c = TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }; assert!(!replay_update_is_noop(Some(c), c, 2)); }
    #[test] fn canonicalization_is_lossless() { assert_eq!(canonical_token(A), A); }
    #[test] fn empty_allowlist_has_no_entries() { let list = TokenAllowlist::empty(); assert_eq!(list.length, 0); assert_eq!(list.find(A), None); }
    #[test] fn allowlist_inserts_new_token() { let mut list = TokenAllowlist::empty(); assert!(list.insert(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }).is_ok()); assert_eq!(list.length, 1); }
    #[test] fn allowlist_rejects_duplicate_insert() { let mut list = TokenAllowlist::empty(); list.insert(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }).unwrap(); assert_eq!(list.insert(TokenConfig { token: A, enabled: false, version: 1, request_key: [0; 32], previous_enabled: false }), Err(TokenPolicyError::VersionConflict)); }
    #[test] fn allowlist_replaces_next_version() { let mut list = TokenAllowlist::empty(); list.insert(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }).unwrap(); assert!(list.replace(TokenConfig { token: A, enabled: false, version: 2, request_key: [0; 32], previous_enabled: false }).is_ok()); assert_eq!(list.find(A).unwrap().enabled, false); }
    #[test] fn allowlist_rejects_skipped_version() { let mut list = TokenAllowlist::empty(); list.insert(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }).unwrap(); assert_eq!(list.replace(TokenConfig { token: A, enabled: false, version: 3, request_key: [0; 32], previous_enabled: false }), Err(TokenPolicyError::VersionConflict)); }
    #[test] fn allowlist_replace_unknown_is_unsupported() { let mut list = TokenAllowlist::empty(); assert_eq!(list.replace(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }), Err(TokenPolicyError::Unsupported)); }
    #[test] fn empty_snapshot_is_zeroed() { assert_eq!(snapshot(&TokenAllowlist::empty()), TokenPolicySnapshot { configured: 0, enabled: 0, highest_version: 0, digest: 0 }); }
    #[test] fn batch_adds_multiple_tokens() { let mut list = TokenAllowlist::empty(); let updates = [TokenUpdateRequest { token: A, enabled: true, expected_version: 0, request_key: [0; 32] }, TokenUpdateRequest { token: B, enabled: true, expected_version: 0, request_key: [0; 32] }]; let result = apply_batch_updates(ADMIN.address, ADMIN, &mut list, &updates).unwrap(); assert_eq!((result.configured, result.enabled), (2, 2)); }
    #[test] fn batch_rejects_zero_token() { let mut list = TokenAllowlist::empty(); let updates = [TokenUpdateRequest { token: [0; 32], enabled: true, expected_version: 0, request_key: [0; 32] }]; assert_eq!(validate_update_batch(ADMIN.address, ADMIN, &updates), Err(TokenPolicyError::Unsupported)); assert_eq!(list.length, 0); }
    #[test] fn batch_rejects_wrong_admin() { let mut list = TokenAllowlist::empty(); let updates = [TokenUpdateRequest { token: A, enabled: true, expected_version: 0, request_key: [0; 32] }]; assert_eq!(apply_batch_updates(B, ADMIN, &mut list, &updates), Err(TokenPolicyError::Unauthorized)); assert_eq!(list.length, 0); }
    #[test] fn rotate_admin_changes_identity() { let next = rotate_admin(ADMIN.address, ADMIN, B).unwrap(); assert_eq!((next.address, next.version), (B, 2)); }
    #[test] fn rotate_admin_rejects_non_admin() { assert_eq!(rotate_admin(B, ADMIN, [3; 32]), Err(TokenPolicyError::Unauthorized)); }
    #[test] fn rotate_admin_rejects_self_rotation() { assert_eq!(rotate_admin(ADMIN.address, ADMIN, ADMIN.address), Err(TokenPolicyError::Unauthorized)); }
    #[test] fn rotate_admin_rejects_zero_next() { assert_eq!(rotate_admin(ADMIN.address, ADMIN, [0; 32]), Err(TokenPolicyError::InvalidAdmin)); }
    #[test] fn contains_enabled_distinguishes_disabled() { let mut list = TokenAllowlist::empty(); list.insert(TokenConfig { token: A, enabled: false, version: 1, request_key: [0; 32], previous_enabled: false }).unwrap(); assert!(!list.contains_enabled(A)); }
    #[test] fn contains_enabled_returns_false_for_unknown() { assert!(!TokenAllowlist::empty().contains_enabled(A)); }
    #[test] fn remove_increments_version() { let mut list = TokenAllowlist::empty(); list.insert(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }).unwrap(); let removed = list.remove(A, 1).unwrap(); assert_eq!(removed.version, 2); assert!(!list.contains_enabled(A)); }
    #[test] fn remove_requires_current_version() { let mut list = TokenAllowlist::empty(); list.insert(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }).unwrap(); assert_eq!(list.remove(A, 0), Err(TokenPolicyError::VersionConflict)); }
    #[test] fn remove_unknown_is_unsupported() { let mut list = TokenAllowlist::empty(); assert_eq!(list.remove(A, 0), Err(TokenPolicyError::Unsupported)); }
    #[test] fn enabled_count_uses_snapshot() { let mut list = TokenAllowlist::empty(); list.insert(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }).unwrap(); list.insert(TokenConfig { token: B, enabled: false, version: 1, request_key: [0; 32], previous_enabled: false }).unwrap(); assert_eq!(list.enabled_count(), 1); }
    #[test] fn empty_allowlist_validates() { assert!(TokenAllowlist::empty().validate().is_ok()); }
    #[test] fn distinct_allowlist_entries_validate() { let mut list = TokenAllowlist::empty(); list.insert(TokenConfig { token: A, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }).unwrap(); list.insert(TokenConfig { token: B, enabled: true, version: 1, request_key: [0; 32], previous_enabled: false }).unwrap(); assert!(list.validate().is_ok()); }
}
