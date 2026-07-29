# Requirements Document

## Introduction

The QuickLendX Soroban smart contract exposes a rich set of investor-centric query functions
(`get_bids_by_investor`, `get_investor_bids_paged`, `get_investor_investments_paged`,
`get_investor_verification`) but provides no symmetric counterparts for business operators.
Frontends and operators currently must iterate over all invoices owned by a business and
aggregate bids manually — a pattern that is expensive on-chain, error-prone, and inconsistent
with the investor experience.

This feature introduces three new query functions that mirror the investor-side API from a
business perspective:

| New function | Investor mirror |
|---|---|
| `get_bids_by_business(business, invoice_id)` | `get_bids_by_investor(invoice_id, investor)` |
| `get_bids_for_business_paged(business, status_filter, offset, limit)` | `get_investor_bids_paged(investor, status_filter, offset, limit)` |
| `get_business_bid_summary(business)` | _(no direct mirror — aggregate analytics)_ |

In addition, a new per-business bid index key `("bid_biz", business_address)` is introduced
so these functions operate in O(active_bids) rather than O(invoices × bids_per_invoice).

All existing public functions and their signatures remain unchanged. No unrelated changes are
in scope.

---

## Glossary

- **Contract**: The `quicklendx-contracts` Soroban smart contract deployed on the Stellar
  blockchain, running in a `#![no_std]` environment.
- **Business**: A verified `Address` that has submitted KYC and had at least one invoice
  stored in the Contract.
- **Investor**: A verified `Address` that places bids on invoices.
- **Invoice**: A funding request stored under a `BytesN<32>` identifier.
- **Bid**: A funding offer stored as a `Bid` struct identified by a `BytesN<32>` bid ID,
  linked to both an `invoice_id` and an `investor` address.
- **Bid_Business_Index**: The per-business bid index stored under the composite key
  `("bid_biz", business_address)` as a `Vec<BytesN<32>>` of bid IDs.
- **BidStatus**: The `BidStatus` enum with variants `Placed`, `Withdrawn`, `Accepted`,
  `Expired`, `Cancelled`.
- **BusinessBidSummary**: A new `contracttype` struct returned by
  `get_business_bid_summary`, containing aggregate statistics across all invoices owned by
  a given business.
- **MAX_QUERY_LIMIT**: The compile-time constant `100` (`pub(crate) const MAX_QUERY_LIMIT: u32 = 100`) that caps every paginated result set.
- **Offset**: A zero-based index into the filtered result set used for cursor-style pagination.
- **Limit**: The maximum number of records requested per page; always capped to
  `MAX_QUERY_LIMIT`.
- **Status_Filter**: An `Option<BidStatus>` passed to paged queries; `None` matches all
  statuses.
- **Env**: The Soroban `Env` runtime object, the sole source of ledger time, storage, and
  cryptographic operations in a `no_std` context.

---

## Requirements

### Requirement 1: Per-Business Bid Index Maintenance

**User Story:** As a smart contract maintainer, I want every bid write operation to update
a per-business bid index, so that business-centric queries can resolve bids in O(1) index
lookups without scanning all invoices.

#### Acceptance Criteria

1. THE Contract SHALL store a `Vec<BytesN<32>>` of bid IDs under the composite storage key
   `("bid_biz", business_address)` whenever a new bid is placed on an invoice owned by
   that business.
2. WHEN a bid is placed via `place_bid`, THE Contract SHALL append the new bid ID to the
   `Bid_Business_Index` for the invoice's owner before the function returns.
3. THE Contract SHALL NOT add a duplicate bid ID to the `Bid_Business_Index` when
   `place_bid` is called with an already-indexed bid ID.
4. THE Contract SHALL locate the new storage key constant for the business bid index in the
   same centralized location as existing index key constants (e.g., alongside
   `symbol_short!("bid_inv")`).
5. THE Contract SHALL retain all existing storage key constants and their associated
   behavior unchanged.

---

### Requirement 2: Cross-Invoice Bid Query by Business

**User Story:** As a business operator, I want to query all bids placed across every invoice
I own in a single call, so that I can monitor investor activity without iterating invoices
manually on the client side.

#### Acceptance Criteria

1. WHEN `get_bids_by_business(business)` is called with a valid business `Address`, THE
   Contract SHALL return a `Vec<Bid>` containing every `Bid` record whose `invoice_id`
   belongs to an invoice owned by `business`, across all invoices.
2. WHEN `get_bids_by_business(business)` is called and `business` owns zero invoices, THE
   Contract SHALL return an empty `Vec<Bid>`.
3. WHEN `get_bids_by_business(business)` is called and `business` owns invoices that have
   no bids, THE Contract SHALL return an empty `Vec<Bid>`.
4. THE Contract SHALL resolve `get_bids_by_business` using the `Bid_Business_Index`
   rather than iterating all invoices in storage.
5. THE Contract SHALL include bids of all `BidStatus` variants in the result of
   `get_bids_by_business` (no implicit status filtering).
6. IF the `Bid_Business_Index` for the requested `business` does not exist in storage,
   THEN THE Contract SHALL return an empty `Vec<Bid>` rather than trapping.

---

### Requirement 3: Paginated Business Bid Query

**User Story:** As a frontend developer, I want a paginated equivalent of
`get_investor_bids_paged` scoped to a business address, so that I can page through
potentially large result sets without loading all bids at once.

#### Acceptance Criteria

1. WHEN `get_bids_for_business_paged(business, status_filter, offset, limit)` is called,
   THE Contract SHALL return a `Vec<Bid>` containing at most `limit` bids, starting at
   position `offset` within the filtered set of bids owned by `business`.
2. THE Contract SHALL cap `limit` to `MAX_QUERY_LIMIT` (100) regardless of the value
   supplied by the caller.
3. WHEN `offset` equals or exceeds the total number of filtered bids for `business`, THE
   Contract SHALL return an empty `Vec<Bid>`.
4. WHEN `status_filter` is `None`, THE Contract SHALL include bids of all `BidStatus`
   variants in the page result.
5. WHEN `status_filter` is `Some(status)`, THE Contract SHALL include only bids whose
   `status` field equals `status`.
6. THE Contract SHALL apply pagination after status filtering, so that `offset` refers to
   a position within the already-filtered sequence.
7. IF `validate_query_params(offset, limit)` returns an error, THEN THE Contract SHALL
   return an empty `Vec<Bid>`.
8. THE Contract SHALL use the same overflow-safe index arithmetic (saturation on
   `start.saturating_add(capped_limit).min(len)`) as existing paged query functions.
9. THE Contract SHALL resolve bids using the `Bid_Business_Index` without performing a
   full invoice scan.

---

### Requirement 4: Business Bid Summary Aggregate

**User Story:** As a business operator or administrator, I want a single call that returns
aggregate bid statistics across all my invoices, so that I can assess investor engagement
and find the best available offer per invoice without multiple round-trips.

#### Acceptance Criteria

1. THE Contract SHALL expose a `BusinessBidSummary` struct declared with `#[contracttype]`
   containing at minimum:
   - `total_bids: u32` — count of all bids regardless of status,
   - `active_bids: u32` — count of bids with `BidStatus::Placed` that have not expired,
   - `best_bid: Option<Bid>` — the single bid with the highest profit margin
     (`expected_return - bid_amount`) across all active bids for the business, using the
     same comparison logic as `BidStorage::compare_bids`,
   - `invoice_count: u32` — number of distinct invoices with at least one bid.
2. WHEN `get_business_bid_summary(business)` is called with a valid business `Address`,
   THE Contract SHALL return a `BusinessBidSummary` populated with accurate counts and
   the best active bid across all invoices owned by `business`.
3. WHEN `get_business_bid_summary(business)` is called and `business` owns no invoices or
   no bids exist, THE Contract SHALL return a `BusinessBidSummary` with `total_bids = 0`,
   `active_bids = 0`, `best_bid = None`, and `invoice_count = 0`.
4. THE Contract SHALL determine `active_bids` by checking both `BidStatus::Placed` and
   `bid.expiration_timestamp > env.ledger().timestamp()` for each candidate bid.
5. THE Contract SHALL determine `best_bid` using the same multi-field ordering as
   `BidStorage::compare_bids`: profit → `expected_return` → `bid_amount` → `timestamp`
   → `bid_id` byte array as final tiebreaker.
6. THE Contract SHALL resolve `get_business_bid_summary` using the `Bid_Business_Index`
   rather than iterating all invoices.

---

### Requirement 5: Error Handling and Edge Cases

**User Story:** As a frontend developer, I want all business query functions to handle
degenerate inputs gracefully, so that my UI does not crash when a business has no
invoices, no bids, or when query parameters are out of range.

#### Acceptance Criteria

1. WHEN any business query function is called with an `Address` that has no entry in the
   `Bid_Business_Index`, THE Contract SHALL return an empty result rather than trapping
   or returning an error.
2. WHEN `get_bids_for_business_paged` is called with `limit = 0`, THE Contract SHALL
   return an empty `Vec<Bid>`.
3. WHEN `get_bids_for_business_paged` is called with `offset` greater than
   `u32::MAX - MAX_QUERY_LIMIT`, THE Contract SHALL return an empty `Vec<Bid>` (matching
   behavior of `validate_query_params`).
4. WHEN `get_bids_for_business_paged` is called with `limit` greater than
   `MAX_QUERY_LIMIT`, THE Contract SHALL silently cap the result to `MAX_QUERY_LIMIT`
   records and SHALL NOT return an error.
5. WHEN `get_business_bid_summary` is called for a `business` whose bids are all in
   terminal states (`Withdrawn`, `Accepted`, `Expired`, `Cancelled`), THE Contract SHALL
   return `active_bids = 0` and `best_bid = None` while reporting correct `total_bids`
   and `invoice_count`.
6. IF a bid ID stored in the `Bid_Business_Index` no longer resolves to a `Bid` record
   in storage, THEN THE Contract SHALL skip that entry silently rather than trapping.

---

### Requirement 6: Pagination Consistency with Existing Query Functions

**User Story:** As a frontend developer, I want the business-side paged query to behave
identically to `get_investor_bids_paged` in all edge cases, so that I can reuse the same
pagination logic on the client side.

#### Acceptance Criteria

1. THE Contract SHALL accept the same parameter types for `get_bids_for_business_paged`
   as `get_investor_bids_paged`: `(env: Env, business: Address, status_filter: Option<BidStatus>, offset: u32, limit: u32) -> Vec<Bid>`.
2. THE Contract SHALL produce an empty `Vec<Bid>` for `get_bids_for_business_paged` under
   every condition where `get_investor_bids_paged` would produce an empty `Vec<Bid>` for
   the same logical parameters.
3. THE Contract SHALL use `cap_query_limit(limit)` from `investment_queries::InvestmentQueries`
   (or the equivalent `lib.rs`-level wrapper) so the cap value stays in sync with the
   rest of the contract if `MAX_QUERY_LIMIT` is ever changed.
4. THE Contract SHALL apply pagination using overflow-safe arithmetic identical to all
   existing paged functions: `start = offset.min(len)`,
   `end = start.saturating_add(capped_limit).min(len)`.

---

### Requirement 7: Backward Compatibility

**User Story:** As an operator running a deployed instance of QuickLendX, I want the
introduction of these query functions to leave all existing public API signatures
unchanged, so that no on-chain callers or off-chain clients break.

#### Acceptance Criteria

1. THE Contract SHALL retain unchanged signatures for every existing public function,
   including `place_bid`, `get_bids_by_investor`, `get_investor_bids_paged`,
   `get_investor_investments_paged`, `get_investor_verification`,
   `get_business_invoices_paged`, and all other contractimpl methods.
2. THE Contract SHALL NOT alter, rename, or remove any existing storage key.
3. THE Contract SHALL NOT change the discriminant values of `BidStatus`, `QuickLendXError`,
   or any other `contracttype` enum.
4. THE Contract SHALL compile successfully for target `wasm32-unknown-unknown` with
   `cargo build --target wasm32-unknown-unknown --release`.
5. THE Contract SHALL pass `cargo test -p quicklendx-contracts` with no regressions.
6. THE Contract SHALL pass `cargo clippy --workspace --all-targets -- -D warnings` with
   no new warnings.

---

### Requirement 8: No `std` Constraint

**User Story:** As a smart contract developer deploying to Stellar, I want all new code to
comply with the `#![no_std]` constraint, so that the contract WASM binary remains deployable.

#### Acceptance Criteria

1. THE Contract SHALL implement all new functions using only `soroban_sdk` primitives
   (`Vec`, `Map`, `Address`, `BytesN`, `Env`, `Symbol`, `contracttype`) and core Rust
   with `no_std`.
2. THE Contract SHALL NOT introduce any `use std::` or `extern crate std` references in
   new or modified files.
3. THE Contract SHALL NOT use heap-allocated types outside those provided by
   `soroban_sdk`'s `alloc` feature (already enabled in `Cargo.toml`).

---

### Requirement 9: Test Coverage

**User Story:** As a developer maintaining the contract, I want deterministic Soroban mock
Env tests and property-based tests for the new query functions, so that correctness is
verified across a wide input space.

#### Acceptance Criteria

1. THE Test_Suite SHALL include unit tests for `get_bids_by_business` covering:
   - business with multiple invoices and multiple bids per invoice,
   - business with invoices but zero bids,
   - address with no invoices at all.
2. THE Test_Suite SHALL include unit tests for `get_bids_for_business_paged` covering:
   - first page, middle page, and last page of a multi-bid result set,
   - `limit` exceeding `MAX_QUERY_LIMIT`,
   - `offset` past end of result set,
   - `status_filter = None` and each `BidStatus` variant as a filter.
3. THE Test_Suite SHALL include unit tests for `get_business_bid_summary` covering:
   - all bids active,
   - mix of active and terminal bids,
   - all bids expired,
   - zero bids.
4. THE Test_Suite SHALL include a property-based test (using the `proptest` crate under the
   `fuzz-tests` feature) asserting: for any set of bids placed across any number of
   invoices for a business, the total count returned by `get_business_bid_summary`
   equals the sum of bids returned per-invoice by `get_bids_by_invoice` for all invoices
   of that business (count invariant).
5. THE Test_Suite SHALL include a property-based test asserting that for any valid
   `(offset, limit)` pair where `offset < total_bids`, the result of
   `get_bids_for_business_paged` is a contiguous sub-sequence of the result of
   `get_bids_by_business` at the correct position (pagination sub-sequence property).
6. THE Test_Suite SHALL include a property-based test asserting that calling
   `get_bids_for_business_paged` twice with the same parameters and the same contract
   state returns identical results (idempotence / determinism property).
7. THE Test_Suite SHALL use only `soroban_sdk::testutils` mock `Env` (no real network
   calls), ensuring all tests are deterministic.
8. THE Test_Suite SHALL add all new test modules under `#[cfg(test)]` in a dedicated file
   (e.g., `test_business_query.rs`) and register it in `lib.rs`.

---

### Requirement 10: Constants Centralization

**User Story:** As a developer reading the codebase, I want the new storage key constant
for the business bid index to live in the same file and section as existing index key
constants, so that adding future indexes follows a predictable pattern.

#### Acceptance Criteria

1. THE Contract SHALL declare the business bid index storage key constant using
   `symbol_short!("bid_biz")` (or equivalent short symbol within the 9-character Soroban
   limit) in `bid.rs` alongside the existing `symbol_short!("bid_inv")` constant.
2. THE Contract SHALL document the new constant with a comment that mirrors the style of
   existing constant documentation in `bid.rs`.
3. THE Contract SHALL NOT scatter the new constant across multiple files.

