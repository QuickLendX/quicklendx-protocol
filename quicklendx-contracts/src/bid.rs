use core::cmp::Ordering;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol, Vec};

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::events::{emit_bid_expired, emit_bid_expiry_grace_updated, emit_bid_ttl_updated};
use crate::storage::{bump_persistent, extend_persistent_ttl};
pub use crate::types::{Bid, BidStatus};

/// Storage keys for indexed bid lookups.
///
/// Uses an indexed layout (Count + Entry) to avoid O(n) read+write on vectors
/// during hot paths like placing bids.
#[derive(Clone)]
#[contracttype]
pub enum BidIndexKey {
    Count(BytesN<32>),
    Entry(BytesN<32>, u32),
    AllBidsCount,
    AllBidsEntry(u32),
    InvestorBidsCount(Address),
    InvestorBidsEntry(Address, u32),
}

// --- Bid TTL configuration ----------------------------------------------------
pub const DEFAULT_BID_TTL_DAYS: u64 = 7;
pub const MIN_BID_TTL_DAYS: u64 = 1;
pub const MAX_BID_TTL_DAYS: u64 = 30;
const BID_TTL_KEY: Symbol = symbol_short!("bid_ttl");
const MAX_ACTIVE_BIDS_PER_INVESTOR_KEY: Symbol = symbol_short!("mx_actbd");
const DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR: u32 = 20;
const SECONDS_PER_DAY: u64 = 86400;

// --- Bid expiry grace period -------------------------------------------------
pub const DEFAULT_BID_EXPIRY_GRACE_SECONDS: u64 = 0;
pub const MIN_BID_EXPIRY_GRACE_SECONDS: u64 = 0;
pub const MAX_BID_EXPIRY_GRACE_SECONDS: u64 = 30 * SECONDS_PER_DAY;
const BID_EXPIRY_GRACE_KEY: Symbol = symbol_short!("bid_grace");

pub const MAX_BIDS_PER_INVOICE: u32 = 50;
pub const INVESTOR_BID_LIMIT_DISABLED: u32 = 0;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BidTtlConfig {
    pub current_days: u64,
    pub min_days: u64,
    pub max_days: u64,
    pub default_days: u64,
    pub is_custom: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BidExpiryGraceConfig {
    pub current_seconds: u64,
    pub min_seconds: u64,
    pub max_seconds: u64,
    pub default_seconds: u64,
    pub is_custom: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BidLimitConfig {
    pub limit: u32,
    pub default_limit: u32,
    pub is_disabled: bool,
    pub is_custom: bool,
}

impl Bid {
    pub fn is_expired(&self, current_timestamp: u64) -> bool {
        current_timestamp >= self.expiration_timestamp
    }

    pub fn is_cleanup_eligible(&self, current_timestamp: u64, grace_seconds: u64) -> bool {
        current_timestamp >= self.expiration_timestamp.saturating_add(grace_seconds)
    }

    pub fn default_expiration(now: u64) -> u64 {
        now.saturating_add(DEFAULT_BID_TTL_DAYS.saturating_mul(SECONDS_PER_DAY))
    }

    pub fn default_expiration_with_env(env: &Env, now: u64) -> u64 {
        let days = BidStorage::get_bid_ttl_days(env);
        now.saturating_add(days.saturating_mul(SECONDS_PER_DAY))
    }
}

impl BidStatus {
    pub fn validate_transition(from: &BidStatus, to: &BidStatus) -> Result<(), QuickLendXError> {
        let allowed = match from {
            BidStatus::Placed => matches!(
                to,
                BidStatus::Accepted
                    | BidStatus::Withdrawn
                    | BidStatus::Cancelled
                    | BidStatus::Expired
            ),
            BidStatus::Accepted => matches!(to, BidStatus::Cancelled),
            _ => false,
        };
        if allowed {
            Ok(())
        } else {
            Err(QuickLendXError::InvalidStatus)
        }
    }
}

pub struct BidStorage;

impl BidStorage {
    pub fn get_all_bids(env: &Env) -> Vec<BytesN<32>> {
        let count_key = BidIndexKey::AllBidsCount;
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        if count > 0 {
            bump_persistent(env, &count_key);
        }
        let mut bids = Vec::new(env);
        let mut idx: u32 = 0;
        while idx < count {
            let entry_key = BidIndexKey::AllBidsEntry(idx);
            if let Some(bid_id) = env.storage().persistent().get(&entry_key) {
                bump_persistent(env, &entry_key);
                bids.push_back(bid_id);
            }
            idx += 1;
        }
        bids
    }

    fn add_to_all_bids(env: &Env, bid_id: &BytesN<32>) {
        let count_key = BidIndexKey::AllBidsCount;
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        let entry_key = BidIndexKey::AllBidsEntry(count);
        env.storage().persistent().set(&entry_key, bid_id);
        bump_persistent(env, &entry_key);
        env.storage().persistent().set(&count_key, &(count + 1));
        bump_persistent(env, &count_key);
    }

    fn invoice_bid_count_key(invoice_id: &BytesN<32>) -> BidIndexKey {
        BidIndexKey::Count(invoice_id.clone())
    }

    fn invoice_bid_entry_key(invoice_id: &BytesN<32>, index: u32) -> BidIndexKey {
        BidIndexKey::Entry(invoice_id.clone(), index)
    }

    pub fn get_bids_by_investor_all(env: &Env, investor: &Address) -> Vec<BytesN<32>> {
        let count_key = BidIndexKey::InvestorBidsCount(investor.clone());
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        if count > 0 {
            bump_persistent(env, &count_key);
        }
        let mut bids = Vec::new(env);
        let mut idx: u32 = 0;
        while idx < count {
            let entry_key = BidIndexKey::InvestorBidsEntry(investor.clone(), idx);
            if let Some(bid_id) = env.storage().persistent().get(&entry_key) {
                bump_persistent(env, &entry_key);
                bids.push_back(bid_id);
            }
            idx += 1;
        }
        bids
    }

    fn add_to_investor_bids(env: &Env, investor: &Address, bid_id: &BytesN<32>) {
        let count_key = BidIndexKey::InvestorBidsCount(investor.clone());
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        let entry_key = BidIndexKey::InvestorBidsEntry(investor.clone(), count);
        env.storage().persistent().set(&entry_key, bid_id);
        bump_persistent(env, &entry_key);
        env.storage().persistent().set(&count_key, &(count + 1));
        bump_persistent(env, &count_key);
    }

    pub fn store_bid(env: &Env, bid: &Bid) {
        crate::assert_view_only!(env);
        env.storage().persistent().set(&bid.bid_id, bid);
        bump_persistent(env, &bid.bid_id);
        // Add to investor index
        Self::add_to_investor_bids(env, &bid.investor, &bid.bid_id);
        // Add to global index
        Self::add_to_all_bids(env, &bid.bid_id);
    }

    pub fn get_bid(env: &Env, bid_id: &BytesN<32>) -> Option<Bid> {
        let result = env.storage().persistent().get(bid_id);
        if result.is_some() {
            bump_persistent(env, &bid_id);
        }
        result
    }

    pub fn update_bid(env: &Env, bid: &Bid) {
        crate::assert_view_only!(env);
        env.storage().persistent().set(&bid.bid_id, bid);
        bump_persistent(env, &bid.bid_id);
    }

    pub fn get_bids_for_invoice(env: &Env, invoice_id: &BytesN<32>) -> Vec<BytesN<32>> {
        let count_key = Self::invoice_bid_count_key(invoice_id);
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        if count > 0 {
            bump_persistent(env, &count_key);
        }
        let mut bids = Vec::new(env);
        let mut idx: u32 = 0;
        while idx < count {
            let entry_key = Self::invoice_bid_entry_key(invoice_id, idx);
            if let Some(bid_id) = env.storage().persistent().get(&entry_key) {
                bump_persistent(env, &entry_key);
                bids.push_back(bid_id);
            }
            idx += 1;
        }
        bids
    }

    pub fn get_active_bid_count(env: &Env, invoice_id: &BytesN<32>) -> u32 {
        let _ = Self::refresh_expired_bids(env, invoice_id);
        let bid_ids = Self::get_bids_for_invoice(env, invoice_id);
        let mut active_count = 0u32;
        let mut idx: u32 = 0;
        while idx < bid_ids.len() {
            let bid_id = bid_ids.get(idx).unwrap();
            if let Some(bid) = Self::get_bid(env, &bid_id) {
                if bid.status == BidStatus::Placed {
                    active_count += 1;
                }
            }
            idx += 1;
        }
        active_count
    }

    pub fn get_bid_ttl_days(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&BID_TTL_KEY)
            .unwrap_or(DEFAULT_BID_TTL_DAYS)
    }

    pub fn get_bid_ttl_config(env: &Env) -> BidTtlConfig {
        let stored: Option<u64> = env.storage().instance().get(&BID_TTL_KEY);
        BidTtlConfig {
            current_days: stored.unwrap_or(DEFAULT_BID_TTL_DAYS),
            min_days: MIN_BID_TTL_DAYS,
            max_days: MAX_BID_TTL_DAYS,
            default_days: DEFAULT_BID_TTL_DAYS,
            is_custom: stored.is_some(),
        }
    }

    pub fn set_bid_ttl_days(env: &Env, admin: &Address, days: u64) -> Result<u64, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        if days == 0 {
            return Err(QuickLendXError::InvalidBidTtl);
        }
        if !(MIN_BID_TTL_DAYS..=MAX_BID_TTL_DAYS).contains(&days) {
            return Err(QuickLendXError::InvalidBidTtl);
        }

        let old_days = Self::get_bid_ttl
