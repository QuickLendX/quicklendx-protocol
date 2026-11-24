
# No-Panic Policy for Soroban Contracts

## Purpose
This policy ensures that QuickLendX Soroban production code avoids `unwrap()`, `expect()`, `panic!()`, `todo!()`, and `unimplemented!()`. Panics in Soroban contracts consume the full gas budget and provide poor diagnostics, making them unsuitable for production use.

## Rules
- **No `unwrap()` or `expect()`** in non-test code
- **No `panic!()`, `todo!()`, or `unimplemented!()`** in production code
- **Use proper error handling** with `Result` and appropriate `QuickLendXError` variants instead
- **Test code is exempt** from this policy, as panics are acceptable in tests
- **Production exceptions** require a narrowly scoped lint allowance with a written reason; broad exemptions are not permitted

## How to Comply
1. Replace `unwrap()` with `?` or `ok_or(QuickLendXError)`
2. Replace `expect()` with `?` or `ok_or(QuickLendXError)` with proper error messages
3. Replace `panic!()`, `todo!()`, and `unimplemented!()` with returning an appropriate `QuickLendXError`
4. Use `unwrap_or()` or `unwrap_or_else()` for safe default values when appropriate

## Clippy Configuration
A `clippy.toml` file in `quicklendx-contracts/` disallows these methods. Run `cargo clippy -- -D warnings` to check compliance.

## CI Enforcement
Test modules that need panic assertions may add `#[allow(clippy::disallowed_macros)]` at that test-module scope. Do not add a crate-wide allowance that also suppresses production linting.

The `assert_view_only!` macro in `src/diagnostics.rs` is a narrowly scoped exception: it is an exported assertion with an existing panic-on-violation contract, so changing it to return a typed error would change its interface. Its lint allowance is attached only to that macro definition.

Run `cargo clippy --all-targets -- -D warnings` to check compliance. The repository CI currently does not invoke Clippy, so this check must be run locally until CI enforcement is added separately.
