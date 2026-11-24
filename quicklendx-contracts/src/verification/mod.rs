pub mod business;
pub mod freeze;
pub mod investor;
pub mod tier;
pub mod validation;

pub use business::*;
pub use freeze::*;
pub use investor::*;
pub use tier::*;
pub use validation::*;

use crate::errors::QuickLendXError;
use soroban_sdk::{contracttype, symbol_short, Address, Env, String, TryFromVal, Val, Vec};

/// Namespaced persistent keys for KYC records. Business and investor
/// records must never share the same storage key.
#[contracttype]
#[derive(Clone)]
enum KycStorageKey {
    BusinessRecord(Address),
    InvestorRecord(Address),
}

/// Persistent, indexed KYC status lists.
#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum KycIndexKind {
    VerifiedBusinesses,
    PendingBusinesses,
    RejectedBusinesses,
    VerifiedInvestors,
    PendingInvestors,
    RejectedInvestors,
}

#[contracttype]
#[derive(Clone)]
enum KycIndexKey {
    Count(KycIndexKind),
    Entry(KycIndexKind, u32),
    Sorted(KycIndexKind),
}

fn kyc_index_legacy_key(kind: KycIndexKind) -> &'static str {
    match kind {
        KycIndexKind::VerifiedBusinesses => "verified_businesses",
        KycIndexKind::PendingBusinesses => "pending_businesses",
        KycIndexKind::RejectedBusinesses => "rejected_businesses",
        KycIndexKind::VerifiedInvestors => "verified_investors",
        KycIndexKind::PendingInvestors => "pending_investors",
        KycIndexKind::RejectedInvestors => "rejected_investors",
    }
}

fn kyc_submitted_at(env: &Env, kind: KycIndexKind, address: &Address) -> u64 {
    match kind {
        KycIndexKind::VerifiedBusinesses
        | KycIndexKind::PendingBusinesses
        | KycIndexKind::RejectedBusinesses => {
            BusinessVerificationStorage::get_verification(env, address)
                .map(|record| record.submitted_at)
                .unwrap_or(0)
        }
        KycIndexKind::VerifiedInvestors
        | KycIndexKind::PendingInvestors
        | KycIndexKind::RejectedInvestors => InvestorVerificationStorage::get(env, address)
            .map(|record| record.submitted_at)
            .unwrap_or(0),
    }
}

fn kyc_address_lex_gt(a: &Address, b: &Address) -> bool {
    let a_bytes = a.to_string().to_bytes();
    let b_bytes = b.to_string().to_bytes();
    let common_len = a_bytes.len().min(b_bytes.len());

    for i in 0..common_len {
        let a_byte = a_bytes.get(i).unwrap_or(0);
        let b_byte = b_bytes.get(i).unwrap_or(0);

        if a_byte != b_byte {
            return a_byte > b_byte;
        }
    }

    a_bytes.len() > b_bytes.len()
}

fn ensure_kyc_index_sorted(env: &Env, kind: KycIndexKind) {
    let sorted_key = KycIndexKey::Sorted(kind);

    if env.storage().persistent().has(&sorted_key) {
        crate::storage::extend_persistent_ttl(env, &sorted_key);
        return;
    }

    let count_key = KycIndexKey::Count(kind);
    let mut entries = if let Some(count) = env.storage().persistent().get::<_, u32>(&count_key) {
        let mut result = Vec::new(env);
        for i in 0..count {
            if let Some(address) = env
                .storage()
                .persistent()
                .get::<_, Address>(&KycIndexKey::Entry(kind, i))
            {
                result.push_back(address);
            }
        }
        result
    } else {
        env.storage()
            .instance()
            .get(&kyc_index_legacy_key(kind))
            .unwrap_or(Vec::new(env))
    };

    // One-time insertion sort by (submitted_at ASC, address ASC).
    let mut i = 1u32;
    while i < entries.len() {
        let mut j = i;
        while j > 0 {
            let previous = entries.get(j - 1).unwrap();
            let current = entries.get(j).unwrap();

            let previous_time = kyc_submitted_at(env, kind, &previous);
            let current_time = kyc_submitted_at(env, kind, &current);

            let swap = previous_time > current_time
                || (previous_time == current_time && kyc_address_lex_gt(&previous, &current));

            if !swap {
                break;
            }

            entries.set(j - 1, current);
            entries.set(j, previous);
            j -= 1;
        }
        i += 1;
    }

    // Persist the ordered entries.
    for i in 0..entries.len() {
        let entry_key = KycIndexKey::Entry(kind, i);
        env.storage()
            .persistent()
            .set(&entry_key, &entries.get(i).unwrap());
        crate::storage::extend_persistent_ttl(env, &entry_key);
    }

    let count = entries.len();
    for i in count
        ..env
            .storage()
            .persistent()
            .get::<_, u32>(&count_key)
            .unwrap_or(0)
    {
        env.storage()
            .persistent()
            .remove(&KycIndexKey::Entry(kind, i));
    }

    env.storage().persistent().set(&count_key, &count);
    crate::storage::extend_persistent_ttl(env, &count_key);
    env.storage().instance().remove(&kyc_index_legacy_key(kind));
    env.storage().persistent().set(&sorted_key, &true);
    crate::storage::extend_persistent_ttl(env, &sorted_key);
}

fn get_kyc_index(env: &Env, kind: KycIndexKind) -> Vec<Address> {
    ensure_kyc_index_sorted(env, kind);

    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&KycIndexKey::Count(kind))
        .unwrap_or(0);

    let mut entries = Vec::new(env);
    for i in 0..count {
        let key = KycIndexKey::Entry(kind, i);
        if let Some(address) = env.storage().persistent().get::<_, Address>(&key) {
            crate::storage::extend_persistent_ttl(env, &key);
            entries.push_back(address);
        }
    }

    crate::storage::extend_persistent_ttl(env, &KycIndexKey::Count(kind));
    entries
}

/// Read only the requested page of an ordered persistent KYC index.
/// Returns (page entries, total entry count).
pub(crate) fn get_kyc_index_page(
    env: &Env,
    kind: KycIndexKind,
    offset: u32,
    limit: u32,
) -> (Vec<Address>, u32) {
    ensure_kyc_index_sorted(env, kind);

    let count_key = KycIndexKey::Count(kind);
    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&count_key)
        .unwrap_or(0);

    crate::storage::extend_persistent_ttl(env, &count_key);

    let start = offset.min(count);
    let end = start.saturating_add(limit).min(count);
    let mut page = Vec::new(env);

    for i in start..end {
        let key = KycIndexKey::Entry(kind, i);
        if let Some(address) = env.storage().persistent().get::<_, Address>(&key) {
            crate::storage::extend_persistent_ttl(env, &key);
            page.push_back(address);
        }
    }

    (page, count)
}

fn add_to_kyc_index(env: &Env, kind: KycIndexKind, address: &Address) {
    ensure_kyc_index_sorted(env, kind);

    let count_key = KycIndexKey::Count(kind);
    let mut count = env
        .storage()
        .persistent()
        .get::<_, u32>(&count_key)
        .unwrap_or(0);

    // Avoid duplicate entries.
    for i in 0..count {
        if env
            .storage()
            .persistent()
            .get::<_, Address>(&KycIndexKey::Entry(kind, i))
            .map(|existing| existing == *address)
            .unwrap_or(false)
        {
            return;
        }
    }

    let submitted_at = kyc_submitted_at(env, kind, address);
    let mut insert_at = count;

    // Find the first entry that sorts after the new record.
    for i in 0..count {
        let existing_key = KycIndexKey::Entry(kind, i);
        if let Some(existing) = env.storage().persistent().get::<_, Address>(&existing_key) {
            let existing_time = kyc_submitted_at(env, kind, &existing);
            let comes_after = existing_time > submitted_at
                || (existing_time == submitted_at && kyc_address_lex_gt(&existing, address));

            if comes_after {
                insert_at = i;
                break;
            }
        }
    }

    // Shift entries right to make room.
    let mut i = count;
    while i > insert_at {
        let previous_key = KycIndexKey::Entry(kind, i - 1);
        let destination_key = KycIndexKey::Entry(kind, i);

        if let Some(previous) = env.storage().persistent().get::<_, Address>(&previous_key) {
            env.storage().persistent().set(&destination_key, &previous);
            crate::storage::extend_persistent_ttl(env, &destination_key);
        }

        i -= 1;
    }

    let new_key = KycIndexKey::Entry(kind, insert_at);
    env.storage().persistent().set(&new_key, address);
    crate::storage::extend_persistent_ttl(env, &new_key);

    count += 1;
    env.storage().persistent().set(&count_key, &count);
    crate::storage::extend_persistent_ttl(env, &count_key);
}

fn remove_from_kyc_index(env: &Env, kind: KycIndexKind, address: &Address) {
    ensure_kyc_index_sorted(env, kind);

    let count_key = KycIndexKey::Count(kind);
    let count = env
        .storage()
        .persistent()
        .get::<_, u32>(&count_key)
        .unwrap_or(0);

    let mut write_index = 0u32;

    for read_index in 0..count {
        let read_key = KycIndexKey::Entry(kind, read_index);

        if let Some(existing) = env.storage().persistent().get::<_, Address>(&read_key) {
            if existing != *address {
                if write_index != read_index {
                    let write_key = KycIndexKey::Entry(kind, write_index);
                    env.storage().persistent().set(&write_key, &existing);
                    crate::storage::extend_persistent_ttl(env, &write_key);
                }
                write_index += 1;
            }
        }
    }

    for i in write_index..count {
        env.storage()
            .persistent()
            .remove(&KycIndexKey::Entry(kind, i));
    }

    env.storage().persistent().set(&count_key, &write_index);
    crate::storage::extend_persistent_ttl(env, &count_key);
}

/// Migrates a bounded page of legacy KYC records keyed directly by Address.
///
/// Callers supply the addresses to process, allowing migration to resume
/// across multiple transactions without attempting to enumerate storage.
pub fn migrate_legacy_kyc_page(
    env: &Env,
    admin: &Address,
    addresses: &Vec<Address>,
) -> Result<u32, QuickLendXError> {
    crate::admin::AdminStorage::require_admin_auth(env, admin)?;

    let limit = crate::pagination::MAX_QUERY_LIMIT;
    if addresses.len() > limit {
        return Err(QuickLendXError::InvalidAmount);
    }

    let mut migrated = 0u32;

    for address in addresses.iter() {
        let raw_value: Option<Val> = env.storage().instance().get(&address);
        let Some(raw_value) = raw_value else {
            continue;
        };

        // Decode as a business record first. Failed decoding is handled as
        // a Result, not an unchecked typed storage read.
        if let Ok(record) = BusinessVerification::try_from_val(env, &raw_value) {
            if record.business != address {
                return Err(QuickLendXError::InvalidKYCStatus);
            }

            let key = KycStorageKey::BusinessRecord(address.clone());
            if !env.storage().persistent().has(&key) {
                BusinessVerificationStorage::store_verification(env, &record);
            } else {
                crate::storage::extend_persistent_ttl(env, &key);
            }
            env.storage().instance().remove(&address);
            migrated += 1;
            continue;
        }

        if let Ok(record) = InvestorVerification::try_from_val(env, &raw_value) {
            if record.investor != address {
                return Err(QuickLendXError::InvalidKYCStatus);
            }

            let key = KycStorageKey::InvestorRecord(address.clone());
            if !env.storage().persistent().has(&key) {
                InvestorVerificationStorage::store(env, &record);

                match record.status {
                    BusinessVerificationStatus::Verified => {
                        InvestorVerificationStorage::add_to_verified_investors(env, &address);
                    }
                    BusinessVerificationStatus::Pending => {
                        InvestorVerificationStorage::add_to_pending_investors(env, &address);
                    }
                    BusinessVerificationStatus::Rejected => {
                        InvestorVerificationStorage::add_to_rejected_investors(env, &address);
                    }
                }
            } else {
                crate::storage::extend_persistent_ttl(env, &key);
            }

            env.storage().instance().remove(&address);
            migrated += 1;
            continue;
        }

        // Preserve unrecognized legacy data for investigation; never delete
        // a key that could not be decoded and migrated safely.
        return Err(QuickLendXError::InvalidKYCStatus);
    }

    Ok(migrated)
}
