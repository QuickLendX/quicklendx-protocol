#![no_std]

//! QuickLendX contracts library — minimal surface.
//!
//! The historical contract implementation lives in the `src/*.rs` sibling
//! modules but is not wired in yet because the legacy test suite is mid-
//! migration (see the `# temporarily disabled` note in
//! `.github/workflows/ci.yml`). Until the legacy modules are restored, this
//! file exposes only the pure, self-contained utility layer plus a minimal
//! placeholder contract.
//!
//! The placeholder `#[contract]` is required for the `wasm32v1-none` release
//! build: Soroban's contract macros install the `#[panic_handler]` and wire
//! the SDK's global allocator, both of which are mandatory on that target.

extern crate alloc;

use soroban_sdk::{contract, contractimpl};

pub mod pagination;

/// Placeholder QuickLendX contract.
///
/// Exists to satisfy `wasm32v1-none` build requirements (panic handler and
/// global allocator come from the Soroban SDK via the `#[contract]` /
/// `#[contractimpl]` macros). It also exposes the query hard-cap as a
/// read-only function so that indexers and UIs can discover the current
/// `MAX_QUERY_LIMIT` on-chain.
#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    /// Return the hard cap enforced by every paginated query endpoint.
    ///
    /// This value is intentionally part of the contract's public surface so
    /// that off-chain consumers (indexers, frontends) can paginate safely
    /// without hardcoding a client-side constant that might drift from the
    /// contract.
    pub fn max_query_limit() -> u32 {
        crate::pagination::MAX_QUERY_LIMIT
    }
}

#[cfg(test)]
mod test_queries;

#[cfg(test)]
mod test_investment_queries;
