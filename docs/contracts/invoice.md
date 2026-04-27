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
