# Soroban Storage TTL Mapping & Extension Policy

This document is intended for **Contributors** working on the QuickLendX Soroban smart contracts. It provides a comprehensive reference mapping each contract storage key to its respective Time-To-Live (TTL) category, storage class, and bump amounts, along with code examples to ensure correct TTL extension behavior.

## TTL Strategy Overview

Soroban requires state rent to be maintained for stored data. If an entry's TTL is not extended and falls to 0, it is archived (Persistent storage) or permanently deleted (Temporary storage). QuickLendX uses two storage classes:

1. **Persistent Storage**: Used for long-lived, user-specific data (e.g., invoices, bids, investments). Entries require explicit TTL extension using `extend_ttl`.
2. **Instance Storage**: Used for shared protocol configurations (e.g., admin settings, whitelists). Entries are automatically extended when the contract instance itself is extended.

---

## TTL Parameters & Constants

All persistent storage keys in QuickLendX are extended using a protocol-wide threshold and bump target defined in [`storage.rs`](file:///c:/Users/hp/quicklendx-protocol/quicklendx-contracts/src/storage.rs):

*   **`PERSISTENT_TTL_THRESHOLD`**: `34,732,800` (representing ~402 days in seconds, or `6,946,560` ledgers assuming 5 seconds per ledger).
*   **Threshold (`threshold`)**: If the remaining TTL of a persistent key is less than `PERSISTENT_TTL_THRESHOLD`, it is bumped.
*   **Extend Target (`extend_to`)**: The key's TTL is bumped to `PERSISTENT_TTL_THRESHOLD`.

---

## Storage Key TTL Map

| Storage Class | Key Name / Type | Serialized Key XDR / Format | Bump / TTL Policy | Operation / When Bumped |
| :--- | :--- | :--- | :--- | :--- |
| **Persistent** | `DataKey::Invoice(invoice_id)` | `DataKey::Invoice(BytesN<32>)` | `PERSISTENT_TTL_THRESHOLD` | On `store`, `get`, `update` |
| **Persistent** | `DataKey::Bid(bid_id)` | `DataKey::Bid(BytesN<32>)` | `PERSISTENT_TTL_THRESHOLD` | On `store_bid`, `get_bid`, `update_bid` |
| **Persistent** | `DataKey::Investment(investment_id)` | `DataKey::Investment(BytesN<32>)` | `PERSISTENT_TTL_THRESHOLD` | On `store_investment`, `get_investment`, `update_investment` |
| **Persistent** | `escrow_id` | `BytesN<32>` | `PERSISTENT_TTL_THRESHOLD` | On `store_escrow`, `get_escrow`, `update_escrow` |
| **Persistent** | `invoice_key` | `(Symbol("escrow"), BytesN<32>)` | `PERSISTENT_TTL_THRESHOLD` | On `store_escrow`, `get_escrow_by_invoice` |
| **Persistent** | `invoice_count` | `Symbol("inv_count")` | `PERSISTENT_TTL_THRESHOLD` | Updated on invoice count increment |
| **Persistent** | `bid_count` | `Symbol("bid_count")` | `PERSISTENT_TTL_THRESHOLD` | Updated on bid count increment |
| **Persistent** | `investment_count` | `Symbol("inv_cnt")` | `PERSISTENT_TTL_THRESHOLD` | Updated on investment count increment |
| **Persistent** | `invoices_by_business` | `(Symbol("inv_bus"), Address)` | `PERSISTENT_TTL_THRESHOLD` | On business index add/remove/read |
| **Persistent** | `invoices_by_status` | `(Symbol("inv_st"), Symbol)` | `PERSISTENT_TTL_THRESHOLD` | On status index add/remove/read |
| **Persistent** | `bids_by_invoice` | `(Symbol("bids_inv"), BytesN<32>)` | `PERSISTENT_TTL_THRESHOLD` | On invoice bid index add/remove/read |
| **Persistent** | `bids_by_investor` | `(Symbol("bids_invr"), Address)` | `PERSISTENT_TTL_THRESHOLD` | On investor bid index add/remove/read |
| **Persistent** | `bids_by_status` | `(Symbol("bids_stat"), Symbol)` | `PERSISTENT_TTL_THRESHOLD` | On status index add/remove/read |
| **Persistent** | `investments_by_invoice` | `(Symbol("invst_inv"), BytesN<32>)` | `PERSISTENT_TTL_THRESHOLD` | On invoice investment index add/remove/read |
| **Persistent** | `investments_by_investor` | `(Symbol("inv_invst"), Address)` | `PERSISTENT_TTL_THRESHOLD` | On investor investment index add/remove/read |
| **Persistent** | `investments_by_status` | `(Symbol("inv_st"), Symbol)` | `PERSISTENT_TTL_THRESHOLD` | On status index add/remove/read |
| **Persistent** | `invoices_by_customer` | `(Symbol("inv_cust"), String)` | `PERSISTENT_TTL_THRESHOLD` | On customer index add/remove/read |
| **Persistent** | `invoices_by_tax_id` | `(Symbol("inv_taxid"), String)` | `PERSISTENT_TTL_THRESHOLD` | On tax ID index add/remove/read |
| **Persistent** | `invoices_by_tag` | `(Symbol("inv_tag"), String)` | `PERSISTENT_TTL_THRESHOLD` | On tag index add/remove/read |
| **Persistent** | `invoices_by_category` | `(Symbol("inv_cat"), Symbol)` | `PERSISTENT_TTL_THRESHOLD` | On category index add/remove/read |
| **Persistent** | `held_reserve_key` | `(Symbol("esc_res"), Address)` | `PERSISTENT_TTL_THRESHOLD` | On held reserve get/set |
| **Persistent** | `reserve_marker_key` | `(Symbol("esc_acc"), BytesN<32>)` | `PERSISTENT_TTL_THRESHOLD` | On reserve marker set/get |
| **Persistent** | `held_reserve_repair_ids_key`| `(Symbol("esc_rids"), Address)`| `PERSISTENT_TTL_THRESHOLD` | On repair snapshot get/set |
| **Persistent** | `all_bids_key` | `Symbol("all_bids")` | `PERSISTENT_TTL_THRESHOLD` | On global bids index add/read |
| **Instance** | `ADMIN_KEY` | `Symbol("admin")` | Extended with Instance | Updated on admin transfer |
| **Instance** | `ADMIN_INITIALIZED_KEY` | `Symbol("adm_init")` | Extended with Instance | Updated on initialization |
| **Instance** | `ADMIN_TRANSFER_LOCK_KEY` | `Symbol("adm_lock")` | Extended with Instance | Used during admin transfers |
| **Instance** | `ADMIN_PENDING_KEY` | `Symbol("adm_pnd")` | Extended with Instance | Used during two-step transfers |
| **Instance** | `ADMIN_TWO_STEP_KEY` | `Symbol("adm_2st")` | Extended with Instance | Used during two-step config |
| **Instance** | `MAINTENANCE_MODE_KEY` | `Symbol("maint")` | Extended with Instance | Checked/set on maintenance toggle |
| **Instance** | `MAINTENANCE_REASON_KEY` | `Symbol("maint_rsn")` | Extended with Instance | Checked/set on maintenance reason write |
| **Instance** | `WHITELIST_KEY` | `Symbol("curr_wl")` | Extended with Instance | Updated on whitelist modify |
| **Instance** | `Platform Fee Config` | `Symbol("fees")` | Extended with Instance | Updated on platform fee changes |
| **Instance** | `RETENTION_POLICY_KEY` | `Symbol("bkup_pol")` | Extended with Instance | Updated on backup policy config |
| **Instance** | `BACKUP_COUNTER_KEY` | `Symbol("bkup_cnt")` | Extended with Instance | Updated on backup create |
| **Instance** | `BACKUP_LIST_KEY` | `Symbol("backups")` | Extended with Instance | Updated on backup list modify |
| **Instance** | `Backup Record` | `BytesN<32>` (backup_id) | Extended with Instance | Updated on backup record write |
| **Instance** | `Backup Data Record` | `(Symbol("bkup_data"), BytesN<32>)` | Extended with Instance | Updated on backup data record write |
| **Instance** | `Business Verification` | `Address` (business) | Extended with Instance | Updated on verification write |
| **Instance** | `Business lists` | `&'static str` (status keys) | Extended with Instance | Updated on verification lists modify |
| **Instance** | `Investor Verification` | `Address` (investor) | Extended with Instance | Updated on verification write |
| **Instance** | `Investor lists` | `&'static str` (status keys) | Extended with Instance | Updated on verification lists modify |
| **Instance** | `BID_TTL_KEY` | `Symbol("bid_ttl")` | Extended with Instance | Updated on bid TTL config changes |
| **Instance** | `MAX_ACTIVE_BIDS_PER_INVESTOR_KEY` | `Symbol("mx_actbd")` | Extended with Instance | Updated on investor bid limit changes |
| **Instance** | Platform Metrics | `Symbol("plt_met")` | Extended with Instance | Updated on platform metrics write |
| **Instance** | Performance Metrics | `Symbol("perf_met")` | Extended with Instance | Updated on performance metrics write |
| **Instance** | User Behavior Metrics | `(Symbol("usr_beh"), Address)` | Extended with Instance | Updated on behavior metrics write |
| **Instance** | Business Report | `(Symbol("biz_rpt"), BytesN<32>)` | Extended with Instance | Updated on report write |
| **Instance** | Investor Report | `(Symbol("inv_rpt"), BytesN<32>)` | Extended with Instance | Updated on report write |
| **Instance** | Investor Analytics | `(Symbol("inv_anal"), Address)` | Extended with Instance | Updated on analytics write |
| **Instance** | Investor Performance | `Symbol("inv_perf")` | Extended with Instance | Updated on performance metrics write |
| **Instance** | Analytics Data | `Symbol("analytics")` | Extended with Instance | Checked/set on analytics data write |
| **Instance** | Idempotency Key | `DataKey::IdempotencyKey(BytesN<32>)` | Extended with Instance | Updated on notification key check/set |
| **Instance** | Idempotency Key Set | `DataKey::IdempotencyKeySet` | Extended with Instance | Updated on notification key set modify |
| **Instance** | User Notifications | `DataKey::UserNotifications(Address)` | Extended with Instance | Updated on notifications write |
| **Instance** | User Preferences | `DataKey::UserPreferences(Address)` | Extended with Instance | Updated on preferences write |
| **Instance** | Notification | `DataKey::Notification(BytesN<32>)` | Extended with Instance | Updated on notification write |
| **Instance** | Notification Type | `DataKey::NotificationType(NotificationType)` | Extended with Instance | Updated on notification stats write |

---

## Implementing TTL Extensions in Contract Code

When creating new persistent entities or indexes, contributors must apply standard TTL extension guards. The contract provides the helper function `extend_persistent_ttl` to make this clean and uniform.

### Concrete Example: Storing and Retaining a Persistent Entity

```rust
use soroban_sdk::{contracttype, BytesN, Env};
use crate::storage::extend_persistent_ttl;

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Invoice(BytesN<32>),
}

// In the storage module:
pub fn store_invoice(env: &Env, invoice_id: &BytesN<32>, invoice: &Invoice) {
    let key = DataKey::Invoice(invoice_id.clone());
    
    // 1. Write the record to persistent storage
    env.storage().persistent().set(&key, invoice);
    
    // 2. Extend the key's TTL to PERSISTENT_TTL_THRESHOLD
    extend_persistent_ttl(env, &key);
}

pub fn get_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Invoice> {
    let key = DataKey::Invoice(invoice_id.clone());
    let result = env.storage().persistent().get(&key);
    
    if result.is_some() {
        // 3. Extend TTL on read to keep active invoices alive
        extend_persistent_ttl(env, &key);
    }
    result
}
```

### Concrete Example: Protocol-Wide TTL Maintenance

For keys that might not be read or written to frequently but must be kept alive, the protocol provides an admin-only routine called `extend_protocol_ttl`. Contributors adding new persistent keys must update this routine:

```rust
// From src/maintenance.rs
pub fn extend_protocol_ttl(
    env: &Env,
    admin: &Address,
) -> Result<ExtendReport, QuickLendXError> {
    AdminStorage::require_admin(env, admin)?;

    let mut report = ExtendReport {
        invoices_refreshed: 0,
        // ...
    };

    // Sweep through all keys and extend their TTLs
    for invoice_id in InvoiceStorage::get_all_invoice_ids(env).iter() {
        extend_persistent_ttl(env, &DataKey::Invoice(invoice_id.clone()));
        report.invoices_refreshed += 1;
    }
    
    // ...
    Ok(report)
}
```

---

## Testing TTL Survival

Contributors must add unit and integration tests to verify that data survives past default Soroban TTL thresholds after applying extension logic. Use the mock ledger timestamp to simulate the passage of time:

```rust
#[test]
fn test_ttl_extension_invoice_survival() {
    let env = Env::default();
    env.mock_all_auths();
    
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let business = Address::generate(&env);
    let invoice = create_test_invoice(&env, invoice_id.clone(), business.clone());

    // Store invoice (this executes extend_persistent_ttl internally)
    InvoiceStorage::store(&env, &invoice);

    // Advance ledger past default Soroban TTL (typically 180 days)
    // to simulate a long lifecycle. PERSISTENT_TTL_THRESHOLD is ~402 days.
    env.ledger().set_timestamp(35_000_000); // 35 million seconds (~405 days)

    // Verify the invoice record is still reachable and not archived
    let retrieved = InvoiceStorage::get(&env, &invoice_id);
    assert!(retrieved.is_some(), "Invoice should survive past default TTL");
}
```
