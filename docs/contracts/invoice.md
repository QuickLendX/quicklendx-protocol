# Invoice Module — ID Generation & Collision Prevention

## Overview

Every invoice in QuickLendX is identified by a deterministic, collision-resistant
32-byte ID. The ID encodes the ledger state at creation time plus a per-contract
monotonic counter, making it unique across all ledger slots and all concurrent
allocations within the same slot.

## Invoice ID Layout (32 bytes)

```
 Byte offset  │ Width  │ Field       │ Description
──────────────┼────────┼─────────────┼──────────────────────────────────────────
  0 ..  8     │ 8 B    │ timestamp   │ Ledger timestamp (u64, big-endian)
  8 .. 12     │ 4 B    │ sequence    │ Ledger sequence number (u32, big-endian)
 12 .. 16     │ 4 B    │ counter     │ Monotonic per-contract counter (u32, BE)
 16 .. 32     │ 16 B   │ reserved    │ Zeroed — reserved for future use
```

### Why three fields?

| Scenario | Distinguishing field |
|----------|---------------------|
| Two invoices in the same ledger slot | `counter` |
| Two invoices in different ledger slots, same sequence | `timestamp` |
| Two invoices at the same timestamp, different blocks | `sequence` |
| Two invoices in completely different ledger states | `timestamp` + `sequence` |

Because no two distinct invoices can share all three fields simultaneously,
collisions are structurally impossible under normal operation.

## Counter Storage

The counter is stored in contract **instance storage** under the key
`symbol_short!("inv_cnt")`. It starts at `0` for a fresh contract and
increments by exactly `1` for each allocation.

```
StorageKeys::investment_count() → symbol_short!("inv_cnt")
```

## Collision Prevention Algorithm

```
1. Read current counter value C from instance storage (default 0).
2. Construct candidate ID from (timestamp, sequence, C).
3. If candidate ID already exists in persistent storage → increment C, goto 2.
4. Store invoice under candidate ID.
5. Write C + 1 back to instance storage.
```

Step 3 is the **collision skip**: even if the counter is rewound by external
storage manipulation, the allocator will never overwrite an existing invoice.

## Security Assumptions

1. **Collision resistance**: Two invoices cannot share the same ID because the
   counter is strictly monotonic within a ledger slot, and the timestamp +
   sequence distinguish different slots.

2. **No predictable overwrite**: A counter rewind (e.g., via a storage bug or
   deliberate manipulation) cannot silently overwrite an existing invoice. The
   allocator detects the occupied slot and advances the counter.

3. **Determinism**: Given the same ledger state (timestamp + sequence) and the
   same counter value, the generated ID is always identical. This makes IDs
   reproducible and auditable.

4. **Reserved bytes are zeroed**: Bytes 16–31 are always `0x00`. Any non-zero
   value in this range indicates a corrupted or externally-crafted ID.

5. **No cross-entity collisions**: The `DataKey::Invoice(id)` storage key wraps
   the invoice ID with a discriminant tag, so an invoice ID and a bid ID with
   the same 32-byte value produce distinct storage keys.

## Test Coverage

All invariants above are codified in
`src/test_invoice_id_collision_regression.rs` (issue #821).

| Test | What is verified |
|------|-----------------|
| `ids_unique_within_same_ledger_slot` | 24 IDs in one slot are all distinct |
| `counter_segment_encodes_big_endian` | Counter bytes are big-endian at 0, 1, 255, 256, MAX |
| `ids_unique_across_different_timestamps` | Same counter, different timestamps → distinct |
| `ids_unique_across_different_sequence_numbers` | Same timestamp, different sequences → distinct |
| `ids_unique_across_five_ledger_slots` | 5 distinct slots, counter=0 each → all distinct |
| `reserved_bytes_always_zeroed` | Bytes 16–31 are 0x00 for all boundary inputs |
| `counter_increments_strictly_by_one` | Counter advances by exactly 1 per allocation |
| `counter_starts_at_zero_for_fresh_contract` | Fresh contract counter = 0 |
| `counter_rewind_skips_occupied_slot` | Rewind to 0 → next ID uses counter 1 |
| `multiple_counter_rewinds_skip_all_occupied_slots` | 3 occupied slots skipped correctly |
| `allocator_resumes_monotonically_after_collision_skip` | Post-skip IDs are 1, 2, 3 |
| `different_businesses_same_slot_get_distinct_ids` | Per-contract counter isolates businesses |
| `id_generation_is_deterministic` | Same inputs → same ID every time |
| `id_generation_is_environment_independent` | Two Env instances, same state → same ID |
| `id_at_zero_boundary` | All-zero inputs → all-zero ID |
| `id_at_max_boundary` | MAX inputs → correct encoding, reserved bytes zeroed |
| `id_counter_min_and_max_are_distinct` | counter=0 ≠ counter=MAX |
| `timestamp_segment_reflects_ledger_timestamp` | 5 timestamp boundary values |
| `sequence_segment_reflects_ledger_sequence` | 5 sequence boundary values |
| `ids_differing_only_in_counter_are_distinct` | 10 consecutive counter values |
| `ids_differing_only_in_timestamp_are_distinct` | Timestamp-only difference |
| `ids_differing_only_in_sequence_are_distinct` | Sequence-only difference |
| **Total** | **22 passed, 0 failed** |

## Running the Tests

```bash
cd quicklendx-contracts
cargo test --lib test_invoice_id_collision_regression
```

Expected output:
```
running 22 tests
test result: ok. 22 passed; 0 failed; 0 ignored
```

## Metadata Bounds and Normalization

To prevent unbounded storage growth and ambiguous query keys, invoice metadata
enforces strict limits and canonicalization rules.

### Bounded vectors

- Invoice tags: maximum `10` normalized tags per invoice.
- Structured metadata line items: maximum `100` line items.
- Invoice ratings: maximum `100` ratings retained per invoice.

Any attempt to exceed these bounds is rejected before storage mutation.

### Tag normalization rules

Tags are canonicalized using trim + ASCII lowercase before validation and
duplicate checks. As a result:

- `" Tech "`, `"tech"`, and `"TECH"` are treated as the same tag.
- Duplicate canonical tags are rejected by invoice tag validation.
- Per-invoice tag growth is capped even when tags are submitted in different
  case/whitespace variants.

### Security notes

- Oversized metadata payloads are rejected early, reducing compute/storage DoS
  surface.
- Canonical duplicate handling prevents ambiguous indexing/query behavior.
- Rating/tag caps keep per-invoice state growth predictable over time.

---

## Batch Invoice Creation — `store_invoices_batch`

### Motivation

Businesses frequently issue multiple invoices per billing cycle. Before this
feature, each invoice required a separate transaction (auth + ledger fees ×
N). `store_invoices_batch` lets a verified business submit up to
`MAX_BATCH_INVOICES` (currently **10**) invoices in a single transaction.

### Entrypoint signature

```rust
pub fn store_invoices_batch(
    env: Env,
    business: Address,
    inputs: Vec<InvoiceInput>,
) -> Result<Vec<BytesN<32>>, QuickLendXError>
```

| Parameter  | Description |
|------------|-------------|
| `business` | Invoice-issuing business address (must sign). |
| `inputs`   | `Vec<InvoiceInput>` — 1 to `MAX_BATCH_INVOICES` entries. |

Returns an ordered `Vec<BytesN<32>>` of newly assigned invoice IDs, one per
input entry.

### `InvoiceInput` struct

```rust
pub struct InvoiceInput {
    pub amount:      i128,
    pub currency:    Address,
    pub due_date:    u64,
    pub description: String,
    pub category:    InvoiceCategory,
    pub tags:        Vec<String>,
}
```

Fields are identical to the `upload_invoice` parameters.

### Semantics

| Property | Behaviour |
|----------|-----------|
| **Auth** | `business.require_auth()` is called once for the whole batch. |
| **KYC** | Business must be `Verified` (same rule as `upload_invoice`). |
| **Batch size** | `1 ≤ len(inputs) ≤ MAX_BATCH_INVOICES`; otherwise `BatchSizeExceeded` (2206). |
| **Active-invoice cap** | The *entire* batch is pre-checked: `active_count + batch_len ≤ max_invoices_per_business`. |
| **Atomicity** | All-or-nothing. A validation error on any entry rolls back the whole batch (Soroban tx semantics). |
| **Two-pass validation** | All inputs are validated *before* any storage write, making the atomicity guarantee easy to reason about. |
| **Events** | An `invoice_uploaded` event is emitted for each successfully stored invoice (same event as `upload_invoice`). |

### Error codes returned

| Code | Symbol | Meaning |
|------|--------|---------|
| 2100 | `PAUSED` | Protocol is paused. |
| 2206 | `BATCH_SZ` | `inputs` is empty or exceeds `MAX_BATCH_INVOICES`. |
| 1600 | `BUS_NV` | Business has no KYC record or was rejected. |
| 1601 | `KYC_PD` | Business KYC is still pending admin review. |
| 1408 | `MAX_INV` | Batch would push the business over its active-invoice cap. |
| 1200 | `INV_AMT` | At least one invoice has an invalid amount. |
| 1004 | `INV_DI` | At least one invoice has a due date in the past or too far in the future. |

### Constant location

`MAX_BATCH_INVOICES` is defined in `src/protocol_limits.rs`:

```rust
pub const MAX_BATCH_INVOICES: u32 = 10;
```

It is the single source of truth. Both the entrypoint guard and the
`BatchSizeExceeded` error doc reference this constant.

### Example (Rust test / Soroban SDK)

```rust
let mut inputs: Vec<InvoiceInput> = Vec::new(&env);
for i in 0..3 {
    inputs.push_back(InvoiceInput {
        amount:      10_000,
        currency:    token.clone(),
        due_date:    env.ledger().timestamp() + 86_400 + i * 60,
        description: String::from_str(&env, "Batch invoice"),
        category:    InvoiceCategory::Services,
        tags:        Vec::new(&env),
    });
}

let ids = client.store_invoices_batch(&business, &inputs);
// ids.len() == 3; each ID is a distinct BytesN<32>
```

### Test coverage

Tests live in `src/test_store_invoices_batch.rs` and run with every `cargo
test` invocation (no feature flag required).

| Test | What is verified |
|------|-----------------|
| `test_batch_single_invoice` | Single-item batch stores one invoice. |
| `test_batch_multiple_invoices` | Multi-item batch: IDs distinct, all Pending. |
| `test_batch_max_size_succeeds` | Exactly `MAX_BATCH_INVOICES` entries accepted. |
| `test_batch_empty_rejected` | Empty vec → `BatchSizeExceeded`. |
| `test_batch_oversized_rejected` | `MAX_BATCH_INVOICES + 1` entries → `BatchSizeExceeded`. |
| `test_batch_respects_active_invoice_cap` | Cap enforced; exact remaining headroom works. |
| `test_batch_unverified_business_rejected` | No-KYC business rejected. |
| `test_batch_pending_business_rejected` | Pending-KYC business → `KYCAlreadyPending`. |
| `test_batch_bad_input_aborts_entirely` | Bad amount in second entry → zero invoices stored. |

```bash
cargo test test_store_invoices_batch
```

---

## Batch Invoice Cancellation — `invoice_batch_cancel`

### Motivation

Businesses frequently cancel multiple unpaid/pending invoices in bulk before issuing new ones or reorganizing billing. Calling `cancel_invoice` N times requires N separate transactions, each incurring authorization checks and round-trip latency. `invoice_batch_cancel` allows a verified business to cancel up to `MAX_BATCH_INVOICES` (currently **10**) invoices in a single atomic transaction.

### Entrypoint signature

```rust
pub fn invoice_batch_cancel(
    env: Env,
    business: Address,
    invoice_ids: Vec<BytesN<32>>,
) -> Result<(), QuickLendXError>
```

| Parameter | Description |
|-----------|-------------|
| `business` | Invoice-issuing business address (must sign). |
| `invoice_ids` | `Vec<BytesN<32>>` — 1 to `MAX_BATCH_INVOICES` entries. |

### Semantics

| Property | Behaviour |
|----------|-----------|
| **Auth** | `business.require_auth()` is called once for the whole batch. |
| **KYC** | Business must be active and `Verified` (same requirement as `cancel_invoice`). |
| **Batch size** | `1 ≤ len(invoice_ids) ≤ MAX_BATCH_INVOICES`; otherwise `BatchSizeExceeded` (2206). |
| **Ownership & Pre-flight** | Every invoice in the batch must exist, belong to `business`, and not be frozen. Validated in a pre-flight pass before state mutations. |
| **Atomicity** | All-or-nothing. An error on any single item (e.g. non-existent ID, frozen invoice, unauthorized owner) aborts the entire batch (Soroban tx semantics). |
| **Events** | An `invoice_cancelled` event is emitted for each successfully cancelled invoice. |

### Error codes returned

| Code | Symbol | Meaning |
|------|--------|---------|
| 2100 | `PAUSED` | Protocol is paused. |
| 2206 | `BATCH_SZ` | `invoice_ids` is empty or exceeds `MAX_BATCH_INVOICES`. |
| 1600 | `BUS_NV` | Business has no KYC record or was rejected. |
| 1601 | `KYC_PD` | Business KYC is still pending admin review. |
| 1000 | `INV_NF` | At least one invoice ID in the batch was not found. |
| 1007 | `INV_FZ` | At least one invoice ID in the batch is frozen. |
| 1100 | `UNAUTH` | At least one invoice in the batch does not belong to the calling business. |

### Example (Rust test / Soroban SDK)

```rust
let mut ids: Vec<BytesN<32>> = Vec::new(&env);
ids.push_back(invoice_id_1);
ids.push_back(invoice_id_2);

client.invoice_batch_cancel(&business, &ids);
```

### Test coverage

Tests live in `src/test_invoice_batch_cancel.rs` and run with every `cargo test` invocation.

| Test | What is verified |
|------|-----------------|
| `test_invoice_batch_cancel_single_success` | Single-item batch cancels one invoice. |
| `test_invoice_batch_cancel_multiple_success` | Multi-item batch cancels all specified invoices. |
| `test_invoice_batch_cancel_max_size_success` | Exactly `MAX_BATCH_INVOICES` entries accepted. |
| `test_invoice_batch_cancel_empty_rejected` | Empty vec → `BatchSizeExceeded`. |
| `test_invoice_batch_cancel_oversized_rejected` | `MAX_BATCH_INVOICES + 1` entries → `BatchSizeExceeded`. |
| `test_invoice_batch_cancel_unverified_business_rejected` | No-KYC business rejected. |
| `test_invoice_batch_cancel_pending_business_rejected` | Pending-KYC business → `KYCAlreadyPending`. |
| `test_invoice_batch_cancel_nonexistent_invoice_aborts` | Missing ID aborts batch with zero state mutations. |
| `test_invoice_batch_cancel_unauthorized_business_aborts` | Invoice owned by another business aborts batch. |
| `test_invoice_batch_cancel_frozen_invoice_aborts` | Frozen invoice in batch aborts entire operation. |

```bash
cargo test test_invoice_batch_cancel
```

