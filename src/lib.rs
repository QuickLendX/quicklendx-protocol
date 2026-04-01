/// QuickLendX Smart Contract Library
///
/// This crate contains the core arithmetic modules for the QuickLendX
/// invoice-financing protocol built on Stellar's Soroban platform.
///
/// ## Modules
///
/// - [`settlement`]    — Invoice settlement payout computation
/// - [`fees`]          — Protocol fee calculations (origination, servicing, default, early-repayment)
/// - [`profits`]       — Investor return metrics and platform revenue aggregation
/// - [`verification`]  — Centralized guards preventing unverified actors from restricted actions
///
/// ## Safety Philosophy
///
/// All financial arithmetic uses `u128` with `checked_*` operations.
/// Any computation that would overflow returns `None`; callers must handle
/// this as an error condition. This eliminates silent wrapping overflow,
/// underflow, and sign-extension bugs.
///
/// The verification module enforces a **deny-by-default** policy: every
/// restricted action requires the caller to prove verified status through
/// a guard function.  Pending, rejected, and unknown actors are blocked.

pub mod fees;
pub mod profits;
pub mod settlement;
pub mod verification;

#[cfg(test)]
mod test_fuzz;

#[cfg(test)]
mod test_business_kyc;

#[cfg(test)]
mod test_investor_kyc;