//! Pure-Rust pagination utilities for QuickLendX query endpoints.
//!
//! This module provides overflow-safe helpers used by every paginated query in
//! the contract. It is intentionally decoupled from Soroban-specific types so
//! that pagination semantics can be unit-tested without touching storage or
//! any contract state.
//!
//! # Invariants
//!
//! 1. **Hard cap** - No call returns more than [`MAX_QUERY_LIMIT`] items, even
//!    if the caller asks for more.
//! 2. **Empty on overflow** - If `offset >= total_count`, every helper returns
//!    the empty/zero-length result. Callers never see a panic or wrap-around.
//! 3. **Stable ordering** - The input slice order is preserved; pagination
//!    never reorders, deduplicates, or skips elements within the current page.
//! 4. **No unbounded loops** - Every iteration count is bounded by
//!    `min(limit, MAX_QUERY_LIMIT, remaining)`.
//! 5. **No panics** - Only `saturating_*` arithmetic is used and all indexing
//!    goes through pre-computed safe bounds.

use crate::errors::QuickLendXError;
use alloc::vec::Vec;

/// Maximum number of records returned by paginated query endpoints.
pub const MAX_QUERY_LIMIT: u32 = 50;

/// Clamp a caller-supplied `limit` to [`MAX_QUERY_LIMIT`].
///
/// # Arguments
/// * `limit` - Raw limit value from the caller. May be `0` or larger than
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

/// Cursor with snapshot generation metadata for paged reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageCursor {
    /// 0-based offset or record index.
    pub offset: u32,
    /// Snapshot generation timestamp/sequence tag for consistency validation.
    pub generation: u64,
}

impl PageCursor {
    /// Construct a new pagination cursor with the given offset and snapshot generation.
    #[inline]
    pub const fn new(offset: u32, generation: u64) -> Self {
        Self { offset, generation }
    }

    /// Validate that this cursor's snapshot generation matches the current snapshot generation.
    ///
    /// # Returns
    /// * `Ok(())` if snapshot generations match.
    /// * `Err(QuickLendXError::UnstableCursor)` if the cursor belongs to a different snapshot generation.
    #[inline]
    pub const fn require_stable(&self, current_generation: u64) -> Result<(), QuickLendXError> {
        require_stable_cursor(self.generation, current_generation)
    }
}

/// Validate that a pagination cursor's snapshot generation matches the active snapshot generation.
///
/// Refuses cursors generated against an older or newer snapshot generation to prevent silent data
/// omissions, duplicates, or inconsistent reads across dynamic state mutations.
///
/// # Arguments
/// * `cursor_generation` - The snapshot generation tag encoded in or associated with the caller's cursor.
/// * `current_generation` - The active snapshot generation of the contract or queried dataset.
///
/// # Returns
/// * `Ok(())` if `cursor_generation == current_generation`.
/// * `Err(QuickLendXError::UnstableCursor)` if `cursor_generation != current_generation`.
#[inline]
pub const fn require_stable_cursor(
    cursor_generation: u64,
    current_generation: u64,
) -> Result<(), QuickLendXError> {
    if cursor_generation != current_generation {
        return Err(QuickLendXError::UnstableCursor);
    }
    Ok(())
}

/// Validate query parameters for security and resource protection.
///
/// # Arguments
/// * `offset` - Pagination offset starting index.
/// * `_limit` - Pagination limit.
///
/// # Returns
/// * `Ok(())` if valid.
/// * `Err(QuickLendXError::InvalidAmount)` on potential offset overflow.
#[inline]
pub const fn validate_query_params(offset: u32, _limit: u32) -> Result<(), QuickLendXError> {
    if offset > u32::MAX - MAX_QUERY_LIMIT {
        return Err(QuickLendXError::InvalidAmount);
    }
    Ok(())
}

/// Validate pagination parameters against a known collection size.
///
/// # Arguments
/// * `offset` - Caller-supplied starting position (0-based).
/// * `limit` - Caller-supplied max records; will be clamped.
/// * `total_count` - Size of the underlying result set after any filtering.
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
/// pair - including `u32::MAX` - without panicking.
///
/// # Arguments
/// * `offset` - Starting position.
/// * `limit` - Number of records requested.
/// * `collection_size` - Size of the collection being paginated.
#[inline]
pub const fn calculate_safe_bounds(offset: u32, limit: u32, collection_size: u32) -> (u32, u32) {
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
/// * Preserves ordering - no sorting, no deduplication.
pub fn paginate_slice<T: Clone>(items: &[T], offset: u32, limit: u32) -> Vec<T> {
    // Cast failure must surface as typed error, not panic.
    // Use try_from and fall back safely; callers in paged endpoints already
    // validate before reaching here. On failure we return empty (no panic).
    let collection_size = match u32::try_from(items.len()) {
        Ok(n) => n,
        Err(_) => return Vec::new(),
    };
    let (start, end) = calculate_safe_bounds(offset, limit, collection_size);
    if start >= end {
        return Vec::new();
    }
    items[(start as usize)..(end as usize)].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_require_stable_cursor_matching_generation() {
        assert_eq!(require_stable_cursor(100, 100), Ok(()));
        let cursor = PageCursor::new(10, 100);
        assert_eq!(cursor.require_stable(100), Ok(()));
    }

    #[test]
    fn test_require_stable_cursor_mismatched_generation() {
        assert_eq!(
            require_stable_cursor(99, 100),
            Err(QuickLendXError::UnstableCursor)
        );
        let cursor = PageCursor::new(10, 99);
        assert_eq!(
            cursor.require_stable(100),
            Err(QuickLendXError::UnstableCursor)
        );
    }
}
