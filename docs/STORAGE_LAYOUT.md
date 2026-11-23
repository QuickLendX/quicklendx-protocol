# Soroban Storage Layout Decisions

This document is intended for **Contributors** working on the QuickLendX Soroban smart contracts. It explains our architectural decisions regarding Soroban's three storage models (Instance, Persistent, and Temporary) to ensure you choose the correct pattern for new features.

## 1. Instance Storage

**When we use it:**
For bounded, globally shared configuration data that the contract needs on almost every invocation.

**Why:**
Instance storage is loaded into memory automatically whenever the contract is invoked. This makes it highly efficient for reading global settings, but if too much data is placed here, the contract will hit memory limits and fail to load.

**Concrete Example:**
The protocol's administrator settings are stored in Instance storage. Because virtually all privileged operations must verify the admin's authorization, it is essential that this data is eagerly loaded.

```rust
// From src/admin.rs
const ADMIN_KEY: Symbol = symbol_short!("admin");

pub fn set_admin(env: &Env, admin: &Address, new_admin: &Address) {
    admin.require_auth();
    env.storage().instance().set(&ADMIN_KEY, new_admin);
}

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&ADMIN_KEY)
}
```

## 2. Persistent Storage

**When we use it:**
For unbounded lists, user-specific structures, and transactional records (like invoices or bids) that must survive indefinitely across contract upgrades.

**Why:**
Persistent items must be explicitly loaded via key lookup. This prevents the contract from loading unnecessary data. Persistent storage requires rent to be maintained via `extend_ttl`; if it expires, it is archived but can be restored later.

**Concrete Example:**
Individual bids are stored in Persistent storage keyed by their unique `bid_id`. If we put bids in Instance storage, the contract would quickly exceed its memory constraints as the protocol scales.

```rust
// From src/bid.rs
pub fn place_bid(...) -> BytesN<32> {
    let bid_id = ...;
    let bid = Bid { ... };
    
    // Explicitly write to persistent storage
    env.storage().persistent().set(&bid_id, &bid);
    
    bid_id
}
```

## 3. Temporary Storage

**When to use it:**
For ephemeral data that is only needed for a short period and can be permanently and safely discarded by the network if rent expires (it cannot be restored like Persistent storage).

**Current Usage in QuickLendX:**
**None.** We currently do not use Temporary storage in the protocol because our current state strictly models financial records (invoices, bids, escrows) which require strict persistence. 

**Hypothetical Example:**
If we implement short-lived, single-use signature nonces for off-chain intent validation in the future, Temporary storage would be the correct choice to save on state rent:

```rust
// Hypothetical Future Usage
pub fn consume_nonce(env: &Env, nonce: BytesN<32>) {
    let key = symbol_short!("nonce");
    if env.storage().temporary().has(&key) {
        panic!("Nonce already used");
    }
    // Set flag in temporary storage; safe if it drops after the transaction window
    env.storage().temporary().set(&key, &true);
}
```
