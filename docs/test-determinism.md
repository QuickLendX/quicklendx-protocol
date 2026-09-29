# Test Determinism and Seed Management

This document describes the unified seed management system for reproducible fuzz testing across all QuickLendX harnesses.

## Overview

All fuzz harnesses now support the `QUICKLENDX_SEED` environment variable for deterministic seeding, enabling:
- **Reproducible CI runs** - Same seed produces identical test sequences
- **Easier debugging** - Reproduce specific failures locally
- **Bisection support** - Deterministic runs improve fault isolation

## Usage

### Deterministic Testing
```bash
# Use specific seed across all harnesses
QUICKLENDX_SEED=42 cargo test --features fuzz-tests

# Test specific harness with deterministic seed
QUICKLENDX_SEED=1337 cargo test --features fuzz-tests test_fuzz_invoice_metadata
```

### Random Testing (Default)
```bash
# Uses OS random seeding (maximum coverage)
cargo test --features fuzz-tests
```

## Seed-Shrink Workflow

When a fuzz test fails:

1. **Capture the failure** - Note the failing input from test output
2. **Reproduce with seed** - Use `QUICKLENDX_SEED=<value>` to reproduce
3. **Store canonical seed** - Add to `proptest-regressions/` if needed
4. **Debug deterministically** - Consistent replay enables debugging

## Implementation

### Unified Helper
The `src/test_seed.rs` module provides:
- `seed()` function for consistent seed handling
- Environment variable parsing with validation
- Fallback to OS random when unset

### Integration
Harnesses use the pattern:
```rust
#![proptest_config({
    let mut config = ProptestConfig::from_env();
    if let Some(seed_array) = crate::test_seed::seed() {
        config.rng_algorithm = proptest::test_runner::RngAlgorithm::ChaCha;
    }
    config
})]
```

## Error Handling

- **Invalid seed values** panic with clear error messages
- **Missing environment variable** falls back to OS random
- **Parsing errors** show expected format: `QUICKLENDX_SEED=42`

## CI Integration

For reproducible CI runs:
```yaml
- name: Run deterministic fuzz tests
  run: QUICKLENDX_SEED=12345 cargo test --features fuzz-tests
```

This ensures identical test sequences across CI runs for consistent bisection and debugging.

## Regression Testing

Store known-good seeds in `proptest-regressions/` directory:
- `currency_whitelist_seed_42.txt`
- `invoice_metadata_seed_1337.txt`
- `escrow_model_seed_999.txt`

These enable regression testing against previously discovered edge cases.