# Currency Whitelist Churn — Fuzz Harness

Property-based fuzz coverage for the admin-managed currency whitelist
(`src/currency.rs`), addressing issue **#1216**.

- **Harness:** `quicklendx-contracts/src/test_fuzz_currency_whitelist.rs`
- **Feature gate:** `#[cfg(all(test, feature = "fuzz-tests"))]`
- **Engine:** [`proptest`](https://docs.rs/proptest) (already an optional dep
  behind the `fuzz-tests` feature)

## What it verifies

The four mutating entrypoints all write to the same instance-storage key under
admin authorization:

| Entrypoint                              | Documented semantics                                  |
|-----------------------------------------|-------------------------------------------------------|
| `add_currency(admin, c)`                | append `c` iff absent — **idempotent**                |
| `remove_currency(admin, c)`             | drop `c` — **no-op when absent**                      |
| `set_currencies(admin, cs)`             | atomic replace, **first-occurrence dedupe**           |
| `clear_currencies(admin)`               | replace with empty list (allow-all mode)              |

The harness asserts one central property:

> **Deterministic replay.** For any random interleaving ("churn") of the four
> actions, the post-state of the live contract is byte-for-byte equal to the
> state produced by replaying the same actions against a pure in-memory model
> encoding the semantics above.

After **every** action it also checks the read oracles:

- `is_allowed_currency(c)` matches model membership — checked both for the key
  just touched and via a full sweep over the entire address pool;
- `currency_count()` equals the model length;
- ordering is preserved element-by-element (first-occurrence order).

### Test inventory

| Test                                        | Kind        | Property |
|---------------------------------------------|-------------|----------|
| `fuzz_whitelist_churn_matches_model`        | proptest    | Live state == deterministic replay after every step; read oracles agree |
| `fuzz_remove_then_add_cycle_idempotent`     | proptest    | `remove(c)`→`add(c)` cycles restore an exact, stable baseline |
| `edge_add_idempotent_no_growth`             | unit edge   | Repeated `add` of the same key never grows the list |
| `edge_remove_absent_is_noop`                | unit edge   | Removing an absent key (and removing from an empty list) is a no-op |
| `edge_set_dedupes_and_replaces`             | unit edge   | `set` collapses duplicates and fully replaces (never merges) |
| `edge_set_empty_then_clear`                 | unit edge   | `set([])` and `clear()` both yield the empty list |
| `auth::non_admin_cannot_add`               | security    | Non-admin `add` → `NotAdmin`, state unchanged |
| `auth::non_admin_cannot_remove`            | security    | Non-admin `remove` → `NotAdmin`, state unchanged |
| `auth::non_admin_cannot_set_even_with_duplicates` | security | Non-admin `set` with duplicate payload → `NotAdmin` **before** any dedupe |
| `auth::non_admin_cannot_clear`             | security    | Non-admin `clear` → `NotAdmin`, state unchanged |

## Model

Actions are generated as small indices into a fixed pool of six distinct
addresses, which keeps proptest shrinking cheap while guaranteeing collisions,
re-adds, and duplicate `set` payloads occur frequently. The reference model
(`Model`) stores pool indices in first-occurrence order and mirrors the exact
documented behaviour of each entrypoint.

## Running

```bash
cd quicklendx-contracts

# Default proptest budget (fast feedback)
cargo test --features fuzz-tests test_fuzz_currency_whitelist

# Assignment requirement: at least 30,000 random sequences
PROPTEST_CASES=30000 cargo test --features fuzz-tests test_fuzz_currency_whitelist

# Reproduce a specific failure
PROPTEST_SEED=<seed> cargo test --features fuzz-tests test_fuzz_currency_whitelist
```

Failing cases are persisted to `proptest-regressions/` and replayed on
subsequent runs.

## Edge cases covered

- Empty-list states (start, after `clear`, after `set([])`).
- Duplicate `add` (idempotency) and duplicate `set` payloads (dedupe).
- Removing an absent key and removing from an empty list (no-op).
- Re-adding a previously removed key (remove→add cycles, repeated).
- `set` replacing — never merging — a pre-existing non-empty list.
- First-occurrence ordering preserved across arbitrary churn.

## Security note — admin auth coverage

Every mutating operation is guarded by the stored-admin identity check
(`AdminStorage::require_admin` / explicit admin comparison) **plus**
`require_auth()`. The `auth` sub-module asserts that a non-admin caller is
rejected with `NotAdmin` for all four mutators and that the whitelist is left
untouched.

Crucially, the `set_currencies` auth test supplies a **duplicate-laden**
payload: because the admin check runs *before* dedupe, this proves that no
silent dedupe path can bypass authorization — i.e. "no silent dedupe drops
authorization checks." Together, these tests ensure the deterministic-replay
property can only ever be reached by the legitimate admin.
