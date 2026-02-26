use core::cmp::Ordering;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol, Vec};

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::events::emit_bid_expired;

// TTL stored in days (admin configurable). Defaults to 7 days. Bounds: 1..=30
const DEFAULT_BID_TTL_DAYS: u64 = 7;
const MIN_BID_TTL_DAYS: u64 = 1;
const MAX_BID_TTL_DAYS: u64 = 30;
const BID_TTL_KEY: Symbol = symbol_short!("bid_ttl");
const SECONDS_PER_DAY: u64 = 86400;

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

    /// Get configured bid TTL in days (returns default if not set)
    pub fn get_bid_ttl_days(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&BID_TTL_KEY)
            .unwrap_or(DEFAULT_BID_TTL_DAYS)
    }

    /// Admin-only: set bid TTL in days. Enforces bounds.
    pub fn set_bid_ttl_days(env: &Env, admin: &Address, days: u64) -> Result<u64, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        if days < MIN_BID_TTL_DAYS || days > MAX_BID_TTL_DAYS {
            return Err(QuickLendXError::InvalidAmount);
        }

        env.storage().instance().set(&BID_TTL_KEY, &days);
        Ok(days)
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
                if bid.status == BidStatus::Placed && bid.is_expired(current_timestamp) {
                    bid.status = BidStatus::Expired;
                    Self::update_bid(env, &bid);
                    emit_bid_expired(env, &bid);
                    expired += 1;
                } else {
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
}
