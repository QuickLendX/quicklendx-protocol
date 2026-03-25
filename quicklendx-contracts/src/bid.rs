use core::cmp::Ordering;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol, Vec};

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::events::{emit_bid_expired, emit_bid_ttl_updated};

// ─── Bid TTL configuration ────────────────────────────────────────────────────
//
// TTL is stored in whole days and is admin-configurable within [MIN, MAX].
// A zero TTL is explicitly rejected to prevent bids that expire immediately.
// An extreme TTL (> MAX_BID_TTL_DAYS) is rejected to prevent bids that
// effectively never expire, which would lock investor funds indefinitely.
//
// Default: 7 days  |  Min: 1 day  |  Max: 30 days
pub const DEFAULT_BID_TTL_DAYS: u64 = 7;
pub const MIN_BID_TTL_DAYS: u64 = 1;
pub const MAX_BID_TTL_DAYS: u64 = 30;
const BID_TTL_KEY: Symbol = symbol_short!("bid_ttl");
const MAX_ACTIVE_BIDS_PER_INVESTOR_KEY: Symbol = symbol_short!("mx_actbd");
const DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR: u32 = 20;
const SECONDS_PER_DAY: u64 = 86400;

/// Maximum number of bids allowed per invoice to prevent unbound storage growth
pub const MAX_BIDS_PER_INVOICE: u32 = 50;

/// Snapshot of the current bid TTL configuration returned by `get_bid_ttl_config`.
///
/// Provides all bounds and the active value in a single call so off-chain
/// clients and tests can assert the full configuration without multiple queries.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BidTtlConfig {
    /// Currently active TTL in days (admin-set or compile-time default).
    pub current_days: u64,
    /// Minimum allowed TTL in days (compile-time constant: 1).
    pub min_days: u64,
    /// Maximum allowed TTL in days (compile-time constant: 30).
    pub max_days: u64,
    /// Compile-time default TTL in days (7).
    pub default_days: u64,
    /// `true` when the admin has explicitly set a TTL; `false` when the
    /// compile-time default is in use.
    pub is_custom: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BidStatus {
    Placed,
    Withdrawn,
    Accepted,
    Expired,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bid {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub timestamp: u64,
    pub status: BidStatus,
    pub expiration_timestamp: u64,
}

impl Bid {
    pub fn is_expired(&self, current_timestamp: u64) -> bool {
        current_timestamp > self.expiration_timestamp
    }

    /// Backward-compatible helper used by some tests: uses compile-time default.
    pub fn default_expiration(now: u64) -> u64 {
        now.saturating_add(DEFAULT_BID_TTL_DAYS.saturating_mul(SECONDS_PER_DAY))
    }

    /// Compute default expiration using configured TTL (admin-configurable).
    pub fn default_expiration_with_env(env: &Env, now: u64) -> u64 {
        let days = BidStorage::get_bid_ttl_days(env);
        now.saturating_add(days.saturating_mul(SECONDS_PER_DAY))
    }
}

pub struct BidStorage;

impl BidStorage {
    fn invoice_key(invoice_id: &BytesN<32>) -> (soroban_sdk::Symbol, BytesN<32>) {
        (symbol_short!("bids"), invoice_id.clone())
    }

    fn investor_bids_key(investor: &Address) -> (soroban_sdk::Symbol, Address) {
        (symbol_short!("bid_inv"), investor.clone())
    }

    pub fn get_bids_by_investor_all(env: &Env, investor: &Address) -> Vec<BytesN<32>> {
        let key = Self::investor_bids_key(investor);
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn add_to_investor_bids(env: &Env, investor: &Address, bid_id: &BytesN<32>) {
        let key = Self::investor_bids_key(investor);
        let mut bids = Self::get_bids_by_investor_all(env, investor);
        let mut exists = false;
        for bid in bids.iter() {
            if bid == *bid_id {
                exists = true;
                break;
            }
        }
        if !exists {
            bids.push_back(bid_id.clone());
            env.storage().instance().set(&key, &bids);
        }
    }

    pub fn store_bid(env: &Env, bid: &Bid) {
        env.storage().instance().set(&bid.bid_id, bid);
        // Add to investor index
        Self::add_to_investor_bids(env, &bid.investor, &bid.bid_id);
    }
    pub fn get_bid(env: &Env, bid_id: &BytesN<32>) -> Option<Bid> {
        env.storage().instance().get(bid_id)
    }
    pub fn update_bid(env: &Env, bid: &Bid) {
        env.storage().instance().set(&bid.bid_id, bid);
    }
    pub fn get_bids_for_invoice(env: &Env, invoice_id: &BytesN<32>) -> Vec<BytesN<32>> {
        env.storage()
            .instance()
            .get(&Self::invoice_key(invoice_id))
            .unwrap_or_else(|| Vec::new(env))
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

    /// Return the currently active bid TTL in days.
    ///
    /// Falls back to `DEFAULT_BID_TTL_DAYS` (7) when no admin override has
    /// been stored, ensuring deterministic behaviour even on a fresh contract.
    pub fn get_bid_ttl_days(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&BID_TTL_KEY)
            .unwrap_or(DEFAULT_BID_TTL_DAYS)
    }

    /// Return the full TTL configuration snapshot.
    ///
    /// Includes the active value, compile-time bounds, the default, and a flag
    /// indicating whether the admin has overridden the default.
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

    /// Admin-only: set bid TTL in days.
    ///
    /// ### Bounds
    /// - Minimum: `MIN_BID_TTL_DAYS` (1) — prevents zero-TTL bids that expire
    ///   immediately and can never be accepted.
    /// - Maximum: `MAX_BID_TTL_DAYS` (30) — prevents extreme windows that
    ///   would lock investor funds for unreasonably long periods.
    ///
    /// ### Errors
    /// Returns `InvalidBidTtl` (not `InvalidAmount`) for a clear, auditable
    /// error signal distinct from monetary validation failures.
    ///
    /// ### Events
    /// Emits `ttl_upd` with the old value, new value, admin address, and
    /// ledger timestamp so off-chain monitors can track every config change.
    pub fn set_bid_ttl_days(env: &Env, admin: &Address, days: u64) -> Result<u64, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        // Explicit zero check first for a clear error message.
        if days == 0 {
            return Err(QuickLendXError::InvalidBidTtl);
        }
        if days < MIN_BID_TTL_DAYS || days > MAX_BID_TTL_DAYS {
            return Err(QuickLendXError::InvalidBidTtl);
        }

        let old_days = Self::get_bid_ttl_days(env);
        env.storage().instance().set(&BID_TTL_KEY, &days);
        emit_bid_ttl_updated(env, old_days, days, admin);
        Ok(days)
    }

    /// Admin-only: reset bid TTL to the compile-time default (7 days).
    ///
    /// Removes the stored override so `get_bid_ttl_days` returns the default
    /// and `get_bid_ttl_config` reports `is_custom = false`.
    ///
    /// ### Events
    /// Emits `ttl_upd` with the old value and `DEFAULT_BID_TTL_DAYS` as the
    /// new value so the reset is fully auditable.
    pub fn reset_bid_ttl_to_default(env: &Env, admin: &Address) -> Result<u64, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        let old_days = Self::get_bid_ttl_days(env);
        env.storage().instance().remove(&BID_TTL_KEY);
        emit_bid_ttl_updated(env, old_days, DEFAULT_BID_TTL_DAYS, admin);
        Ok(DEFAULT_BID_TTL_DAYS)
    }

    /// Get configured max number of active (Placed) bids per investor across all invoices.
    /// A value of 0 disables this limit.
    pub fn get_max_active_bids_per_investor(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&MAX_ACTIVE_BIDS_PER_INVESTOR_KEY)
            .unwrap_or(DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR)
    }

    /// Admin-only: set max number of active (Placed) bids per investor across all invoices.
    /// A value of 0 disables this limit.
    pub fn set_max_active_bids_per_investor(
        env: &Env,
        admin: &Address,
        limit: u32,
    ) -> Result<u32, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;
        env.storage()
            .instance()
            .set(&MAX_ACTIVE_BIDS_PER_INVESTOR_KEY, &limit);
        Ok(limit)
    }

    /// Count currently active (Placed) bids for an investor across all invoices.
    /// Expired bids are transitioned to `Expired` during this scan and do not count.
    pub fn count_active_placed_bids_for_investor(env: &Env, investor: &Address) -> u32 {
        let current_timestamp = env.ledger().timestamp();
        let bid_ids = Self::get_bids_by_investor_all(env, investor);
        let mut count = 0u32;

        for bid_id in bid_ids.iter() {
            if let Some(mut bid) = Self::get_bid(env, &bid_id) {
                if bid.status != BidStatus::Placed {
                    continue;
                }
                if bid.is_expired(current_timestamp) {
                    bid.status = BidStatus::Expired;
                    Self::update_bid(env, &bid);
                    emit_bid_expired(env, &bid);
                } else {
                    count = count.saturating_add(1);
                }
            }
        }

        count
    }
    pub fn add_bid_to_invoice(env: &Env, invoice_id: &BytesN<32>, bid_id: &BytesN<32>) {
        let mut bids = Self::get_bids_for_invoice(env, invoice_id);
        let mut exists = false;
        let mut idx: u32 = 0;
        while idx < bids.len() {
            if bids.get(idx).unwrap() == *bid_id {
                exists = true;
                break;
            }
            idx += 1;
        }
        if !exists {
            bids.push_back(bid_id.clone());
            env.storage()
                .instance()
                .set(&Self::invoice_key(invoice_id), &bids);
        }
    }
    fn refresh_expired_bids(env: &Env, invoice_id: &BytesN<32>) -> u32 {
        let current_timestamp = env.ledger().timestamp();
        let bid_ids = Self::get_bids_for_invoice(env, invoice_id);
        let mut active = Vec::new(env);
        let mut expired = 0u32;
        let mut idx: u32 = 0;
        while idx < bid_ids.len() {
            let bid_id = bid_ids.get(idx).unwrap();
            if let Some(mut bid) = Self::get_bid(env, &bid_id) {
                // Invariant 1: Preservation — terminal bids are NEVER touched by cleanup.
                // Accepted, Withdrawn, and Cancelled are immutable terminal states.
                let is_terminal = bid.status == BidStatus::Accepted
                    || bid.status == BidStatus::Withdrawn
                    || bid.status == BidStatus::Cancelled;
                if is_terminal {
                    active.push_back(bid_id);
                // Invariant 2: Idempotency — already-Expired bids are silently skipped.
                } else if bid.status == BidStatus::Expired {
                    // drop from active list; do not re-process
                // Invariant 3: Deadline — only expire Placed bids past their deadline.
                } else if bid.status == BidStatus::Placed && bid.is_expired(current_timestamp) {
                    bid.status = BidStatus::Expired;
                    Self::update_bid(env, &bid);
                    emit_bid_expired(env, &bid);
                    expired += 1;
                } else {
                    // Placed but deadline not yet reached — keep active
                    active.push_back(bid_id);
                }
            }
            idx += 1;
        }
        env.storage()
            .instance()
            .set(&Self::invoice_key(invoice_id), &active);
        expired
    }

    pub fn cleanup_expired_bids(env: &Env, invoice_id: &BytesN<32>) -> u32 {
        Self::refresh_expired_bids(env, invoice_id)
    }

    pub fn get_bid_records_for_invoice(env: &Env, invoice_id: &BytesN<32>) -> Vec<Bid> {
        let _ = Self::refresh_expired_bids(env, invoice_id);
        let mut bids = Vec::new(env);
        for bid_id in Self::get_bids_for_invoice(env, invoice_id).iter() {
            if let Some(bid) = Self::get_bid(env, &bid_id) {
                bids.push_back(bid);
            }
        }
        bids
    }
    pub fn get_bids_by_status(env: &Env, invoice_id: &BytesN<32>, status: BidStatus) -> Vec<Bid> {
        let mut filtered = Vec::new(env);
        let records = Self::get_bid_records_for_invoice(env, invoice_id);
        let mut idx: u32 = 0;
        while idx < records.len() {
            let bid = records.get(idx).unwrap();
            if bid.status == status {
                filtered.push_back(bid);
            }
            idx += 1;
        }
        filtered
    }
    pub fn get_bids_by_investor(
        env: &Env,
        invoice_id: &BytesN<32>,
        investor: &Address,
    ) -> Vec<Bid> {
        let mut filtered = Vec::new(env);
        let records = Self::get_bid_records_for_invoice(env, invoice_id);
        let mut idx: u32 = 0;
        while idx < records.len() {
            let bid = records.get(idx).unwrap();
            if &bid.investor == investor {
                filtered.push_back(bid);
            }
            idx += 1;
        }
        filtered
    }
    pub fn compare_bids(bid1: &Bid, bid2: &Bid) -> Ordering {
        let profit1 = bid1.expected_return.saturating_sub(bid1.bid_amount);
        let profit2 = bid2.expected_return.saturating_sub(bid2.bid_amount);
        if profit1 != profit2 {
            return profit1.cmp(&profit2);
        }
        if bid1.expected_return != bid2.expected_return {
            return bid1.expected_return.cmp(&bid2.expected_return);
        }
        if bid1.bid_amount != bid2.bid_amount {
            return bid1.bid_amount.cmp(&bid2.bid_amount);
        }
        if bid1.timestamp != bid2.timestamp {
            return bid2.timestamp.cmp(&bid1.timestamp);
        }
        Ordering::Equal
    }
    pub fn get_best_bid(env: &Env, invoice_id: &BytesN<32>) -> Option<Bid> {
        let records = Self::get_bid_records_for_invoice(env, invoice_id);
        let mut best: Option<Bid> = None;
        let mut idx: u32 = 0;
        while idx < records.len() {
            let candidate = records.get(idx).unwrap();
            if candidate.status != BidStatus::Placed {
                idx += 1;
                continue;
            }
            best = match best {
                None => Some(candidate),
                Some(current) => {
                    if Self::compare_bids(&candidate, &current) == Ordering::Greater {
                        Some(candidate)
                    } else {
                        Some(current)
                    }
                }
            };
            idx += 1;
        }
        best
    }
    pub fn rank_bids(env: &Env, invoice_id: &BytesN<32>) -> Vec<Bid> {
        let records = Self::get_bid_records_for_invoice(env, invoice_id);
        let mut remaining = Vec::new(env);
        let mut idx: u32 = 0;
        while idx < records.len() {
            let bid = records.get(idx).unwrap();
            if bid.status == BidStatus::Placed {
                remaining.push_back(bid);
            }
            idx += 1;
        }

        let mut ranked = Vec::new(env);

        while remaining.len() > 0 {
            let mut best_idx: u32 = 0;
            let mut best_bid = remaining.get(0).unwrap();
            let mut search_idx: u32 = 1;
            while search_idx < remaining.len() {
                let candidate = remaining.get(search_idx).unwrap();
                if Self::compare_bids(&candidate, &best_bid) == Ordering::Greater {
                    best_idx = search_idx;
                    best_bid = candidate;
                }
                search_idx += 1;
            }
            ranked.push_back(best_bid);

            let mut new_remaining = Vec::new(env);
            let mut copy_idx: u32 = 0;
            while copy_idx < remaining.len() {
                if copy_idx != best_idx {
                    new_remaining.push_back(remaining.get(copy_idx).unwrap());
                }
                copy_idx += 1;
            }
            remaining = new_remaining;
        }

        ranked
    }

    /// Cancel a placed bid by bid_id. Only transitions Placed → Cancelled.
    /// Returns false if bid not found or already not Placed.
    pub fn cancel_bid(env: &Env, bid_id: &BytesN<32>) -> bool {
        if let Some(mut bid) = Self::get_bid(env, bid_id) {
            if bid.status == BidStatus::Placed {
                bid.status = BidStatus::Cancelled;
                Self::update_bid(env, &bid);
                return true;
            }
        }
        false
    }

    /// Return all bids placed by an investor across all invoices, with their full Bid records.
    pub fn get_all_bids_by_investor(env: &Env, investor: &Address) -> Vec<Bid> {
        let bid_ids = Self::get_bids_by_investor_all(env, investor);
        let mut result = Vec::new(env);
        for bid_id in bid_ids.iter() {
            if let Some(bid) = Self::get_bid(env, &bid_id) {
                result.push_back(bid);
            }
        }
        result
    }

    /// Count the number of currently active (Placed) bids for a given investor.
    ///
    /// This is used by rate-limiting logic in the main contract to enforce a
    /// maximum number of open bids per investor across all invoices.
    pub fn count_active_bids_by_investor(env: &Env, investor: &Address) -> u32 {
        let all_bids = Self::get_all_bids_by_investor(env, investor);
        let mut count: u32 = 0;
        let mut idx: u32 = 0;
        while idx < all_bids.len() {
            let bid = all_bids.get(idx).unwrap();
            if bid.status == BidStatus::Placed {
                count = count.saturating_add(1);
            }
            idx = idx.saturating_add(1);
        }
        count
    }

    /// Calculate the sum of all currently active (Placed) bid amounts for a given investor.
    /// Used for checking against the investor's total investment limit.
    pub fn get_active_bid_amount_sum_for_investor(env: &Env, investor: &Address) -> i128 {
        let all_bids = Self::get_all_bids_by_investor(env, investor);
        let current_timestamp = env.ledger().timestamp();
        let mut total_amount: i128 = 0;
        let mut idx: u32 = 0;
        while idx < all_bids.len() {
            let bid = all_bids.get(idx).unwrap();
            if bid.status == BidStatus::Placed && !bid.is_expired(current_timestamp) {
                total_amount = total_amount.saturating_add(bid.bid_amount);
            }
            idx = idx.saturating_add(1);
        }
        total_amount
    }
    /// Generates a unique 32-byte bid ID using timestamp and a simple counter.
    /// This approach avoids potential serialization issues with large counters.
    pub fn generate_unique_bid_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let counter_key = symbol_short!("bid_cnt");
        let counter: u64 = env.storage().instance().get(&counter_key).unwrap_or(0u64);
        let next_counter = counter.saturating_add(1);
        env.storage().instance().set(&counter_key, &next_counter);

        let mut bytes = [0u8; 32];
        // Add bid prefix to distinguish from other entity types
        bytes[0] = 0xB1; // 'B' for Bid
        bytes[1] = 0xD0; // 'D' for biD
                         // Embed timestamp in next 8 bytes
        bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        // Embed counter in next 8 bytes
        bytes[10..18].copy_from_slice(&next_counter.to_be_bytes());
        // Fill remaining bytes with a pattern to ensure uniqueness (overflow-safe)
        let mix = timestamp
            .saturating_add(next_counter)
            .saturating_add(0xB1D0);
        for i in 18..32 {
            bytes[i] = (mix % 256) as u8;
        }
        BytesN::from_array(env, &bytes)
    }

    /// Validates cleanup invariants for all bids on an invoice.
    ///
    /// Returns `true` if all invariants hold:
    /// - Every `Expired` bid has a deadline strictly in the past.
    /// - No `Placed` bid has a deadline that has already passed (cleanup was run).
    pub fn assert_bid_invariants(
        env: &Env,
        invoice_id: &BytesN<32>,
        current_timestamp: u64,
    ) -> bool {
        let bid_ids = Self::get_bids_for_invoice(env, invoice_id);
        let mut idx: u32 = 0;
        while idx < bid_ids.len() {
            let bid_id = bid_ids.get(idx).unwrap();
            if let Some(bid) = Self::get_bid(env, &bid_id) {
                // Every Expired bid must have a past deadline
                if bid.status == BidStatus::Expired {
                    if bid.expiration_timestamp >= current_timestamp {
                        return false;
                    }
                }
                // No Placed bid should remain past its deadline
                if bid.status == BidStatus::Placed {
                    if bid.is_expired(current_timestamp) {
                        return false;
                    }
                }
            }
            idx += 1;
        }
        true
    }

    /// Returns bid counts by status as `(placed, accepted, withdrawn, expired, cancelled)`.
    /// Useful for assertions in tests and analytics.
    pub fn count_bids_by_status(
        env: &Env,
        invoice_id: &BytesN<32>,
    ) -> (u32, u32, u32, u32, u32) {
        let records = Self::get_bid_records_for_invoice(env, invoice_id);
        let (mut placed, mut accepted, mut withdrawn, mut expired, mut cancelled) =
            (0u32, 0u32, 0u32, 0u32, 0u32);
        let mut idx: u32 = 0;
        while idx < records.len() {
            let bid = records.get(idx).unwrap();
            match bid.status {
                BidStatus::Placed => placed += 1,
                BidStatus::Accepted => accepted += 1,
                BidStatus::Withdrawn => withdrawn += 1,
                BidStatus::Expired => expired += 1,
                BidStatus::Cancelled => cancelled += 1,
            }
            idx += 1;
        }
        (placed, accepted, withdrawn, expired, cancelled)
    }
}
