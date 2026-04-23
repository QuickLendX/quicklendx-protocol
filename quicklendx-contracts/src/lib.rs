#![no_std]

//! QuickLendX contracts library — minimal surface.
//!
//! The historical contract implementation lives in the `src/*.rs` sibling
//! modules but is not wired in yet because the legacy test suite is mid-
//! migration (see the `# temporarily disabled` note in
//! `.github/workflows/ci.yml`). Until the legacy modules are restored, this
//! file exposes only the pure, self-contained utility layer plus its tests.

extern crate alloc;

pub mod pagination;

#[cfg(test)]
mod test_queries;

#[cfg(test)]
mod test_investment_queries;
