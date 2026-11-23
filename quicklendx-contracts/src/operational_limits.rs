//! Consolidated operational ceilings for clients, indexers, and integrators.
//!
//! [`OperationalLimits`] composes the protocol's batch-scan cap, query-page
//! cap, and fee cap into a single read via [`get_operational_limits`]. Without
//! it, a caller has to know which module owns each constant (and, for the fee
//! cap, there was previously no getter at all) and probe each one separately
//! or via trial and error.

use crate::defaults;
use soroban_sdk::contracttype;

/// Single-read snapshot of protocol-wide operational ceilings.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationalLimits {
    /// Hard cap on funded-invoice batch size per overdue-scan call.
    /// See [`defaults::max_overdue_scan_batch_limit`].
    pub max_batch: u32,
    /// Hard cap on items returned per paginated query call.
    /// See [`crate::MAX_QUERY_LIMIT`].
    pub max_limit: u32,
    /// Hard cap on protocol fees, in basis points (1000 = 10%).
    /// See `init::MAX_FEE_BPS`.
    pub max_fee: u32,
}

/// Build a fresh [`OperationalLimits`] snapshot from existing protocol constants.
pub fn get_operational_limits() -> OperationalLimits {
    OperationalLimits {
        max_batch: defaults::max_overdue_scan_batch_limit(),
        max_limit: crate::MAX_QUERY_LIMIT,
        max_fee: crate::init::MAX_FEE_BPS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operational_limits_matches_source_constants() {
        let limits = get_operational_limits();

        assert_eq!(limits.max_batch, defaults::max_overdue_scan_batch_limit());
        assert_eq!(limits.max_limit, crate::MAX_QUERY_LIMIT);
        assert_eq!(limits.max_fee, crate::init::MAX_FEE_BPS);
    }

    #[test]
    fn test_operational_limits_is_deterministic() {
        assert_eq!(get_operational_limits(), get_operational_limits());
    }
}
