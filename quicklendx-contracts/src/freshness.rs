//! Data freshness metadata for QuickLendX API responses.
//!
//! Provides [`FreshnessMetadata`] and [`FreshResponse<T>`] so clients can
//! determine whether the data they receive is near-real-time or lagging.
//!
//! # Fields
//!
//! | Field | Type | Description |
//! |---|---|---|
//! | `last_indexed_ledger` | `u32` | Last ledger sequence processed |
//! | `index_lag_seconds` | `i64` | Seconds between indexed ledger close and now (0 when current) |
//! | `last_updated_at` | `String` | ISO 8601 UTC timestamp of the indexed ledger close |
//! | `cursor` | `String` | Opaque pagination cursor: `"<ledger_seq>_<offset>"` |
//!
//! # Security
//!
//! Only public ledger data is exposed. No node addresses, validator identities,
//! or network topology are included.
//!
//! # UI-Safe Semantics
//!
//! Clients SHOULD warn users when `index_lag_seconds > 30`.

use soroban_sdk::{Env, String};

/// Freshness metadata attached to every API response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FreshnessMetadata {
    /// Last ledger sequence number processed by the indexer.
    pub last_indexed_ledger: u32,
    /// Seconds between the indexed ledger's close time and the current ledger's
    /// close time. Zero when the indexed ledger is the current ledger.
    pub index_lag_seconds: i64,
    /// ISO 8601 UTC timestamp of the indexed ledger close time.
    /// Format: `"YYYY-MM-DDTHH:MM:SSZ"` (derived from Unix epoch seconds).
    pub last_updated_at: String,
    /// Opaque pagination/replay cursor encoded as `"<ledger_seq>_<offset>"`.
    pub cursor: String,
}

impl FreshnessMetadata {
    /// Build [`FreshnessMetadata`] from the current ledger environment.
    ///
    /// `indexed_ledger_seq` is the last ledger sequence the indexer has
    /// processed. `indexed_ledger_timestamp` is its close time (Unix seconds).
    /// `offset` is the pagination offset to embed in the cursor.
    ///
    /// `index_lag_seconds` is computed as:
    /// `current_timestamp - indexed_ledger_timestamp`
    /// When `indexed_ledger_seq >= current_seq` the lag is 0.
    pub fn from_env(
        env: &Env,
        indexed_ledger_seq: u32,
        indexed_ledger_timestamp: u64,
        offset: u32,
    ) -> Self {
        let current_timestamp = env.ledger().timestamp();
        let current_seq = env.ledger().sequence();

        let index_lag_seconds: i64 = if indexed_ledger_seq >= current_seq {
            0
        } else {
            (current_timestamp as i64).saturating_sub(indexed_ledger_timestamp as i64)
        };

        let last_updated_at = unix_to_iso8601(env, indexed_ledger_timestamp);
        let cursor = build_cursor(env, indexed_ledger_seq, offset);

        FreshnessMetadata {
            last_indexed_ledger: indexed_ledger_seq,
            index_lag_seconds,
            last_updated_at,
            cursor,
        }
    }
}

/// Generic response wrapper pairing any payload `T` with [`FreshnessMetadata`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FreshResponse<T> {
    pub data: T,
    pub freshness: FreshnessMetadata,
}

impl<T> FreshResponse<T> {
    pub fn new(data: T, freshness: FreshnessMetadata) -> Self {
        FreshResponse { data, freshness }
    }
}

// ── Cursor helpers ────────────────────────────────────────────────────────────

/// Encode a cursor as `"<ledger_seq>_<offset>"`.
///
/// Uses a fixed-size stack buffer — no heap allocation.
pub fn build_cursor(env: &Env, ledger_seq: u32, offset: u32) -> String {
    // Max: "4294967295_4294967295" = 21 bytes
    let mut buf = [0u8; 22];
    let len = write_cursor_bytes(&mut buf, ledger_seq, offset);
    let s = core::str::from_utf8(&buf[..len]).unwrap_or("0_0");
    String::from_str(env, s)
}

/// Decode a cursor produced by [`build_cursor`].
///
/// Returns `(ledger_seq, offset)` or `None` if the cursor is malformed.
/// Uses `copy_into_slice` to read the soroban String bytes without `std`.
pub fn parse_cursor(env: &Env, cursor: &String) -> Option<(u32, u32)> {
    let len = cursor.len() as usize;
    if len == 0 || len > 21 {
        return None;
    }
    let mut buf = [0u8; 21];
    cursor.copy_into_slice(&mut buf[..len]);
    let s = core::str::from_utf8(&buf[..len]).ok()?;
    let mut parts = s.splitn(2, '_');
    let seq: u32 = parts.next()?.parse().ok()?;
    let off: u32 = parts.next()?.parse().ok()?;
    let _ = env; // env parameter kept for API consistency
    Some((seq, off))
}

// ── ISO 8601 helper ───────────────────────────────────────────────────────────

/// Convert a Unix timestamp (seconds) to an ISO 8601 UTC string.
///
/// Produces `"YYYY-MM-DDTHH:MM:SSZ"` using pure integer arithmetic.
pub fn unix_to_iso8601(env: &Env, unix_secs: u64) -> String {
    let (year, month, day, hour, min, sec) = unix_to_parts(unix_secs);

    let mut buf = [0u8; 20];
    write_u32_padded(&mut buf[0..4], year, 4);
    buf[4] = b'-';
    write_u32_padded(&mut buf[5..7], month, 2);
    buf[7] = b'-';
    write_u32_padded(&mut buf[8..10], day, 2);
    buf[10] = b'T';
    write_u32_padded(&mut buf[11..13], hour, 2);
    buf[13] = b':';
    write_u32_padded(&mut buf[14..16], min, 2);
    buf[16] = b':';
    write_u32_padded(&mut buf[17..19], sec, 2);
    buf[19] = b'Z';

    let s = core::str::from_utf8(&buf).unwrap_or("1970-01-01T00:00:00Z");
    String::from_str(env, s)
}

// ── Internal arithmetic ───────────────────────────────────────────────────────

/// Decompose a Unix timestamp into (year, month, day, hour, min, sec).
fn unix_to_parts(unix_secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let sec = (unix_secs % 60) as u32;
    let mins = unix_secs / 60;
    let min = (mins % 60) as u32;
    let hours = mins / 60;
    let hour = (hours % 24) as u32;
    let mut days = (hours / 24) as u32;

    let mut year = 1970u32;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let month_days: [u32; 12] = [
        31,
        if is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u32;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    let day = days + 1;

    (year, month, day, hour, min, sec)
}

#[inline]
fn is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Write `value` as zero-padded decimal into `buf` (big-endian ASCII).
fn write_u32_padded(buf: &mut [u8], mut value: u32, width: usize) {
    for i in (0..width).rev() {
        buf[i] = b'0' + (value % 10) as u8;
        value /= 10;
    }
}

/// Write `"<seq>_<offset>"` into `buf`, return byte length written.
fn write_cursor_bytes(buf: &mut [u8; 22], seq: u32, offset: u32) -> usize {
    let mut tmp = [0u8; 10];
    let seq_len = u32_to_ascii(seq, &mut tmp);
    buf[..seq_len].copy_from_slice(&tmp[..seq_len]);
    buf[seq_len] = b'_';
    let off_len = u32_to_ascii(offset, &mut tmp);
    buf[seq_len + 1..seq_len + 1 + off_len].copy_from_slice(&tmp[..off_len]);
    seq_len + 1 + off_len
}

/// Write a `u32` as ASCII decimal into `buf`, return byte length.
fn u32_to_ascii(mut value: u32, buf: &mut [u8; 10]) -> usize {
    if value == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 10];
    let mut len = 0usize;
    while value > 0 {
        tmp[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
    }
    // Reverse into buf
    for i in 0..len {
        buf[i] = tmp[len - 1 - i];
    }
    len
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Ledger, Env};

    fn make_env() -> Env {
        let env = Env::default();
        env.ledger().set_sequence_number(1000);
        env.ledger().set_timestamp(1_700_000_000); // 2023-11-14T22:13:20Z
        env
    }

    // ── Schema stability ──────────────────────────────────────────────────────

    #[test]
    fn test_schema_all_fields_present() {
        let env = make_env();
        let meta = FreshnessMetadata::from_env(&env, 1000, 1_700_000_000, 0);

        // All four fields must be accessible and correctly typed.
        let _: u32 = meta.last_indexed_ledger;
        let _: i64 = meta.index_lag_seconds;
        let _: String = meta.last_updated_at.clone();
        let _: String = meta.cursor.clone();

        assert_eq!(meta.last_indexed_ledger, 1000);
        assert_eq!(meta.index_lag_seconds, 0);
        assert!(meta.last_updated_at.len() > 0);
        assert!(meta.cursor.len() > 0);
    }

    // ── Zero-lag ──────────────────────────────────────────────────────────────

    #[test]
    fn test_zero_lag_when_indexed_equals_current() {
        let env = make_env();
        let meta = FreshnessMetadata::from_env(&env, 1000, 1_700_000_000, 0);
        assert_eq!(meta.index_lag_seconds, 0);
    }

    #[test]
    fn test_zero_lag_when_indexed_ahead_of_current() {
        let env = make_env();
        // indexed_ledger_seq > current_seq must not panic and must return 0 lag
        let meta = FreshnessMetadata::from_env(&env, 1001, 1_700_000_000, 0);
        assert_eq!(meta.index_lag_seconds, 0);
    }

    // ── Lag simulation ────────────────────────────────────────────────────────

    #[test]
    fn test_positive_lag_when_indexed_behind_current() {
        let env = make_env();
        // Current timestamp = 1_700_000_000; indexed 60 s earlier.
        let indexed_ts = 1_700_000_000u64 - 60;
        let meta = FreshnessMetadata::from_env(&env, 999, indexed_ts, 0);
        assert!(meta.index_lag_seconds > 0);
        assert_eq!(meta.index_lag_seconds, 60);
    }

    #[test]
    fn test_lag_simulation_large_gap() {
        let env = make_env();
        let indexed_ts = 1_700_000_000u64 - 3600;
        let meta = FreshnessMetadata::from_env(&env, 500, indexed_ts, 0);
        assert_eq!(meta.index_lag_seconds, 3600);
    }

    // ── Cursor round-trip ─────────────────────────────────────────────────────

    #[test]
    fn test_cursor_round_trip() {
        let env = make_env();
        let cursor = build_cursor(&env, 42, 7);
        let (seq, off) = parse_cursor(&env, &cursor).expect("cursor must parse");
        assert_eq!(seq, 42);
        assert_eq!(off, 7);
    }

    #[test]
    fn test_cursor_round_trip_zero_offset() {
        let env = make_env();
        let cursor = build_cursor(&env, 1000, 0);
        let (seq, off) = parse_cursor(&env, &cursor).expect("cursor must parse");
        assert_eq!(seq, 1000);
        assert_eq!(off, 0);
    }

    #[test]
    fn test_cursor_round_trip_large_values() {
        let env = make_env();
        let cursor = build_cursor(&env, u32::MAX, u32::MAX);
        let (seq, off) = parse_cursor(&env, &cursor).expect("cursor must parse");
        assert_eq!(seq, u32::MAX);
        assert_eq!(off, u32::MAX);
    }

    #[test]
    fn test_cursor_malformed_returns_none() {
        let env = make_env();
        let bad = String::from_str(&env, "notacursor");
        assert!(parse_cursor(&env, &bad).is_none());
    }

    #[test]
    fn test_cursor_empty_returns_none() {
        let env = make_env();
        let bad = String::from_str(&env, "");
        assert!(parse_cursor(&env, &bad).is_none());
    }

    // ── ISO 8601 timestamp ────────────────────────────────────────────────────

    #[test]
    fn test_iso8601_epoch_zero() {
        let env = make_env();
        let ts = unix_to_iso8601(&env, 0);
        assert_eq!(ts, String::from_str(&env, "1970-01-01T00:00:00Z"));
    }

    #[test]
    fn test_iso8601_known_timestamp() {
        let env = make_env();
        // 2023-11-14T22:13:20Z = 1_700_000_000
        let ts = unix_to_iso8601(&env, 1_700_000_000);
        assert_eq!(ts, String::from_str(&env, "2023-11-14T22:13:20Z"));
    }

    #[test]
    fn test_iso8601_length_is_always_20() {
        let env = make_env();
        let ts = unix_to_iso8601(&env, 1_700_000_000);
        assert_eq!(ts.len(), 20);
    }

    // ── Security: no topology fields ─────────────────────────────────────────

    #[test]
    fn test_no_internal_topology_in_metadata() {
        let env = make_env();
        let meta = FreshnessMetadata::from_env(&env, 1000, 1_700_000_000, 0);

        // Read cursor bytes and check for forbidden keywords.
        let cursor_len = meta.cursor.len() as usize;
        let mut cursor_buf = [0u8; 22];
        meta.cursor.copy_into_slice(&mut cursor_buf[..cursor_len]);
        let cursor_str = core::str::from_utf8(&cursor_buf[..cursor_len]).unwrap_or("");

        let ts_len = meta.last_updated_at.len() as usize;
        let mut ts_buf = [0u8; 20];
        meta.last_updated_at.copy_into_slice(&mut ts_buf[..ts_len]);
        let ts_str = core::str::from_utf8(&ts_buf[..ts_len]).unwrap_or("");

        // Cursor must only contain digits and underscore.
        for b in cursor_str.bytes() {
            assert!(
                b.is_ascii_digit() || b == b'_',
                "cursor byte '{b}' is not a digit or underscore"
            );
        }

        // Timestamp must match ISO 8601 pattern (digits, dashes, colon, T, Z).
        for b in ts_str.bytes() {
            assert!(
                b.is_ascii_digit() || b == b'-' || b == b':' || b == b'T' || b == b'Z',
                "timestamp byte '{b}' is unexpected"
            );
        }

        // Only public ledger fields are present — no extra fields on the struct.
        assert!(meta.last_indexed_ledger == 1000);
        assert!(meta.index_lag_seconds >= 0);
    }

    // ── FreshResponse wrapper ─────────────────────────────────────────────────

    #[test]
    fn test_fresh_response_wraps_payload() {
        let env = make_env();
        let meta = FreshnessMetadata::from_env(&env, 1000, 1_700_000_000, 0);
        let resp: FreshResponse<u32> = FreshResponse::new(42u32, meta.clone());
        assert_eq!(resp.data, 42u32);
        assert_eq!(resp.freshness, meta);
    }

    #[test]
    fn test_fresh_response_clone_eq() {
        let env = make_env();
        let meta = FreshnessMetadata::from_env(&env, 1000, 1_700_000_000, 5);
        let resp: FreshResponse<u32> = FreshResponse::new(99u32, meta);
        let cloned = resp.clone();
        assert_eq!(resp, cloned);
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    #[test]
    fn test_u32_to_ascii_zero() {
        let mut buf = [0u8; 10];
        let len = u32_to_ascii(0, &mut buf);
        assert_eq!(len, 1);
        assert_eq!(buf[0], b'0');
    }

    #[test]
    fn test_u32_to_ascii_max() {
        let mut buf = [0u8; 10];
        let len = u32_to_ascii(u32::MAX, &mut buf);
        let s = core::str::from_utf8(&buf[..len]).unwrap();
        assert_eq!(s, "4294967295");
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap(2000));
        assert!(is_leap(2024));
        assert!(!is_leap(1900));
        assert!(!is_leap(2023));
    }
}
