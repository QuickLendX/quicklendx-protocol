/// QuickLendX Smart Contract Library
///
/// This crate contains the core arithmetic modules for the QuickLendX
/// invoice-financing protocol built on Stellar's Soroban platform.
///
/// ## Modules
///
/// - [`settlement`] — Invoice settlement payout computation
/// - [`fees`]       — Protocol fee calculations (origination, servicing, default, early-repayment)
/// - [`profits`]    — Investor return metrics and platform revenue aggregation
///
/// ## Safety Philosophy
///
/// All financial arithmetic uses `u128` with `checked_*` operations.
/// Any computation that would overflow returns `None`; callers must handle
/// this as an error condition. This eliminates silent wrapping overflow,
/// underflow, and sign-extension bugs.
pub mod fees;
pub mod profits;
pub mod settlement;

#[cfg(test)]
mod test_fuzz;
