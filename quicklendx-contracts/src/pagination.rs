//! Pagination utilities for QuickLendX query endpoints.
//!
//! This module provides overflow-safe helpers and the shared opaque cursor
//! representation used by paged contract queries.
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
use alloc::{string::String, vec::Vec};

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

/// Versioned opaque cursor for paged reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cursor {
    /// 0-based offset or record index.
    pub offset: u32,
    /// Collection generation used to reject stale pages.
    pub generation: u64,
}

impl Cursor {
    const VERSION: u8 = 1;
    const PAYLOAD_LEN: usize = 13;
    pub const ENCODED_LEN: usize = 24;
    const BASE64: &'static [u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    /// Construct a new pagination cursor with the given offset and snapshot generation.
    #[inline]
    pub const fn new(offset: u32, generation: u64) -> Self {
        Self { offset, generation }
    }

    /// Encode the version, generation, offset, and checksum as URL-safe base64.
    pub fn encode(&self) -> String {
        let mut payload = [0u8; 17];
        payload[0] = Self::VERSION;
        payload[1..9].copy_from_slice(&self.generation.to_be_bytes());
        payload[9..13].copy_from_slice(&self.offset.to_be_bytes());
        let checksum = Self::cursor_checksum(&payload[..Self::PAYLOAD_LEN]);
        payload[13..17].copy_from_slice(&checksum.to_be_bytes());

        let mut encoded = String::new();
        let mut index = 0;
        while index + 3 <= payload.len() {
            let a = payload[index] as usize;
            let b = payload[index + 1] as usize;
            let c = payload[index + 2] as usize;
            encoded.push(Self::BASE64[a >> 2] as char);
            encoded.push(Self::BASE64[((a & 0x03) << 4) | (b >> 4)] as char);
            encoded.push(Self::BASE64[((b & 0x0f) << 2) | (c >> 6)] as char);
            encoded.push(Self::BASE64[c & 0x3f] as char);
            index += 3;
        }

        let a = payload[index] as usize;
        let b = payload[index + 1] as usize;
        encoded.push(Self::BASE64[a >> 2] as char);
        encoded.push(Self::BASE64[((a & 0x03) << 4) | (b >> 4)] as char);
        encoded.push(Self::BASE64[(b & 0x0f) << 2] as char);
        encoded.push('=');
        encoded
    }

    /// Decode a cursor produced by [`Cursor::encode`].
    #[inline]
    pub fn decode(raw: &str) -> Result<Self, QuickLendXError> {
        let encoded = raw.as_bytes();
        if encoded.len() != Self::ENCODED_LEN || encoded[Self::ENCODED_LEN - 1] != b'=' {
            return Err(QuickLendXError::InvalidCursor);
        }

        let mut payload = [0u8; 17];
        let mut input_index = 0;
        let mut output_index = 0;
        while input_index + 4 < encoded.len() {
            let a = Self::base64_value(encoded[input_index])? as u32;
            let b = Self::base64_value(encoded[input_index + 1])? as u32;
            let c = Self::base64_value(encoded[input_index + 2])? as u32;
            let d = Self::base64_value(encoded[input_index + 3])? as u32;
            payload[output_index] = ((a << 2) | (b >> 4)) as u8;
            payload[output_index + 1] = ((b << 4) | (c >> 2)) as u8;
            payload[output_index + 2] = ((c << 6) | d) as u8;
            input_index += 4;
            output_index += 3;
        }

        let a = Self::base64_value(encoded[input_index])? as u32;
        let b = Self::base64_value(encoded[input_index + 1])? as u32;
        let c = Self::base64_value(encoded[input_index + 2])? as u32;
        if c & 0x03 != 0 {
            return Err(QuickLendXError::InvalidCursor);
        }
        payload[output_index] = ((a << 2) | (b >> 4)) as u8;
        payload[output_index + 1] = ((b << 4) | (c >> 2)) as u8;

        if payload[0] != Self::VERSION {
            return Err(QuickLendXError::InvalidCursor);
        }
        let expected_checksum =
            u32::from_be_bytes([payload[13], payload[14], payload[15], payload[16]]);
        if Self::cursor_checksum(&payload[..Self::PAYLOAD_LEN]) != expected_checksum {
            return Err(QuickLendXError::InvalidCursor);
        }

        let generation = u64::from_be_bytes(
            payload[1..9]
                .try_into()
                .map_err(|_| QuickLendXError::InvalidCursor)?,
        );
        let offset = u32::from_be_bytes(
            payload[9..13]
                .try_into()
                .map_err(|_| QuickLendXError::InvalidCursor)?,
        );
        Ok(Self::new(offset, generation))
    }

    fn decode_soroban(raw: &soroban_sdk::String) -> Result<Cursor, QuickLendXError> {
        if raw.len() != Cursor::ENCODED_LEN as u32 {
            return Err(QuickLendXError::InvalidCursor);
        }
        let bytes = raw.to_bytes();
        let mut encoded = [0u8; Cursor::ENCODED_LEN];
        bytes.copy_into_slice(&mut encoded);
        let encoded = core::str::from_utf8(&encoded).map_err(|_| QuickLendXError::InvalidCursor)?;
        Cursor::decode(encoded)
    }

    fn encode_soroban(env: &soroban_sdk::Env, cursor: Cursor) -> soroban_sdk::String {
        soroban_sdk::String::from_str(env, &cursor.encode())
    }

    fn base64_value(byte: u8) -> Result<u8, QuickLendXError> {
        Cursor::BASE64
            .iter()
            .position(|candidate| *candidate == byte)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or(QuickLendXError::InvalidCursor)
    }

    fn cursor_checksum(bytes: &[u8]) -> u32 {
        let mut checksum = u32::MAX;
        for byte in bytes {
            checksum ^= *byte as u32;
            for _ in 0..8 {
                checksum = if checksum & 1 == 1 {
                    (checksum >> 1) ^ 0xedb88320
                } else {
                    checksum >> 1
                };
            }
        }
        !checksum
    }

    /// Calculate the next page cursor given limit and total record count.
    /// Returns `None` if end-of-stream has been reached (`offset + limit >= total_count`).
    pub fn next_cursor(&self, limit: u32, total_count: u32) -> Option<Self> {
        let capped = cap_query_limit(limit);
        let next_offset = self.offset.saturating_add(capped);
        if capped > 0 && next_offset < total_count {
            Some(Self::new(next_offset, self.generation))
        } else {
            None
        }
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
    let has_more = if effective_limit == 0 {
        false
    } else {
        safe_offset.saturating_add(effective_limit) < total_count
    };
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

/// Compute pagination metadata (total count, has_more) for a filtered collection.
///
/// Returns `(total_count, has_more)` where:
/// * `total_count` is the size of the filtered set **before** pagination.
/// * `has_more` is `true` iff additional pages exist past `offset + limit`.
///
/// This helper is intended for paginated query endpoints that construct their
/// own result vecs (e.g. using Soroban `Vec`) and need to attach metadata.
#[inline]
pub fn pagination_metadata(offset: u32, limit: u32, total_count: u32) -> (u32, bool) {
    let capped_limit = cap_query_limit(limit);
    let has_more = capped_limit > 0 && offset.saturating_add(capped_limit) < total_count;
    (total_count, has_more)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_require_stable_cursor_matching_generation() {
        assert_eq!(require_stable_cursor(100, 100), Ok(()));
        let cursor = Cursor::new(10, 100);
        assert_eq!(cursor.require_stable(100), Ok(()));
    }

    #[test]
    fn test_require_stable_cursor_mismatched_generation() {
        assert_eq!(
            require_stable_cursor(99, 100),
            Err(QuickLendXError::UnstableCursor)
        );
        let cursor = Cursor::new(10, 99);
        assert_eq!(
            cursor.require_stable(100),
            Err(QuickLendXError::UnstableCursor)
        );
    }

    #[test]
    fn test_page_cursor_round_trip_and_invalid_input() {
        let original = Cursor::new(7, 42);
        let encoded = original.encode();
        assert_eq!(encoded.len(), Cursor::ENCODED_LEN);
        assert_eq!(Cursor::decode(&encoded), Ok(original));

        assert_eq!(Cursor::decode("42"), Err(QuickLendXError::InvalidCursor));
        assert_eq!(Cursor::decode("abc_7"), Err(QuickLendXError::InvalidCursor));
        assert_eq!(
            Cursor::decode("42_abc"),
            Err(QuickLendXError::InvalidCursor)
        );
        assert_eq!(
            Cursor::decode("42_7_9"),
            Err(QuickLendXError::InvalidCursor)
        );
    }

    #[test]
    fn test_cursor_rejects_tampering_and_unknown_versions() {
        let encoded = Cursor::new(7, 42).encode();
        let mut bytes = encoded.into_bytes();
        bytes[2] = if bytes[2] == b'A' { b'B' } else { b'A' };
        let tampered = core::str::from_utf8(&bytes).unwrap();
        assert_eq!(
            Cursor::decode(tampered),
            Err(QuickLendXError::InvalidCursor)
        );

        let mut payload = [0u8; 17];
        payload[0] = 2;
        payload[1..9].copy_from_slice(&42u64.to_be_bytes());
        payload[9..13].copy_from_slice(&7u32.to_be_bytes());
        let checksum = Cursor::cursor_checksum(&payload[..Cursor::PAYLOAD_LEN]);
        payload[13..17].copy_from_slice(&checksum.to_be_bytes());
        let unknown_version = encode_test_payload(&payload);
        assert_eq!(
            Cursor::decode(&unknown_version),
            Err(QuickLendXError::InvalidCursor)
        );
    }

    fn encode_test_payload(payload: &[u8; 17]) -> String {
        let mut encoded = String::new();
        for chunk in payload.chunks_exact(3) {
            let a = chunk[0] as usize;
            let b = chunk[1] as usize;
            let c = chunk[2] as usize;
            encoded.push(Cursor::BASE64[a >> 2] as char);
            encoded.push(Cursor::BASE64[((a & 3) << 4) | (b >> 4)] as char);
            encoded.push(Cursor::BASE64[((b & 15) << 2) | (c >> 6)] as char);
            encoded.push(Cursor::BASE64[c & 63] as char);
        }
        let a = payload[15] as usize;
        let b = payload[16] as usize;
        encoded.push(Cursor::BASE64[a >> 2] as char);
        encoded.push(Cursor::BASE64[((a & 3) << 4) | (b >> 4)] as char);
        encoded.push(Cursor::BASE64[(b & 15) << 2] as char);
        encoded.push('=');
        encoded
    }

    #[test]
    fn test_page_cursor_next_cursor() {
        let cursor = Cursor::new(0, 100);
        let next = cursor.next_cursor(10, 25);
        assert_eq!(next, Some(Cursor::new(10, 100)));

        let next2 = next.unwrap().next_cursor(10, 25);
        assert_eq!(next2, Some(Cursor::new(20, 100)));

        let next3 = next2.unwrap().next_cursor(10, 25);
        assert_eq!(next3, None); // 20 + 10 = 30 >= 25 -> end of stream
        assert_eq!(cursor.next_cursor(0, 25), None);
        assert_eq!(pagination_metadata(0, 0, 25), (25, false));
    }
}

/// Convert an opaque Soroban string to the shared cursor representation.
pub fn decode_soroban_cursor(raw: &soroban_sdk::String) -> Result<Cursor, QuickLendXError> {
    Cursor::decode_soroban(raw)
}

/// Convert the shared cursor representation to an opaque Soroban string.
pub fn encode_soroban_cursor(env: &soroban_sdk::Env, cursor: Cursor) -> soroban_sdk::String {
    Cursor::encode_soroban(env, cursor)
}
