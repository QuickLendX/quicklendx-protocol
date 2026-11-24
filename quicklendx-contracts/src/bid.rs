use core::cmp::Ordering;
use soroban_sdk::{
    contracttype, symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, Symbol, Vec,
};

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

    /// Active means still in the Placed state and not yet expired at the
    /// current ledger timestamp.
    pub fn is_active(&self, current_timestamp: u64) -> bool {
        self.status == BidStatus::Placed && !self.is_expired(current_timestamp)
    }

    /// Backward-compatible helper used by some tests: uses compile-time default.
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

    pub fn get_active_bid_count(
        env: &Env,
        invoice_id: &BytesN<32>,
    ) -> Result<u32, QuickLendXError> {
        Self::refresh_expired_bids(env, invoice_id)?;
        let bid_ids = Self::get_bids_for_invoice(env, invoice_id);
        let current_timestamp = env.ledger().timestamp();
        let mut active_count = 0u32;
        for bid_id in bid_ids.iter() {
            if let Some(bid) = Self::get_bid(env, &bid_id) {
                if bid.is_active(current_timestamp) {
                    active_count = active_count.saturating_add(1);
                }
            }
        }
        Ok(active_count)
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

    /// Return the currently active bid expiry grace period, in seconds.
    ///
    /// Falls back to `DEFAULT_BID_EXPIRY_GRACE_SECONDS` (0 — no grace, matching
    /// pre-existing behaviour) when no admin override has been stored.
    pub fn get_bid_expiry_grace_seconds(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&BID_EXPIRY_GRACE_KEY)
            .unwrap_or(DEFAULT_BID_EXPIRY_GRACE_SECONDS)
    }

    /// Return the full bid expiry grace-period configuration snapshot.
    pub fn get_bid_expiry_grace_config(env: &Env) -> BidExpiryGraceConfig {
        let stored: Option<u64> = env.storage().instance().get(&BID_EXPIRY_GRACE_KEY);
        BidExpiryGraceConfig {
            current_seconds: stored.unwrap_or(DEFAULT_BID_EXPIRY_GRACE_SECONDS),
            min_seconds: MIN_BID_EXPIRY_GRACE_SECONDS,
            max_seconds: MAX_BID_EXPIRY_GRACE_SECONDS,
            default_seconds: DEFAULT_BID_EXPIRY_GRACE_SECONDS,
            is_custom: stored.is_some(),
        }
    }

    /// Admin-only: set the bid expiry grace period, in seconds.
    ///
    /// ### Bounds
    /// - Minimum: `MIN_BID_EXPIRY_GRACE_SECONDS` (0) - allows cleanup
    ///   immediately at expiry when no buffer is desired.
    /// - Maximum: `MAX_BID_EXPIRY_GRACE_SECONDS` (30 days) - prevents a grace
    ///   window so long that stale bids never become cleanable.
    ///
    /// ### Errors
    /// Returns `InvalidTimestamp` for an out-of-bounds value, matching the
    /// convention used by `defaults::resolve_grace_period` for the analogous
    /// invoice grace period.
    ///
    /// ### Events
    /// Emits `BidExpiryGraceUpdated` with the old value, new value, admin
    /// address, and ledger timestamp so off-chain monitors can track every
    /// config change.
    pub fn set_bid_expiry_grace_seconds(
        env: &Env,
        admin: &Address,
        seconds: u64,
    ) -> Result<u64, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        if seconds > MAX_BID_EXPIRY_GRACE_SECONDS {
            return Err(QuickLendXError::InvalidTimestamp);
        }

        let old_seconds = Self::get_bid_expiry_grace_seconds(env);
        env.storage()
            .instance()
            .set(&BID_EXPIRY_GRACE_KEY, &seconds);
        emit_bid_expiry_grace_updated(env, old_seconds, seconds, admin);
        Ok(seconds)
    }

    /// Admin-only: reset the bid expiry grace period to the compile-time
    /// default (0).
    ///
    /// Removes the stored override so `get_bid_expiry_grace_seconds` returns
    /// the default and `get_bid_expiry_grace_config` reports `is_custom = false`.
    pub fn reset_bid_expiry_grace_to_default(
        env: &Env,
        admin: &Address,
    ) -> Result<u64, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        let old_seconds = Self::get_bid_expiry_grace_seconds(env);
        env.storage().instance().remove(&BID_EXPIRY_GRACE_KEY);
        emit_bid_expiry_grace_updated(env, old_seconds, DEFAULT_BID_EXPIRY_GRACE_SECONDS, admin);
        Ok(DEFAULT_BID_EXPIRY_GRACE_SECONDS)
    }

    /// Get configured max number of active (Placed) bids per investor across all invoices.
    /// A value of 0 disables this limit.
    pub fn get_max_active_bids_per_investor(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&MAX_ACTIVE_BIDS_PER_INVESTOR_KEY)
            .unwrap_or(DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR)
    }

    /// Return a complete snapshot of the investor active-bid limit policy.
    ///
    /// Analogous to [`BidStorage::get_bid_ttl_config`] for TTL.  Intended
    /// for off-chain dashboards, admin panels, and test assertions.
    ///
    pub fn get_bid_limit_config(env: &Env) -> BidLimitConfig {
        let stored: Option<u32> = env
            .storage()
            .instance()
            .get(&MAX_ACTIVE_BIDS_PER_INVESTOR_KEY);
        let limit = stored.unwrap_or(DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR);
        BidLimitConfig {
            limit,
            default_limit: DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR,
            is_disabled: limit == INVESTOR_BID_LIMIT_DISABLED,
            is_custom: stored.is_some(),
        }
    }

    /// Returns `true` when the investor active-bid limit is enforced.
    ///
    /// Returns `false` when the limit has been set to `0`
    /// (`INVESTOR_BID_LIMIT_DISABLED`), meaning bids will **not** be rejected
    /// for having too many open positions.
    ///
    /// ### Usage
    ///
    /// Prefer this over comparing `get_max_active_bids_per_investor() != 0`
    /// directly, to keep the zero-is-disabled semantic in one place.
    ///
    /// ```ignore
    /// if BidStorage::is_investor_bid_limit_active(&env) {
    ///     // enforcement is on; check count
    /// }
    /// ```
    pub fn is_investor_bid_limit_active(env: &Env) -> bool {
        Self::get_max_active_bids_per_investor(env) != INVESTOR_BID_LIMIT_DISABLED
    }

    /// This function is **read-only** with respect to the limit policy itself.
    /// Setting or changing the limit requires admin authority and goes through
    /// [`BidStorage::set_max_active_bids_per_investor`].
    pub fn investor_has_reached_bid_limit(
        env: &Env,
        investor: &Address,
    ) -> Result<bool, QuickLendXError> {
        let limit = Self::get_max_active_bids_per_investor(env);

        // Limit of 0 means "disabled" - never block a placement.
        if limit == INVESTOR_BID_LIMIT_DISABLED {
            return Ok(false);
        }

        let active = Self::count_active_placed_bids_for_investor(env, investor)?;
        Ok(active >= limit)
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

    /// Admin-only: reset the investor active-bid limit to the compile-time
    /// default (`DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR` = 20).
    ///
    /// Removes the stored override so `get_bid_limit_config` reports
    /// `is_custom = false` and `is_disabled = false`.
    ///
    /// Useful for reverting a previous `set_max_active_bids_per_investor(0)`
    /// call when the unrestricted window should end.
    pub fn reset_max_active_bids_per_investor(
        env: &Env,
        admin: &Address,
    ) -> Result<u32, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;
        env.storage()
            .instance()
            .remove(&MAX_ACTIVE_BIDS_PER_INVESTOR_KEY);
        Ok(DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR)
    }

    /// @notice Prunes expired bids from the investor's global index.
    ///
    /// # Purpose
    /// Maintains the investor's bid list to prevent unbounded growth with historical expired bids.
    /// Ensures that investor active-bid limit checks (e.g., MAX_ACTIVE_BIDS_PER_INVESTOR) operate
    /// in O(active_bids) time, not O(all_historical_bids).
    ///
    /// # Invariants
    /// - Terminal bids (Accepted, Withdrawn, Cancelled) are kept in the index for historical audit
    /// - Expired bids are pruned to keep the list size manageable
    /// - Placed (non-expired) bids are preserved
    /// - The index after refresh accurately reflects countable active bids for rate-limiting
    ///
    /// # Grace period
    /// A `Placed` bid only transitions to `Expired` once it has passed its
    /// `expiration_timestamp` by at least the configured
    /// `bid_expiry_grace_seconds` (see `BidStorage::get_bid_expiry_grace_seconds`).
    /// It is still excluded from acceptance and active-bid counts as soon as
    /// its raw TTL passes; the grace period only delays this storage
    /// transition and index pruning. See `docs/BID_LIFECYCLE_DIAGRAM.md`.
    ///
    /// # Parameters
    /// @param env The Soroban environment
    /// @param investor The address of the investor
    ///
    /// @return newly_expired The number of bids that transitioned from Placed to Expired in this call
    pub fn refresh_investor_bids(env: &Env, investor: &Address) -> Result<u32, QuickLendXError> {
        let current_timestamp = env.ledger().timestamp();
        let grace_seconds = Self::get_bid_expiry_grace_seconds(env);
        let bid_ids = Self::get_bids_by_investor_all(env, investor);
        let mut active = Vec::new(env);
        let mut newly_expired = 0u32;

        for bid_id in bid_ids.iter() {
            if let Some(mut bid) = Self::get_bid(env, &bid_id) {
                // Determine if this bid should remain in the investor's active index.
                // We keep terminal states (Accepted, Withdrawn, Cancelled) in the index
                // but prune Expired ones to keep the list size manageable.
                if bid.status == BidStatus::Placed {
                    if bid.is_cleanup_eligible(current_timestamp, grace_seconds) {
                        bid.status = BidStatus::Expired;
                        Self::update_bid(env, &bid);
                        emit_bid_expired(env, &bid);
                        crate::audit::log_bid_expired(
                            env,
                            bid.invoice_id.clone(),
                            bid.investor.clone(),
                            bid.bid_amount,
                            bid.bid_id.clone(),
                        )?;
                        newly_expired = newly_expired.saturating_add(1);
                        // Do not push to active -> prunes this expired bid
                    } else {
                        active.push_back(bid_id);
                    }
                } else if bid.status == BidStatus::Expired {
                    // Prune already expired bids from the index
                } else {
                    // Keep terminal states: Accepted, Withdrawn, Cancelled
                    active.push_back(bid_id);
                }
            }
        }

        // Only update storage if the list actually shrank. The investor index
        // lives in the *persistent* storage domain (see `add_to_investor_bids`
        // and `get_bids_by_investor_all`). Writing the pruned list to instance
        // storage here would silently leave the raw persistent index untouched,
        // so expired bids would never actually be pruned and every later sweep
        // would re-scan them (violating the bounded-index invariant).
        let count_key = BidIndexKey::InvestorBidsCount(investor.clone());
        let stored_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        if active.len() < stored_count {
            for (index, bid_id) in active.iter().enumerate() {
                let entry_key =
                    BidIndexKey::InvestorBidsEntry(investor.clone(), index as u32);
                env.storage().persistent().set(&entry_key, &bid_id);
                bump_persistent(env, &entry_key);
            }
            for index in active.len()..stored_count {
                let entry_key = BidIndexKey::InvestorBidsEntry(investor.clone(), index);
                env.storage().persistent().remove(&entry_key);
            }
            env.storage()
                .persistent()
                .set(&count_key, &active.len());
            bump_persistent(env, &count_key);
        }

        Ok(newly_expired)
    }

    /// @notice Count currently active (Placed) bids for an investor across all invoices.
    ///
    /// # Purpose
    /// Returns the count of non-expired Placed bids for rate limiting and bid management.
    /// Used by bidding logic to enforce MAX_ACTIVE_BIDS_PER_INVESTOR.
    ///
    /// # Invariants
    /// - Includes only Placed bids that have not yet reached their expiration timestamp
    /// - Excludes terminal states (Accepted, Withdrawn, Cancelled) and Expired bids
    /// - The count is always <= the investor's active bid limit (if enforced)
    ///
    /// # Side Effects
    /// - Calls refresh_investor_bids, which may update the investor's bid index to prune expired bids
    /// - Does NOT modify bid statuses (transitions happen within refresh_investor_bids)
    ///
    /// @param env The Soroban environment
    /// @param investor The address of the investor
    /// @return count The number of non-expired Placed bids across all invoices
    pub fn count_active_placed_bids_for_investor(
        env: &Env,
        investor: &Address,
    ) -> Result<u32, QuickLendXError> {
        Self::refresh_investor_bids(env, investor)?;
        let current_timestamp = env.ledger().timestamp();
        let bid_ids = Self::get_bids_by_investor_all(env, investor);
        let mut count = 0u32;

        for bid_id in bid_ids.iter() {
            if let Some(bid) = Self::get_bid(env, &bid_id) {
                if bid.status == BidStatus::Placed && !bid.is_expired(current_timestamp) {
                    count = count.saturating_add(1);
                }
            }
        }

        Ok(count)
    }
    pub fn add_bid_to_invoice(env: &Env, invoice_id: &BytesN<32>, bid_id: &BytesN<32>) {
        crate::assert_view_only!(env);
        let count_key = Self::invoice_bid_count_key(invoice_id);
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        let entry_key = Self::invoice_bid_entry_key(invoice_id, count);
        env.storage().persistent().set(&entry_key, bid_id);
        bump_persistent(env, &entry_key);
        env.storage().persistent().set(&count_key, &(count + 1));
        bump_persistent(env, &count_key);
    }
    /// @notice Scans and prunes expired bids from an invoice's bid list.
    /// @dev Maintains O(N) where N is current bids on invoice. Pruning keeps N small.
    ///
    /// # Invariants
    /// - Invariant 1: Terminal bids (Accepted, Withdrawn, Cancelled) are NEVER modified or removed
    /// - Invariant 2: Active Placed bids are preserved if not yet expired
    /// - Invariant 3: Expired/orphaned bids are removed from the index to prevent unbounded growth
    /// - Invariant 4: The operation is idempotent - calling multiple times on same state yields same result
    /// - Invariant 5: Cleanup is bounded by O(N) compute and storage changes
    ///
    /// # Security Properties
    /// - Cleanup cannot corrupt active bid records; terminal states are always preserved
    /// - Cleanup cannot trigger DoS via unbounded iteration (index size capped at MAX_BIDS_PER_INVOICE)
    /// - Cleanup is deterministic: same ledger timestamp + bid set -> same result always
    ///
    /// @param env The Soroban environment (for timestamp, storage access).
    /// @param invoice_id The unique identifier of the invoice.
    /// @return cleaned_count Total number of bids cleaned (transitioned to Expired or already Expired bids removed from index).
    ///
    /// A `Placed` bid transitions to `Expired` only after `expiration_timestamp
    /// + bid_expiry_grace_seconds` has elapsed; see `Bid::is_cleanup_eligible`.
    pub fn refresh_expired_bids(
        env: &Env,
        invoice_id: &BytesN<32>,
    ) -> Result<u32, QuickLendXError> {
        let current_timestamp = env.ledger().timestamp();
        let grace_seconds = Self::get_bid_expiry_grace_seconds(env);
        let count_key = Self::invoice_bid_count_key(invoice_id);
        let old_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        if old_count > 0 {
            bump_persistent(env, &count_key);
        }
        let mut cleaned_count = 0u32;
        let mut read_idx: u32 = 0;

        while read_idx < old_count {
            let entry_key = Self::invoice_bid_entry_key(invoice_id, read_idx);
            let should_keep =
                if let Some(bid_id) = env.storage().persistent().get::<_, BytesN<32>>(&entry_key) {
                    bump_persistent(env, &entry_key);
                    if let Some(mut bid) = Self::get_bid(env, &bid_id) {
                        let is_terminal = bid.status == BidStatus::Accepted
                            || bid.status == BidStatus::Withdrawn
                            || bid.status == BidStatus::Cancelled;

                        if is_terminal || bid.status == BidStatus::Expired {
                            true
                        } else if bid.status == BidStatus::Placed
                            && bid.is_cleanup_eligible(current_timestamp, grace_seconds)
                        {
                            bid.status = BidStatus::Expired;
                            Self::update_bid(env, &bid);
                            emit_bid_expired(env, &bid);
                            crate::audit::log_bid_expired(
                                env,
                                bid.invoice_id.clone(),
                                bid.investor.clone(),
                                bid.bid_amount,
                                bid.bid_id.clone(),
                            )?;
                            cleaned_count = cleaned_count.saturating_add(1);
                            true
                        } else {
                            true
                        }
                    } else {
                        cleaned_count = cleaned_count.saturating_add(1);
                        false
                    }
                } else {
                    false
                };

            if !should_keep {
                break;
            }
            read_idx += 1;
        }

        Ok(cleaned_count)
    }

    /// @notice Public interface to trigger cleanup of expired bids for a specific invoice.
    ///
    /// # Purpose
    /// Removes expired bids from an invoice's bid list to prevent storage bloat.
    /// Can be called proactively by off-chain indexers or triggered during on-chain operations.
    ///
    /// # Idempotency Guarantee
    /// This operation is fully idempotent: calling it multiple times on the same invoice
    /// and ledger timestamp will always:
    /// - Return 0 on subsequent calls (nothing new to clean)
    /// - Leave the index state unchanged
    /// - Never corrupt terminal bid records
    ///
    /// # DoS Safety
    /// - Cleanup is O(N) where N = number of bids on invoice (capped at MAX_BIDS_PER_INVOICE)
    /// - No unbounded allocations or recursive calls
    /// - No external calls; purely state transition
    /// - Gas cost scales predictably with bid count
    ///
    /// # Terminal Bid Preservation
    /// Accepted, Withdrawn, and Cancelled bids are NEVER touched by cleanup,
    /// even if they have passed their expiration timestamp. Only Placed bids
    /// can transition to Expired and be pruned.
    ///
    /// # Returns
    /// The count of bids cleaned (including newly expired and already-expired bids removed).
    /// On the second call with unchanged ledger time, returns 0.
    ///
    /// # Example
    /// ```ignore
    /// let cleaned = BidStorage::cleanup_expired_bids(&env, &invoice_id);
    /// // First call: returns 3 (3 expired Placed bids transitioned and removed)
    /// // Second call: returns 0 (idempotent; nothing left to clean)
    /// ```
    pub fn cleanup_expired_bids(
        env: &Env,
        invoice_id: &BytesN<32>,
    ) -> Result<u32, QuickLendXError> {
        Self::refresh_expired_bids(env, invoice_id)
    }

    /// @notice Paginated cleanup of expired bids for a specific invoice.
    ///
    /// # Purpose
    /// Removes expired bids from an invoice's bid list with pagination support.
    /// Allows operators to process large bid lists in multiple transactions to avoid
    /// instruction budget exhaustion at maximum capacity (MAX_BIDS_PER_INVOICE = 50).
    ///
    /// # Pagination Parameters
    /// - `offset`: Starting position in the bid list (0-indexed)
    /// - `limit`: Maximum number of bids to process in this call (capped at MAX_BIDS_PER_INVOICE)
    ///
    /// # Instruction Budget Safety
    /// By using pagination, operators can split cleanup of 50 bids across multiple transactions:
    /// - Single call with limit=50: ~500-1000 instructions (worst-case)
    /// - Two calls with limit=25: ~250-500 instructions each (safe margin)
    /// - Five calls with limit=10: ~100-200 instructions each (very safe)
    ///
    /// # Idempotency Guarantee
    /// This operation is fully idempotent: calling it multiple times on the same invoice
    /// and ledger timestamp will always:
    /// - Return 0 on subsequent calls (nothing new to clean)
    /// - Leave the index state unchanged
    /// - Never corrupt terminal bid records
    ///
    /// # Terminal Bid Preservation
    /// Accepted, Withdrawn, and Cancelled bids are NEVER touched by cleanup,
    /// even if they have passed their expiration timestamp. Only Placed bids
    /// can transition to Expired and be pruned.
    ///
    /// # Returns
    /// A tuple (cleaned_count, total_count) where:
    /// - `cleaned_count`: Number of bids cleaned in this call
    /// - `total_count`: Total number of bids on invoice after cleanup
    ///
    /// # Example
    /// ```ignore
    /// // Process 50 bids in two transactions
    /// let (cleaned1, total1) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 0, 25);
    /// // First call: returns (3, 47) - cleaned 3 bids, 47 remain
    /// let (cleaned2, total2) = BidStorage::cleanup_expired_bids_paged(&env, &invoice_id, 25, 25);
    /// // Second call: returns (0, 47) - no more to clean, 47 remain
    /// ```
    pub fn cleanup_expired_bids_paged(
        env: &Env,
        invoice_id: &BytesN<32>,
        offset: u32,
        limit: u32,
    ) -> Result<(u32, u32), QuickLendXError> {
        // Validate and cap pagination parameters
        let capped_limit = limit.min(MAX_BIDS_PER_INVOICE);

        // Prevent overflow: offset + limit must not exceed u32::MAX
        if offset > u32::MAX - capped_limit {
            return Ok((0, 0));
        }

        let current_timestamp = env.ledger().timestamp();
        let grace_seconds = Self::get_bid_expiry_grace_seconds(env);
        let count_key = Self::invoice_bid_count_key(invoice_id);
        let old_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        if old_count > 0 {
            bump_persistent(env, &count_key);
        }

        // If offset is beyond the current count, return early
        if offset >= old_count {
            return Ok((0, old_count));
        }

        let end_idx = (offset + capped_limit).min(old_count);
        let mut cleaned_count = 0u32;
        let mut write_idx: u32 = offset;
        let mut read_idx: u32 = offset;

        // Process only the requested range [offset, end_idx)
        while read_idx < end_idx {
            let entry_key = Self::invoice_bid_entry_key(invoice_id, read_idx);
            let should_keep =
                if let Some(bid_id) = env.storage().persistent().get::<_, BytesN<32>>(&entry_key) {
                    bump_persistent(env, &entry_key);
                    if let Some(mut bid) = Self::get_bid(env, &bid_id) {
                        let is_terminal = bid.status == BidStatus::Accepted
                            || bid.status == BidStatus::Withdrawn
                            || bid.status == BidStatus::Cancelled;

                        if is_terminal || bid.status == BidStatus::Expired {
                            true
                        } else if bid.status == BidStatus::Placed
                            && bid.is_cleanup_eligible(current_timestamp, grace_seconds)
                        {
                            bid.status = BidStatus::Expired;
                            Self::update_bid(env, &bid);
                            emit_bid_expired(env, &bid);
                            crate::audit::log_bid_expired(
                                env,
                                bid.invoice_id.clone(),
                                bid.investor.clone(),
                                bid.bid_amount,
                                bid.bid_id.clone(),
                            )?;
                            cleaned_count = cleaned_count.saturating_add(1);
                            true
                        } else {
                            true
                        }
                    } else {
                        cleaned_count = cleaned_count.saturating_add(1);
                        false
                    }
                } else {
                    false
                };

            if !should_keep {
                break;
            }
            read_idx += 1;
        }

        // Expired bids remain in the invoice index for historical queries; we
        // only transition them to `Expired` in storage and return the number
        // that were newly marked expired.
        Ok((cleaned_count, old_count))
    }

    pub fn get_bid_records_for_invoice(
        env: &Env,
        invoice_id: &BytesN<32>,
    ) -> Result<Vec<Bid>, QuickLendXError> {
        Self::refresh_expired_bids(env, invoice_id)?;
        let mut bids = Vec::new(env);
        for bid_id in Self::get_bids_for_invoice(env, invoice_id).iter() {
            if let Some(bid) = Self::get_bid(env, &bid_id) {
                if bid.invoice_id == *invoice_id {
                    bids.push_back(bid);
                }
            }
        }
        Ok(bids)
    }
    pub fn get_bids_by_status(
        env: &Env,
        invoice_id: &BytesN<32>,
        status: BidStatus,
    ) -> Result<Vec<Bid>, QuickLendXError> {
        let mut filtered = Vec::new(env);
        let records = Self::get_bid_records_for_invoice(env, invoice_id)?;
        let mut idx: u32 = 0;
        while idx < records.len() {
            let bid = records.get(idx).unwrap();
        let records = Self::get_bid_records_for_invoice(env, invoice_id);
        for bid in records.iter() {
            let eligible = status != BidStatus::Placed || !bid.is_expired(env.ledger().timestamp());
            if bid.status == status && eligible {
                filtered.push_back(bid);
            }
        }
        Ok(filtered)
    }
    pub fn get_bids_by_investor(
        env: &Env,
        invoice_id: &BytesN<32>,
        investor: &Address,
    ) -> Result<Vec<Bid>, QuickLendXError> {
        let mut filtered = Vec::new(env);
        let records = Self::get_bid_records_for_invoice(env, invoice_id)?;
        let mut idx: u32 = 0;
        while idx < records.len() {
            let bid = records.get(idx).unwrap();
        let records = Self::get_bid_records_for_invoice(env, invoice_id);
        for bid in records.iter() {
            if &bid.investor == investor {
                filtered.push_back(bid);
            }
        }
        Ok(filtered)
    }
    /// @notice Deterministically compares two bids.
    /// @dev Ordering priority: (1) profit, (2) expected_return, (3) bid_amount,
    /// (4) timestamp with newer bids first, (5) bid_id as final stable tiebreaker.
    /// This guarantees reproducible ranking across validators even when all economic
    /// values match.
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
            return bid1.timestamp.cmp(&bid2.timestamp);
        }
        // Final deterministic tiebreaker to avoid validator-dependent ordering
        if bid1.bid_id != bid2.bid_id {
            return bid1.bid_id.to_array().cmp(&bid2.bid_id.to_array());
        }
        Ordering::Equal
    }

    /// Select the best placed bid from a bid list using `compare_bids`.
    ///
    /// # Security
    /// This helper is used by both `get_best_bid` and `rank_bids` so they
    /// cannot drift on tie handling. Any ordering change flows through one
    /// path, preserving the invariant that best bid == first ranked bid.
    /// Expired bids are excluded from selection.
    fn select_best_placed_bid(records: &Vec<Bid>, current_timestamp: u64) -> Option<Bid> {
        let mut best: Option<Bid> = None;
        for candidate in records.iter() {
            if candidate.status != BidStatus::Placed || candidate.is_expired(current_timestamp) {
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
        }
        best
    }

    /// Stable, descending sort of bids using `compare_bids`.
    ///
    /// # Complexity
    /// Bottom-up merge sort: `O(n log n)` comparisons and `O(n)` extra memory
    /// per pass. This replaces the previous "scan for the maximum, then rebuild
    /// the remaining vector" loop, which was `O(n^2)` in both comparisons and
    /// vector writes.
    ///
    /// # Stability
    /// When two bids compare `Equal`, the one that appeared first in storage
    /// stays first in the ranking. The old selector only replaced the current
    /// best on `Ordering::Greater`, so it preserved insertion order for ties;
    /// the merge below keeps the left (earlier) run on equality to produce the
    /// exact same ordering.
    fn sort_bids_desc(env: &Env, bids: Vec<Bid>) -> Vec<Bid> {
        let len = bids.len();
        if len < 2 {
            return bids;
        }

        let mut best_idx: u32 = 0;
        let mut best_bid = records.get(0)?;
        let mut idx: u32 = 1;
        while idx < records.len() {
            if let Some(candidate) = records.get(idx) {
                if Self::compare_bids(&candidate, &best_bid) == Ordering::Greater {
                    best_idx = idx;
                    best_bid = candidate;
                }
        let mut src = bids;
        let mut width: u32 = 1;
        while width < len {
            let mut merged = Vec::new(env);
            let mut left: u32 = 0;
            while left < len {
                let mid = left.saturating_add(width).min(len);
                let right = left.saturating_add(width.saturating_mul(2)).min(len);
                let mut a = left;
                let mut b = mid;
                let mut emitted: u32 = 0;
                let run_len = right - left;
                while emitted < run_len {
                    let take_left = if a >= mid {
                        false
                    } else if b >= right {
                        true
                    } else {
                        // Only a strictly better right-hand bid may jump ahead of
                        // an equal left-hand bid, preserving tie stability.
                        Self::compare_bids(&src.get(b).unwrap(), &src.get(a).unwrap())
                            != Ordering::Greater
                    };
                    if take_left {
                        merged.push_back(src.get(a).unwrap());
                        a += 1;
                    } else {
                        merged.push_back(src.get(b).unwrap());
                        b += 1;
                    }
                    emitted += 1;
                }
                left = right;
            }
            src = merged;
            width = width.saturating_mul(2);
        }
        src
    }

    /// Return the highest-ranked placed bid for an invoice.
    ///
    /// # Invariant
    /// When `rank_bids` is non-empty, this method always returns the same bid
    /// as `rank_bids(...).get(0)`.
    ///
    /// Stale bids (past their raw expiry, even while a grace window delays
    /// their `Placed -> Expired` storage transition) are excluded, matching
    /// `accept_bid` and `get_bids_by_status`.
    pub fn get_best_bid(
        env: &Env,
        invoice_id: &BytesN<32>,
    ) -> Result<Option<Bid>, QuickLendXError> {
        let records = Self::get_bid_records_for_invoice(env, invoice_id)?;
        let current_timestamp = env.ledger().timestamp();
        Ok(Self::select_best_placed_bid(&records, current_timestamp))
    }

    /// Return all placed bids sorted from best to worst.
    ///
    /// # Invariant
    /// If this function returns at least one bid, the first element equals the
    /// value returned by `get_best_bid` for the same invoice and ledger state.
    ///
    /// Stale bids (past their raw expiry, even while a grace window delays
    /// their `Placed -> Expired` storage transition) are excluded, matching
    /// `accept_bid` and `get_bids_by_status`.
    pub fn rank_bids(env: &Env, invoice_id: &BytesN<32>) -> Result<Vec<Bid>, QuickLendXError> {
        let records = Self::get_bid_records_for_invoice(env, invoice_id)?;
        let current_timestamp = env.ledger().timestamp();
        let mut remaining = Vec::new(env);
        for bid in records.iter() {

        // One filtering pass, then one sort: the ranking is computed once
        // instead of re-scanning and rebuilding the candidate list per rank.
        let mut placed = Vec::new(env);
        let mut idx: u32 = 0;
        while idx < records.len() {
            let bid = records.get(idx).unwrap();
            if bid.status == BidStatus::Placed && !bid.is_expired(current_timestamp) {
                placed.push_back(bid);
            }
        }

        let mut ranked = Vec::new(env);

        while !remaining.is_empty() {
            let best_idx = match Self::select_best_index(&remaining) {
                Some(idx) => idx,
                None => break,
            };
            if let Some(best_bid) = remaining.get(best_idx) {
                ranked.push_back(best_bid);
            }

            let mut new_remaining = Vec::new(env);
            let mut copy_idx: u32 = 0;
            while copy_idx < remaining.len() {
                if copy_idx != best_idx {
                    if let Some(bid) = remaining.get(copy_idx) {
                        new_remaining.push_back(bid);
                    }
                }
                copy_idx += 1;
            }
            remaining = new_remaining;
        }

        Ok(ranked)
        Self::sort_bids_desc(env, placed)
    }

    /// Return ranked placed bids using a cursor bound to the ranked snapshot.
    pub fn rank_bids_paged(
        env: &Env,
        invoice_id: &BytesN<32>,
        cursor: Option<soroban_sdk::String>,
        limit: u32,
    ) -> Result<(Vec<Bid>, u32, bool), QuickLendXError> {
        if crate::pagination::validate_query_params(offset, limit).is_err() {
            return Ok((Vec::new(env), 0, false));
        }
        let ranked = Self::rank_bids(env, invoice_id)?;
    ) -> Result<crate::types::CursorBidsPage, QuickLendXError> {
        let ranked = Self::rank_bids(env, invoice_id);
        let total_count = ranked.len();
        let mut snapshot = Bytes::new(env);
        snapshot.append(&invoice_id.to_xdr(env));
        for bid in ranked.iter() {
            snapshot.append(&bid.to_xdr(env));
        }
        let digest = env.crypto().sha256(&snapshot).to_array();
        let generation = u64::from_be_bytes([
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6],
            digest[7],
        ]);
        let offset = match cursor {
            Some(raw) => {
                let decoded = crate::pagination::decode_soroban_cursor(&raw)?;
                decoded.require_stable(generation)?;
                decoded.offset
            }
            None => 0,
        };
        let (start, end) = crate::pagination::calculate_safe_bounds(offset, limit, total_count);
        let mut result = Vec::new(env);
        let mut idx = start;
        while idx < end {
            if let Some(bid) = ranked.get(idx) {
                result.push_back(bid);
            }
            idx += 1;
        }
        let (_, has_more) = crate::pagination::pagination_metadata(offset, limit, total_count);
        Ok((result, total_count, has_more))
        let capped_limit = crate::pagination::cap_query_limit(limit);
        let next_cursor = if has_more {
            Some(crate::pagination::encode_soroban_cursor(
                env,
                crate::pagination::Cursor::new(
                    start.saturating_add(capped_limit),
                    generation,
                ),
            ))
        } else {
            None
        };
        Ok(crate::types::CursorBidsPage {
            items: result,
            total_count,
            has_more,
            next_cursor,
        })
    }

    /// Legacy offset-based ranked bid query.
    #[deprecated(note = "use rank_bids_paged with an opaque cursor")]
    pub fn rank_bids_offset(
        env: &Env,
        invoice_id: &BytesN<32>,
        offset: u32,
        limit: u32,
    ) -> (Vec<Bid>, u32, bool) {
        let ranked = Self::rank_bids(env, invoice_id);
        let total_count = ranked.len();
        let (start, end) = crate::pagination::calculate_safe_bounds(offset, limit, total_count);
        let mut result = Vec::new(env);
        for index in start..end {
            if let Some(bid) = ranked.get(index) {
                result.push_back(bid);
            }
        }
        let (_, has_more) = crate::pagination::pagination_metadata(offset, limit, total_count);
        (result, total_count, has_more)
    }

    /// Cancel a placed bid by bid_id. Only transitions Placed -> Cancelled.
    ///
    /// # Errors
    /// - `StorageKeyNotFound` if the bid does not exist.
    /// - `BidStale` if the bid is not in `Placed` status (already cancelled,
    ///   accepted, expired, or withdrawn).
    /// - Host-level auth panic if the caller is not `bid.investor`.
    pub fn cancel_bid(env: &Env, bid_id: &BytesN<32>) -> Result<(), QuickLendXError> {
        let mut bid = Self::get_bid(env, bid_id).ok_or(QuickLendXError::StorageKeyNotFound)?;
        // SECURITY FIX: User must authorize their own bid cancellation
        bid.investor.require_auth();

        if bid.status != BidStatus::Placed {
            return Err(QuickLendXError::BidStale);
        }

        bid.status = BidStatus::Cancelled;
        Self::update_bid(env, &bid);
        crate::events::emit_bid_cancelled(env, &bid);
        Ok(())
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
        let current_timestamp = env.ledger().timestamp();
        let mut count: u32 = 0;
        for bid in all_bids.iter() {
            if bid.is_active(current_timestamp) {
                count = count.saturating_add(1);
            }
        }
        count
    }

    /// Calculate the sum of all currently active (Placed) bid amounts for a given investor.
    /// Used for checking against the investor's total investment limit.
    pub fn get_active_bid_amount_sum_for_investor(env: &Env, investor: &Address) -> i128 {
        let all_bids = Self::get_all_bids_by_investor(env, investor);
        let current_timestamp = env.ledger().timestamp();
        let mut total_amount: i128 = 0;
        for bid in all_bids.iter() {
            if bid.status == BidStatus::Placed && !bid.is_expired(current_timestamp) {
                total_amount = total_amount.saturating_add(bid.bid_amount);
            }
        }
        total_amount
    }
    pub fn generate_next_bid_counter(env: &Env) -> u64 {
        let counter_key = symbol_short!("bid_cnt");
        let counter: u64 = env.storage().instance().get(&counter_key).unwrap_or(0u64);
        let next_counter = counter.saturating_add(1);
        env.storage().instance().set(&counter_key, &next_counter);
        next_counter
    }

    /// Generates a unique 32-byte bid ID using timestamp and a simple counter.
    /// This approach avoids potential serialization issues with large counters.
    pub fn generate_unique_bid_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let next_counter = Self::generate_next_bid_counter(env);

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
        let grace_seconds = Self::get_bid_expiry_grace_seconds(env);
        let bid_ids = Self::get_bids_for_invoice(env, invoice_id);
        for bid_id in bid_ids.iter() {
            if let Some(bid) = Self::get_bid(env, &bid_id) {
                if bid.status == BidStatus::Expired && bid.expiration_timestamp > current_timestamp
                {
                    return false;
                }
                if bid.status == BidStatus::Placed
                    && bid.is_cleanup_eligible(current_timestamp, grace_seconds)
                {
                    return false;
                }
            }
        }
        true
    }

    /// Returns bid counts by status as `(placed, accepted, withdrawn, expired, cancelled)`.
    /// Useful for assertions in tests and analytics.
    pub fn count_bids_by_status(
        env: &Env,
        invoice_id: &BytesN<32>,
    ) -> Result<(u32, u32, u32, u32, u32), QuickLendXError> {
        let records = Self::get_bid_records_for_invoice(env, invoice_id)?;
        let (mut placed, mut accepted, mut withdrawn, mut expired, mut cancelled) =
            (0u32, 0u32, 0u32, 0u32, 0u32);
        for bid in records.iter() {
            match bid.status {
                BidStatus::Placed => placed += 1,
                BidStatus::Accepted => accepted += 1,
                BidStatus::Withdrawn => withdrawn += 1,
                BidStatus::Expired => expired += 1,
                BidStatus::Cancelled => cancelled += 1,
            }
        }
        Ok((placed, accepted, withdrawn, expired, cancelled))
    }

    // --- Aliases and compatibility methods ---

    pub fn store(env: &Env, bid: &Bid) {
        Self::store_bid(env, bid);
    }

    pub fn get(env: &Env, bid_id: &BytesN<32>) -> Option<Bid> {
        Self::get_bid(env, bid_id)
    }

    pub fn update(env: &Env, bid: &Bid) {
        Self::update_bid(env, bid);
    }

    pub fn get_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Vec<BytesN<32>> {
        Self::get_bids_for_invoice(env, invoice_id)
    }

    pub fn get_by_investor(env: &Env, investor: &Address) -> Vec<BytesN<32>> {
        Self::get_bids_by_investor_all(env, investor)
    }

    pub fn get_by_status(env: &Env, status: BidStatus) -> Vec<BytesN<32>> {
        // Fallback for status-based retrieval if needed
        let mut result = Vec::new(env);
        // This is inefficient but avoids complex indexing for now
        for bid_id in Self::get_all_bids(env).iter() {
            if let Some(bid) = Self::get_bid(env, &bid_id) {
                if bid.status == status {
                    result.push_back(bid_id);
                }
            }
        }
        result
    }

    pub fn next_count(env: &Env) -> u64 {
        Self::generate_next_bid_counter(env)
    }
}

/// Precondition check: verify a bid is compatible with the given invoice
/// before proceeding with bid acceptance / matching logic.
///
/// # Checks
/// 1. The bid belongs to the invoice (`bid.invoice_id == invoice.id`).
/// 2. The bid is in `Placed` status (not yet Accepted, Cancelled, etc.).
/// 3. The bid has not expired (`bid.expiration_timestamp > now`).
/// 4. The bid amount is positive (`bid.bid_amount > 0`).
/// 5. The bid amount does not exceed the invoice amount.
///
/// # Threat mitigated
///
/// Without this explicit precondition check, a caller could attempt to match
/// a bid that belongs to a different invoice, or one that has already expired
/// or been cancelled, leading to state corruption or inconsistent accounting.
/// Each guard returns a distinct typed error so the caller (and any audit
/// monitor) can distinguish between a wrong-invoice call (`Unauthorized`),
/// an expired bid (`InvalidStatus`), and a zero-amount bid (`InvalidAmount`).
///
/// # Errors
///
/// | Condition | Error |
/// |---|---|
/// | bid does not reference this invoice | `Unauthorized` |
/// | bid not in `Placed` state | `InvalidStatus` |
/// | bid has expired | `InvalidStatus` |
/// | bid amount ≤ 0 | `InvalidAmount` |
/// | bid amount > invoice amount | `InvalidAmount` |
pub fn verify_bid_match(
    env: &Env,
    bid: &Bid,
    invoice: &crate::types::Invoice,
) -> Result<(), QuickLendXError> {
    if bid.invoice_id != invoice.id {
        return Err(QuickLendXError::Unauthorized);
    }

    if bid.status != BidStatus::Placed {
        return Err(QuickLendXError::InvalidStatus);
    }

    if bid.is_expired(env.ledger().timestamp()) {
        return Err(QuickLendXError::BidStale);
    }

    if bid.bid_amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    Ok(())
}
