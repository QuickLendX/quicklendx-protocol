//! Shared test utilities for contract regression coverage.
//!
//! These helpers are intentionally kept small and explicit because they guard
//! a dangerous boundary: a test fixture or shared test constant must never be
//! permitted to silently target mainnet.

/// Shared constant used by regression tests to ensure a fixture cannot be
/// accidentally treated as a production-safe default.
pub const NO_ACCIDENTAL_MAINNET: &str = "TEST_ONLY_FIXTURE: do not deploy this fixture to mainnet";

/// Returns `Ok(())` for non-mainnet networks and fails loudly for mainnet.
pub fn assert_non_mainnet(network: &str) -> Result<(), &'static str> {
    if network.eq_ignore_ascii_case("mainnet") {
        Err(NO_ACCIDENTAL_MAINNET)
    } else {
        Ok(())
    }
}
