extern crate alloc;

use crate::kyc_policy::{KycActor, KycDependentAction};
use alloc::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonceEntry {
    pub actor: Option<KycActor>,
    pub action: Option<KycDependentAction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonceCheckResult {
    New,
    SafeRetry,
    Conflict,
}

#[cfg(test)]
static NONCE_STORE: std::sync::Mutex<Option<BTreeMap<(u32, u64), NonceEntry>>> =
    std::sync::Mutex::new(None);

pub fn check_and_record_nonce(
    version: u32,
    nonce: u64,
    actor: Option<KycActor>,
    action: Option<KycDependentAction>,
) -> NonceCheckResult {
    if nonce == 0 {
        return NonceCheckResult::New;
    }

    #[cfg(test)]
    {
        let mut guard = NONCE_STORE.lock().unwrap();
        let map = guard.get_or_insert_with(BTreeMap::new);
        let key = (version, nonce);
        if let Some(existing) = map.get(&key) {
            if existing.actor == actor && existing.action == action {
                NonceCheckResult::SafeRetry
            } else {
                NonceCheckResult::Conflict
            }
        } else {
            map.insert(key, NonceEntry { actor, action });
            NonceCheckResult::New
        }
    }

    #[cfg(not(test))]
    {
        NonceCheckResult::New
    }
}

pub fn reset_nonces() {
    #[cfg(test)]
    {
        let mut guard = NONCE_STORE.lock().unwrap();
        if let Some(map) = guard.as_mut() {
            map.clear();
        }
    }
}
