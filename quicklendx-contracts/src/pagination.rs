//! Pure-Rust pagination utilities for QuickLendX query endpoints.
//!
//! This module provides overflow-safe helpers used by every paginated query in
//! the contract. It is intentionally decoupled from Soroban-specific types so
//! that pagination semantics can be unit-tested without touching storage or
//! any contract state.
//!
//! # Invariants
//!
//! 1. **Hard cap** — No call returns more than [`MAX_QUERY_LIMIT`] items, even
//!    if the caller asks for more.
//! 2. **Empty on overflow** — If `offset >= total_count`, every helper returns
//!    the empty/zero-length result. Callers never see a panic or wrap-around.
//! 3. **Stable ordering** — The input slice order is preserved; pagination
//!    never reorders, deduplicates, or skips elements within the current page.
//! 4. **No unbounded loops** — Every iteration count is bounded by
//!    `min(limit, MAX_QUERY_LIMIT, remaining)`.
//! 5. **No panics** — Only `saturating_*` arithmetic is used and all indexing
//!    goes through pre-computed safe bounds.

extern crate alloc;

use alloc::vec::Vec;

/// Maximum number of records any paginated query endpoint may return in a
/// single response.
///
/// Raising this constant requires a security review because it raises the
/// worst-case memory consumption and gas cost of every query endpoint.
pub const MAX_QUERY_LIMIT: u32 = 100;

/// Clamp a caller-supplied `limit` to [`MAX_QUERY_LIMIT`].
///
/// # Arguments
/// * `limit` — Raw limit value from the caller. May be `0` or larger than
///   [`MAX_QUERY_LIMIT`].
///
/// # Returns
/// A value in `0..=MAX_QUERY_LIMIT`.
#[inline]
pub const fn cap_query_limit(limit: u32) -> u32 {
    if limit > MAX_QUERY_LIMIT {
        MAX_QUERY_LIMIT
    } else {
        limit
    }
}

/// Validate pagination parameters against a known collection size.
///
/// # Arguments
/// * `offset` — Caller-supplied starting position (0-based).
/// * `limit` — Caller-supplied max records; will be clamped.
/// * `total_count` — Size of the underlying result set after any filtering.
///
/// # Returns
/// `(safe_offset, effective_limit, has_more)` where:
/// * `safe_offset` is clamped to `total_count` (never panics),
/// * `effective_limit` is clamped to both [`MAX_QUERY_LIMIT`] and the remaining
///   items, and
/// * `has_more` is `true` iff additional pages exist past this response.
#[inline]
pub const fn validate_pagination_params(
    offset: u32,
    limit: u32,
    total_count: u32,
) -> (u32, u32, bool) {
    let capped_limit = cap_query_limit(limit);
    let safe_offset = if offset > total_count {
        total_count
    } else {
        offset
    };
    let remaining = total_count.saturating_sub(safe_offset);
    let effective_limit = if capped_limit > remaining {
        remaining
    } else {
        capped_limit
    };
    let has_more = safe_offset.saturating_add(effective_limit) < total_count;
    (safe_offset, effective_limit, has_more)
}

/// Compute the `[start, end)` slice indices required to paginate a collection
/// of the given `collection_size`.
///
/// Guarantees `0 <= start <= end <= collection_size` for any `(offset, limit)`
/// pair — including `u32::MAX` — without panicking.
///
/// # Arguments
/// * `offset` — Starting position.
/// * `limit` — Number of records requested.
/// * `collection_size` — Size of the collection being paginated.
#[inline]
pub const fn calculate_safe_bounds(
    offset: u32,
    limit: u32,
    collection_size: u32,
) -> (u32, u32) {
    let capped_limit = cap_query_limit(limit);
    let start = if offset > collection_size {
        collection_size
    } else {
        offset
    };
    let end_raw = start.saturating_add(capped_limit);
    let end = if end_raw > collection_size {
        collection_size
    } else {
        end_raw
    };
    (start, end)
}

/// Paginate an arbitrary slice of cloneable values.
///
/// Returns a freshly allocated `Vec<T>` containing up to
/// `min(limit, MAX_QUERY_LIMIT)` items starting at `offset` from `items`,
/// preserving the input order.
///
/// # Security
/// * Never panics, even for `offset` or `limit` equal to `u32::MAX`.
/// * Enforces [`MAX_QUERY_LIMIT`] to bound allocation size.
/// * Preserves ordering — no sorting, no deduplication.
pub fn paginate_slice<T: Clone>(items: &[T], offset: u32, limit: u32) -> Vec<T> {
    let collection_size = u32::try_from(items.len()).unwrap_or(u32::MAX);
    let (start, end) = calculate_safe_bounds(offset, limit, collection_size);
    if start >= end {
        return Vec::new();
    }
    items[(start as usize)..(end as usize)].to_vec()
}
