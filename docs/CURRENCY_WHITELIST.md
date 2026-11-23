# Currency Whitelist

Audience: **contributors** — people reading or modifying the contract code.

The currency whitelist is an admin-managed set of Soroban token addresses that are permitted as the denomination currency on invoices and bids. All enforcement is in [`quicklendx-contracts/src/currency.rs`](../quicklendx-contracts/src/currency.rs). The full per-function rustdoc lives in [`docs/contracts/currency-whitelist.md`](contracts/currency-whitelist.md).

---

## How it works

### Empty-list / allow-all mode

When the whitelist contains **zero entries**, every token address is accepted. This lets a freshly deployed contract operate without any initial admin setup and keeps existing tests working without modification.

The moment any currency is added via `add_currency` or `set_currencies`, the list becomes **restrictive**: only the listed addresses pass the check.

```
whitelist empty  →  require_allowed_currency always returns Ok(())
whitelist non-empty  →  require_allowed_currency returns Err(InvalidCurrency)
                        if the token is absent
```

### Enforcement points

`require_allowed_currency` is called in two places:

| Entrypoint | Where |
|---|---|
| `store_invoice` / `upload_invoice` | Before the invoice is written to storage |
| `place_bid` | Before the bid is accepted |

Passing a non-whitelisted token address at either point returns `InvalidCurrency` immediately — no partial state is written.

### Storage

The whitelist is stored under the instance storage key `curr_wl` (a `Symbol`) as a `Vec<Address>`. There is exactly one entry per unique token address (duplicates are collapsed on every write path).

---

## Contract entrypoints

These are the functions exposed on the contract trait in `lib.rs`:

| Function | Auth | Returns | Notes |
|---|---|---|---|
| `add_currency(admin, currency)` | Admin | `Result<(), QuickLendXError>` | Idempotent; no-op if already present |
| `remove_currency(admin, currency)` | Admin | `Result<(), QuickLendXError>` | No-op if absent |
| `add_currencies_batch(admin, currencies)` | Admin | `Result<Vec<bool>, QuickLendXError>` | `true` = newly added, `false` = already present |
| `remove_currencies_batch(admin, currencies)` | Admin | `Result<Vec<bool>, QuickLendXError>` | `true` = removed, `false` = was absent |
| `set_currencies(admin, currencies)` | Admin | `Result<(), QuickLendXError>` | Atomically replaces entire list; deduplicates input |
| `clear_currencies(admin)` | Admin | `Result<(), QuickLendXError>` | Resets to allow-all (empty) state |
| `is_allowed_currency(currency)` | Public | `bool` | Raw membership check; does **not** apply empty-list rule |
| `get_whitelisted_currencies()` | Public | `Vec<Address>` | Returns full list as stored |
| `get_whitelisted_currencies_paged(offset, limit)` | Public | `Vec<Address>` | Offset is 0-based; limit capped at 100 |
| `currency_count()` | Public | `u32` | Length of the stored list |

All write operations also check `PauseControl::require_not_paused` — they fail with `ContractPaused` while the contract is paused.

---

## Authentication model

Every state-mutating function enforces two independent checks:

1. **Storage check** — `AdminStorage::get_admin(env)` retrieves the stored admin address. Fails with `NotAdmin` if no admin has been initialised.
2. **Runtime auth** — `admin.require_auth()` asks the Soroban host to verify the transaction carries a valid signature for that address.

Both checks must pass. Passing only one is not sufficient to mutate state.

---

## Working with the whitelist in tests

The test helper pattern used throughout `test_currency.rs`:

```rust
// Initialise a contract with an admin
let env = Env::default();
env.mock_all_auths();
let contract_id = env.register_contract(None, QuickLendXContract);
let client = QuickLendXContractClient::new(&env, &contract_id);

let admin = Address::generate(&env);
let usdc  = Address::generate(&env);  // stands in for a real token contract address

client.initialize(&admin, /* ...other init params... */);

// Add a currency
client.add_currency(&admin, &usdc);
assert!(client.is_allowed_currency(&usdc));

// Remove it — immediately takes effect
client.remove_currency(&admin, &usdc);
assert!(!client.is_allowed_currency(&usdc));

// Atomic bulk replace
let eurc = Address::generate(&env);
client.set_currencies(&admin, &soroban_sdk::vec![&env, usdc.clone(), eurc.clone()]);
assert_eq!(client.currency_count(), 2);

// Back to allow-all
client.clear_currencies(&admin);
assert_eq!(client.currency_count(), 0);
```

`env.mock_all_auths()` satisfies `require_auth()` in unit tests. Do **not** use `std::` types — the contract crate is `#![no_std]`; use `soroban_sdk::Vec`, `soroban_sdk::vec![]`, etc.

---

## Adding a new currency in production

1. Obtain the Soroban token contract address for the token (e.g. USDC on Stellar mainnet).
2. Construct a transaction that calls `add_currency(admin, <token_address>)` signed by the admin key.
3. Submit and confirm on-chain. From the next ledger, invoices and bids denominated in that token are accepted.

For a batch update prefer `set_currencies` over multiple `add_currency` calls — it is a single storage write and avoids partial-state windows between transactions.

---

## Removing a currency

```
remove_currency(admin, <token_address>)
```

Takes effect immediately: any `store_invoice` or `place_bid` call using the removed token in the same or a later ledger will fail with `InvalidCurrency`. Existing invoices and open bids that were created before the removal are **not** retroactively invalidated — only new writes are blocked.

---

## Related docs

- [`docs/contracts/currency-whitelist.md`](contracts/currency-whitelist.md) — detailed per-function reference, pagination semantics, and full test inventory.
- [`docs/contracts/currency.md`](contracts/currency.md) — short function table overview.
- [`docs/PLATFORM_FEES.md`](PLATFORM_FEES.md) — fee schedule; fees are denominated in the invoice currency, so the whitelist indirectly governs which tokens flow through the fee path.
- [`quicklendx-contracts/src/currency.rs`](../quicklendx-contracts/src/currency.rs) — implementation source.
- [`quicklendx-contracts/src/test_currency.rs`](../quicklendx-contracts/src/test_currency.rs) — unit and boundary tests.
