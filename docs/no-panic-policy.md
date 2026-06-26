
# No-Panic Policy for Soroban Contracts

## Purpose
This policy ensures that the QuickLendX Soroban smart contracts avoid using `unwrap()`, `expect()`, and direct `panic!()` calls outside of test code. Panics in Soroban contracts consume the full gas budget and provide poor diagnostics, making them unsuitable for production use.

## Rules
- **No `unwrap()` or `expect()`** in non-test code
- **No direct `panic!()`** in non-test code
- **Use proper error handling** with `Result` and `QuickLendXError` instead
- **Test code is exempt** from this policy, as panics are acceptable in tests

## How to Comply
1. Replace `unwrap()` with `?` or `ok_or(QuickLendXError)`
2. Replace `expect()` with `?` or `ok_or(QuickLendXError)` with proper error messages
3. Replace `panic!()` with returning an appropriate `QuickLendXError`
4. Use `unwrap_or()` or `unwrap_or_else()` for safe default values when appropriate

## Clippy Configuration
A `clippy.toml` file in `quicklendx-contracts/` disallows these methods. Run `cargo clippy -- -D warnings` to check compliance.

## CI Enforcement
The CI pipeline runs clippy with strict warnings as errors, ensuring no disallowed methods are merged into the codebase.
