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

// ============================================================================
// Soroban Transaction Footprint Limits
// ============================================================================

/// Soroban network per-transaction ledger read-entry ceiling (Protocol 22/23+).
pub const SOROBAN_MAX_READ_ENTRIES: u32 = 100;
/// Soroban network per-transaction ledger write-entry ceiling (Protocol 22/23+).
pub const SOROBAN_MAX_WRITE_ENTRIES: u32 = 50;
/// Soroban network per-transaction ledger read-bytes ceiling (200 KB).
pub const SOROBAN_MAX_READ_BYTES: u32 = 200_000;
/// Soroban network per-transaction ledger write-bytes ceiling (65 KiB).
pub const SOROBAN_MAX_WRITE_BYTES: u32 = 66_560;
/// Soroban network per-transaction CPU instruction ceiling (100M instructions).
pub const SOROBAN_MAX_INSTRUCTIONS: u64 = 100_000_000;
/// Soroban network per-transaction memory ceiling (40 MiB).
pub const SOROBAN_MAX_MEM_BYTES: u64 = 41_943_040;

/// Maximum allowed Soroban transaction footprint for a heavy contract entrypoint
/// executed at its maximum configured protocol bounds.
///
/// All values are enforced by `src/test_footprint_limits.rs` and documented in
/// `docs/CAPS.md` and `docs/gas-baselines.md`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FootprintLimit {
    /// Entrypoint or operation identifier.
    pub entrypoint: &'static str,
    /// Maximum ledger entries read (`cost_estimate().resources().read_entries`).
    pub max_read_entries: u32,
    /// Maximum ledger entries written (`cost_estimate().resources().write_entries`).
    pub max_write_entries: u32,
    /// Maximum bytes read from ledger storage (`cost_estimate().resources().disk_read_bytes`).
    pub max_read_bytes: u32,
    /// Maximum bytes written to ledger storage (`cost_estimate().resources().write_bytes`).
    pub max_write_bytes: u32,
    /// Maximum CPU instructions (`cost_estimate().resources().instructions`).
    pub max_instructions: u64,
    /// Maximum memory bytes (`cost_estimate().resources().mem_bytes`).
    pub max_mem_bytes: u64,
}

/// Footprint ceiling for `accept_bid` / `accept_bid_and_fund` at `MAX_BIDS_PER_INVOICE` (50).
pub const FOOTPRINT_ACCEPT_BID_AND_FUND: FootprintLimit = FootprintLimit {
    entrypoint: "accept_bid_and_fund",
    max_read_entries: SOROBAN_MAX_READ_ENTRIES,
    max_write_entries: SOROBAN_MAX_WRITE_ENTRIES,
    max_read_bytes: SOROBAN_MAX_READ_BYTES,
    max_write_bytes: SOROBAN_MAX_WRITE_BYTES,
    max_instructions: SOROBAN_MAX_INSTRUCTIONS,
    max_mem_bytes: SOROBAN_MAX_MEM_BYTES,
};

/// Footprint ceiling for `settle_invoice` on a funded invoice.
pub const FOOTPRINT_SETTLE_INVOICE: FootprintLimit = FootprintLimit {
    entrypoint: "settle_invoice",
    max_read_entries: SOROBAN_MAX_READ_ENTRIES,
    max_write_entries: SOROBAN_MAX_WRITE_ENTRIES,
    max_read_bytes: SOROBAN_MAX_READ_BYTES,
    max_write_bytes: SOROBAN_MAX_WRITE_BYTES,
    max_instructions: SOROBAN_MAX_INSTRUCTIONS,
    max_mem_bytes: SOROBAN_MAX_MEM_BYTES,
};

/// Footprint ceiling for `cleanup_expired_bids` at `MAX_BIDS_PER_INVOICE` (50) expired bids.
pub const FOOTPRINT_CLEANUP_EXPIRED_BIDS: FootprintLimit = FootprintLimit {
    entrypoint: "cleanup_expired_bids",
    max_read_entries: SOROBAN_MAX_READ_ENTRIES,
    max_write_entries: SOROBAN_MAX_WRITE_ENTRIES,
    max_read_bytes: SOROBAN_MAX_READ_BYTES,
    max_write_bytes: SOROBAN_MAX_WRITE_BYTES,
    max_instructions: SOROBAN_MAX_INSTRUCTIONS,
    max_mem_bytes: SOROBAN_MAX_MEM_BYTES,
};

/// Footprint ceiling for `process_partial_payment` at max configured payment history bounds.
pub const FOOTPRINT_PROCESS_PARTIAL_PAYMENT: FootprintLimit = FootprintLimit {
    entrypoint: "process_partial_payment",
    max_read_entries: SOROBAN_MAX_READ_ENTRIES,
    max_write_entries: SOROBAN_MAX_WRITE_ENTRIES,
    max_read_bytes: SOROBAN_MAX_READ_BYTES,
    max_write_bytes: SOROBAN_MAX_WRITE_BYTES,
    max_instructions: SOROBAN_MAX_INSTRUCTIONS,
    max_mem_bytes: SOROBAN_MAX_MEM_BYTES,
};

/// Footprint ceiling for paged queries (`get_payment_records`, `get_dispute_timeline`,
/// `get_bids_for_invoice`) at `MAX_QUERY_LIMIT` (50).
pub const FOOTPRINT_PAGED_QUERIES: FootprintLimit = FootprintLimit {
    entrypoint: "paged_queries",
    max_read_entries: SOROBAN_MAX_READ_ENTRIES,
    max_write_entries: SOROBAN_MAX_WRITE_ENTRIES,
    max_read_bytes: SOROBAN_MAX_READ_BYTES,
    max_write_bytes: SOROBAN_MAX_WRITE_BYTES,
    max_instructions: SOROBAN_MAX_INSTRUCTIONS,
    max_mem_bytes: SOROBAN_MAX_MEM_BYTES,
};

/// Single-source table of all enforced per-entrypoint footprint ceilings.
pub const FOOTPRINT_LIMITS: &[FootprintLimit] = &[
    FOOTPRINT_ACCEPT_BID_AND_FUND,
    FOOTPRINT_SETTLE_INVOICE,
    FOOTPRINT_CLEANUP_EXPIRED_BIDS,
    FOOTPRINT_PROCESS_PARTIAL_PAYMENT,
    FOOTPRINT_PAGED_QUERIES,
];

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

    #[test]
    fn test_footprint_limits_within_soroban_network_caps() {
        for limit in FOOTPRINT_LIMITS {
            assert!(
                limit.max_read_entries <= SOROBAN_MAX_READ_ENTRIES,
                "{}: max_read_entries {} exceeds Soroban cap {}",
                limit.entrypoint,
                limit.max_read_entries,
                SOROBAN_MAX_READ_ENTRIES
            );
            assert!(
                limit.max_write_entries <= SOROBAN_MAX_WRITE_ENTRIES,
                "{}: max_write_entries {} exceeds Soroban cap {}",
                limit.entrypoint,
                limit.max_write_entries,
                SOROBAN_MAX_WRITE_ENTRIES
            );
            assert!(
                limit.max_read_bytes <= SOROBAN_MAX_READ_BYTES,
                "{}: max_read_bytes {} exceeds Soroban cap {}",
                limit.entrypoint,
                limit.max_read_bytes,
                SOROBAN_MAX_READ_BYTES
            );
            assert!(
                limit.max_write_bytes <= SOROBAN_MAX_WRITE_BYTES,
                "{}: max_write_bytes {} exceeds Soroban cap {}",
                limit.entrypoint,
                limit.max_write_bytes,
                SOROBAN_MAX_WRITE_BYTES
            );
            assert!(
                limit.max_instructions <= SOROBAN_MAX_INSTRUCTIONS,
                "{}: max_instructions {} exceeds Soroban cap {}",
                limit.entrypoint,
                limit.max_instructions,
                SOROBAN_MAX_INSTRUCTIONS
            );
            assert!(
                limit.max_mem_bytes <= SOROBAN_MAX_MEM_BYTES,
                "{}: max_mem_bytes {} exceeds Soroban cap {}",
                limit.entrypoint,
                limit.max_mem_bytes,
                SOROBAN_MAX_MEM_BYTES
            );
        }
    }
}
