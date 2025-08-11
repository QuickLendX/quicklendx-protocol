use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BidStatus {
    Placed,
    Withdrawn,
    Accepted,
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
    /// Generates a unique 32-byte bid ID using timestamp and a simple counter.
    /// This approach avoids potential serialization issues with large counters.
    pub fn generate_unique_bid_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let counter_key = symbol_short!("bid_cnt");
        let mut counter: u64 = env.storage().instance().get(&counter_key).unwrap_or(0u64);
        counter += 1;
        env.storage().instance().set(&counter_key, &counter);

        let mut bytes = [0u8; 32];
        // Add bid prefix to distinguish from other entity types
        bytes[0] = 0xB1; // 'B' for Bid
        bytes[1] = 0xD0; // 'D' for biD
                         // Embed timestamp in next 8 bytes
        bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        // Embed counter in next 8 bytes
        bytes[10..18].copy_from_slice(&counter.to_be_bytes());
        // Fill remaining bytes with a pattern to ensure uniqueness
        for i in 18..32 {
            bytes[i] = ((timestamp + counter as u64 + 0xB1D0) % 256) as u8;
        }
        BytesN::from_array(env, &bytes)
    }
}
