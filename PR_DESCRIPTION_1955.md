## Summary

Closes #1955

Adds regression coverage for the governance proposal guard so open proposals remain blocked from execution, passed proposals execute once, and rejected/executed proposals cannot be re-run.

## What changed

- Added unit tests for the governance proposal lifecycle guard in [quicklendx-contracts/src/test_governance.rs](quicklendx-contracts/src/test_governance.rs)
- Wired the new test module into [quicklendx-contracts/src/lib.rs](quicklendx-contracts/src/lib.rs)

## Testing

- Formatted the Rust sources with `cargo fmt --all`
- Attempted to run `cargo test -p quicklendx-contracts test_governance -- --nocapture`
- The local Windows environment is currently blocked by a missing MSVC linker (`link.exe`), so full test execution could not complete here
