use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Vec};

use crate::{errors::QuickLendXError, events};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BidStatus {
    Placed,
    Withdrawn,
    Accepted,
    Expired,
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
}

pub struct BidStorage;

impl BidStorage {
    pub fn store_bid(env: &Env, bid: &Bid) {
        env.storage().instance().set(&bid.bid_id, bid);
    }

    pub fn get_bid(env: &Env, bid_id: &BytesN<32>) -> Option<Bid> {
        env.storage().instance().get(bid_id)
    }

    pub fn update_bid(env: &Env, bid: &Bid) {
        env.storage().instance().set(&bid.bid_id, bid);
    }

    pub fn get_bids_for_invoice(env: &Env, invoice_id: &BytesN<32>) -> Vec<BytesN<32>> {
        let key = (symbol_short!("bids"), invoice_id.clone());
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env))
    }

    pub fn add_bid_to_invoice(env: &Env, invoice_id: &BytesN<32>, bid_id: &BytesN<32>) {
        let mut bids = Self::get_bids_for_invoice(env, invoice_id);
        bids.push_back(bid_id.clone());
        let key = (symbol_short!("bids"), invoice_id.clone());
        env.storage().instance().set(&key, &bids);
    }

    pub fn generate_unique_bid_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let counter_key = symbol_short!("bid_cnt");
        let mut counter: u64 = env.storage().instance().get(&counter_key).unwrap_or(0u64);
        counter += 1;
        env.storage().instance().set(&counter_key, &counter);

        let mut bytes = [0u8; 32];
        bytes[0] = 0xB1;
        bytes[1] = 0xD0;
        bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        bytes[10..18].copy_from_slice(&counter.to_be_bytes());
        for i in 18..32 {
            bytes[i] = ((timestamp + counter as u64 + 0xB1D0) % 256) as u8;
        }
        BytesN::from_array(env, &bytes)
    }

    pub fn clean_expired_bids(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
        let bids = Self::get_bids_for_invoice(env, invoice_id);
        let current_timestamp = env.ledger().timestamp();
        let mut updated_bids = Vec::new(env);
        let mut any_expired = false;

        for bid_id in bids.iter() {
            if let Some(mut bid) = Self::get_bid(env, &bid_id) {
                if bid.status == BidStatus::Placed && bid.is_expired(current_timestamp) {
                    bid.status = BidStatus::Expired;
                    Self::update_bid(env, &bid);
                    any_expired = true;
                    events::emit_bid_expired(env, &bid_id, &invoice_id, &bid.investor);
                } else if bid.status != BidStatus::Expired {
                    // Only keep non-expired bids in the list
                    updated_bids.push_back(bid_id);
                }
            }
        }

        // Always update the bids list to ensure consistency
        let key = (symbol_short!("bids"), invoice_id.clone());
        env.storage().instance().set(&key, &updated_bids);

        Ok(())
    }
}
