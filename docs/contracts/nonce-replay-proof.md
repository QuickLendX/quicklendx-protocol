# Nonce Replay-Window Proof — `credence_delegation`

> **Module**: `contracts/credence_delegation/src/nonce.rs`  
> **Constant**: `MAX_NONCE_INVALIDATION_SPAN = 10_000`  
> **Test harness**: `contracts/credence_delegation/tests/nonce_replay.rs`

---

## 1. Definitions

| Symbol | Meaning |
|--------|---------|
| `current` | The stored nonce value for a specific delegator. Initialised to some `N₀ ≥ 0`. |
| `payload.nonce` | The nonce embedded in a pre-signed delegation payload. |
| `consume(n)` | Attempt to execute a payload with nonce `n`. Succeeds iff `n == current`; advances `current` to `current + 1` on success. |
| `invalidate_nonce_range(m)` | Advance `current` to `m` atomically, provided `m > current` and `m - current ≤ MAX_NONCE_INVALIDATION_SPAN`. |
| `MAX_NONCE_INVALIDATION_SPAN` | 10 000 — the maximum single-call advance. |

---

## 2. Core Invariant

**Monotonicity**: `current` is strictly non-decreasing across the lifetime of a `NonceStore`.

*Proof.*  
`current` is modified in exactly two places:
1. `consume` — sets `current ← current + 1` (increase by 1).
2. `invalidate_nonce_range` — sets `current ← new_nonce` where `new_nonce > current` (strict increase).

No code path decreases `current`.  
Both paths are guarded by precondition checks that return an error (leaving `current` unchanged) before any mutation occurs. ∎

---

## 3. Replay-Window Theorem

**Theorem**: Let `S` be a `NonceStore`. After any finite sequence of `consume` and `invalidate_nonce_range` calls that leaves `current = N`, the call `consume(k)` returns `Err(NonceError::InvalidNonce)` for every `k < N` — and will continue to do so for all future states of `S`.

**Proof.**

*Step 1 — Immediate rejection after the call.*  
After the sequence, `current = N`.  
`consume(k)` checks `k == current`.  
For any `k < N`: `k < N = current`, so `k ≠ current`, so the check fails and `Err(InvalidNonce)` is returned.

*Step 2 — Rejection in all future states.*  
By the Monotonicity Invariant, every future state has `current ≥ N`.  
For any `k < N` and any future `current' ≥ N > k`: `k ≠ current'`, so `consume(k)` returns `Err(InvalidNonce)`.

*Step 3 — Coverage of the invalidated range.*  
`invalidate_nonce_range(new_nonce)` advances `current` from `N_before` to `new_nonce`.  
The set of nonces made newly unspendable by this call is `{N_before, N_before+1, …, new_nonce−1}`, i.e. the half-open interval `[N_before, new_nonce)`.  
Combined with the previously-unspendable set `[0, N_before)`, the full unspendable set after the call is `[0, new_nonce)`. ∎

---

## 4. Half-Open Range Semantics

```
Before invalidate_nonce_range(new_nonce):

   0 ─── … ─── N_before ─── … ─── new_nonce ─── … ─── ∞
   ├──── already invalid ───┤       └── future nonces
                             └── current = N_before

After invalidate_nonce_range(new_nonce):

   0 ─── … ─── N_before ─── … ─── new_nonce ─── … ─── ∞
   ├────────── all invalid ────────┤
                                   └── current = new_nonce (first valid)
```

- `n < new_nonce` → invalid (Err(InvalidNonce)).
- `n == new_nonce` → valid (the next spendable nonce).
- `n > new_nonce` → invalid (out-of-order; nonces must be used strictly in sequence).

---

## 5. The `MAX_NONCE_INVALIDATION_SPAN` Bound

`invalidate_nonce_range` rejects any call where `new_nonce - current > MAX_NONCE_INVALIDATION_SPAN`.

### Why the bound is necessary

| Risk without bound | Mitigation |
|--------------------|-----------|
| **Accidental lockout**: a single buggy call to `invalidate_nonce_range(u64::MAX)` would push `current` to `u64::MAX`, making the delegation channel permanently unusable. | Cap limits a single mistake to at most 10 000 skipped nonces. |
| **On-chain gas abuse**: a smart-contract implementation that must iterate over the invalidated range (e.g. to emit events) would consume unbounded gas. | O(10 000) worst case is predictable and auditable. |
| **Replay-window DoS**: an attacker with brief write access could lock out a legitimate delegator by advancing `current` far into the future. | Caps how far each call can advance. |

### Why 10 000

- Matches the BPS denominator (`10_000 = 100%`) used throughout QuickLendX, making it easy to reason about in context.
- Large enough to cover any realistic operational batch (e.g. revoking all pre-signed payloads from a compromised signing key in a single transaction).
- Small enough that the worst-case gas cost is bounded and the unit test `nonce_replay_invalidation_by_max` completes in microseconds.

---

## 6. Threat Model

### 6.1 Attacker capabilities assumed

- The attacker holds one or more pre-signed delegation payloads (`payload.nonce = k`) that were valid at the time of signing.
- The attacker can submit those payloads to the protocol at any time.
- The attacker cannot forge new signatures (standard cryptographic assumption).
- The attacker cannot directly write to the `NonceStore` storage.

### 6.2 Attack vector: replay after key rotation / revocation

**Scenario**: The delegator signs a batch of payloads for nonces 0–999, then decides to revoke all of them (e.g. after a key compromise) and calls `invalidate_nonce_range(1_000)`.

**Outcome**: `current` advances to 1 000. Every payload with `nonce ∈ [0, 1 000)` will now return `Err(InvalidNonce)`. The attacker's payloads are permanently unspendable. ✓

### 6.3 Attack vector: span overflow / lockout

**Scenario**: An attacker (or buggy client) calls `invalidate_nonce_range(current + 10_001)`.

**Outcome**: `SpanTooLarge` is returned; `current` is unchanged. The delegation channel is unaffected. ✓

### 6.4 Attack vector: replay at the boundary nonce

**Scenario**: The delegator invalidates to `new_nonce`. The attacker attempts to use the payload with `nonce = new_nonce - 1` (the last nonce in the invalidated range).

**Outcome**: `new_nonce - 1 < new_nonce = current` → `Err(InvalidNonce)`. The boundary is correctly enforced as a strict less-than. ✓

### 6.5 Non-threat: sequential replay by the delegator

A delegator may still use nonces `≥ current` in strict order (one per `consume` call). This is the intended usage. Each consumed nonce advances `current` by exactly 1, so prior nonces can never be reused. ✓

### 6.6 Non-threat: concurrent invalidation calls

Multiple calls to `invalidate_nonce_range` are additive: each advances `current` forward. Because `current` is monotonically non-decreasing, no combination of valid calls can reduce `current` or make a previously-invalidated nonce reachable again. ✓

---

## 7. Property Tests — Summary

All tests live in `contracts/credence_delegation/tests/nonce_replay.rs`.

Run with:

```bash
cargo test -p credence_delegation nonce_replay
```

### 7.1 Property tests (10 000 random cases each)

| Test name | What it covers |
|-----------|----------------|
| `prop_invalidated_nonces_are_unspendable` | Core claim: ∀ `k < new_nonce`, `consume(k)` = `InvalidNonce` after invalidation. |
| `prop_minimal_span_invalidates_exactly_one_nonce` | Span = 1: the single skipped nonce is invalid; `start + 1` is valid. |
| `prop_max_span_boundary_nonce_rejected` | Span = MAX: boundary nonce `new_nonce - 1` is invalid; `new_nonce` is valid. |
| `prop_over_max_span_always_rejected` | Span > MAX: always `SpanTooLarge`; store unchanged. |
| `prop_first_valid_nonce_accepted` | `new_nonce` is the first accepted nonce after invalidation. |
| `prop_future_nonces_rejected` | Nonces > `new_nonce` are also invalid (out-of-order). |
| `prop_current_is_non_decreasing` | Chained invalidations never decrease `current`. |

### 7.2 Deterministic edge-case tests (always run)

| Test name | Specific case |
|-----------|---------------|
| `nonce_replay_invalidation_by_one` | Span = 1, start = 0 |
| `nonce_replay_invalidation_by_max` | Span = 10 000, iterates all 10 000 invalid nonces |
| `nonce_replay_invalidation_by_max_plus_one_rejected` | Span = 10 001, must be `SpanTooLarge(10_001)` |
| `nonce_replay_boundary_last_invalidated` | start = 500, span = MAX−1; checks `new_nonce − 1` fails and `new_nonce` succeeds |
| `nonce_replay_persists_across_further_invalidations` | Two chained invalidations; all prior nonces remain invalid |
| `nonce_replay_no_decrease` | Confirms `NonceMustIncrease` when attempting to lower `current` |

---

## 8. Formal Summary

Given a `NonceStore` with `current = N` (reached by any sequence of valid operations):

```
∀ k ∈ ℕ₀ where k < N:  consume(k) = Err(InvalidNonce)
```

This holds because:
1. `consume` requires exact equality `k == current`.
2. `current` is monotonically non-decreasing (`current' ≥ current` after every operation).
3. `k < N ≤ current'` ⟹ `k ≠ current'` ⟹ `Err(InvalidNonce)`.

No additional mechanism (timeout, expiry, external oracle) is needed. The monotonicity of a `u64` counter under the two defined operations provides the replay protection unconditionally.
