//! Payment and escrow operations: create escrow, release, refund, and token transfers.
//!
//! Public release/refund entry points are wrapped with a reentrancy guard in lib.rs.
//!
//! The implementation is split by responsibility into the private `escrow`,
//! `rate_limit`, `repayment`, `reserve` and `transfer` submodules. Every item that
//! was previously reachable at `crate::payments::<item>` is re-exported here, so
//! call sites are unchanged.

mod escrow;
mod rate_limit;
mod repayment;
mod reserve;
mod transfer;

#[cfg(test)]
mod test_support;

pub use escrow::*;
pub use rate_limit::*;
pub use repayment::*;
pub use reserve::*;
pub use transfer::*;
