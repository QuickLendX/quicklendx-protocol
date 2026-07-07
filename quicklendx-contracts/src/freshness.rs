use alloc::{format, string::String as RustString};
use soroban_sdk::{contracterror, contracttype, Env, String};

/// Default client-facing drift bound for freshness reads.
///
/// Freshness metadata is considered usable while `index_lag_seconds <= 120`.
/// Clients should treat `index_lag_seconds > 120` as stale data.
pub const DEFAULT_MAX_FRESHNESS_DRIFT_SECS: i64 = 120;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum DataKey {
    Admin,
    MaxFreshnessDriftSecs,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FreshnessError {
    NotAuthorized = 1,
    StaleDataRejected = 2,
    InvalidConfigValue = 3,
}

/// Client-facing freshness severity for indexed data.
///
/// The tier is computed from `FreshnessMetadata::index_lag_seconds` and two
/// inclusive thresholds:
///
/// ```rust,ignore
/// let tier = metadata.classify(30, 120);
/// assert_eq!(tier, FreshnessTier::Fresh);
/// ```
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreshnessTier {
    /// Drift is less than or equal to the configured fresh threshold.
    Fresh,
    /// Drift is above the fresh threshold but still within the stale threshold.
    Stale,
    /// Drift is above the stale threshold, or thresholds are invalid.
    Critical,
}

/// Transport-friendly freshness metadata returned by `lib.rs::get_freshness`.
///
/// See `quicklendx-contracts/docs/freshness.md` for the documented stale-data
/// boundary and recommended client behavior.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FreshnessMetadata {
    pub last_indexed_ledger: u32,
    pub index_lag_seconds: i64,
    pub last_updated_at: String,
    pub cursor: String,
}

impl FreshnessMetadata {
    pub fn from_env(
        env: &Env,
        indexed_ledger_seq: u32,
        indexed_ledger_timestamp: u64,
        offset: u32,
    ) -> Self {
        let current_timestamp = env.ledger().timestamp();
        let lag = if current_timestamp >= indexed_ledger_timestamp {
            current_timestamp
                .saturating_sub(indexed_ledger_timestamp)
                .min(i64::MAX as u64) as i64
        } else {
            -((indexed_ledger_timestamp - current_timestamp).min(i64::MAX as u64) as i64)
        };

        Self {
            last_indexed_ledger: indexed_ledger_seq,
            index_lag_seconds: lag,
            last_updated_at: iso8601_from_unix_timestamp(env, indexed_ledger_timestamp),
            cursor: String::from_str(env, &format!("{indexed_ledger_seq}_{offset}")),
        }
    }

    /// Classify this metadata's lag into a [`FreshnessTier`].
    ///
    /// `fresh_max` and `stale_max` are inclusive bounds in seconds:
    /// - `index_lag_seconds <= fresh_max` returns [`FreshnessTier::Fresh`]
    /// - `index_lag_seconds <= stale_max` returns [`FreshnessTier::Stale`]
    /// - larger drift returns [`FreshnessTier::Critical`]
    ///
    /// If `fresh_max > stale_max`, this method fails closed and returns
    /// [`FreshnessTier::Critical`]. Call [`Self::try_classify`] when the caller
    /// needs a typed rejection instead.
    pub fn classify(&self, fresh_max: i64, stale_max: i64) -> FreshnessTier {
        self.try_classify(fresh_max, stale_max)
            .unwrap_or(FreshnessTier::Critical)
    }

    /// Classify this metadata's lag, rejecting invalid threshold ordering.
    ///
    /// Returns [`FreshnessError::InvalidConfigValue`] when `fresh_max` is greater
    /// than `stale_max`.
    pub fn try_classify(
        &self,
        fresh_max: i64,
        stale_max: i64,
    ) -> Result<FreshnessTier, FreshnessError> {
        if fresh_max > stale_max {
            return Err(FreshnessError::InvalidConfigValue);
        }

        if self.index_lag_seconds <= fresh_max {
            Ok(FreshnessTier::Fresh)
        } else if self.index_lag_seconds <= stale_max {
            Ok(FreshnessTier::Stale)
        } else {
            Ok(FreshnessTier::Critical)
        }
    }
}

fn iso8601_from_unix_timestamp(env: &Env, timestamp: u64) -> String {
    let days = (timestamp / 86_400) as i64;
    let secs_of_day = (timestamp % 86_400) as u32;

    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;

    let rendered: RustString =
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z");
    String::from_str(env, &rendered)
}

// Howard Hinnant's civil-from-days algorithm adapted for Unix epoch days.
fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };

    (year as i32, m as u32, d as u32)
}
