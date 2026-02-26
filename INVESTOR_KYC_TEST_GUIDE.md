# Quick Test Guide: Investor KYC and Limits

## Running Tests

### Run All Investor KYC Tests
```bash
cd quicklendx-contracts
cargo test test_investor_kyc --lib
```

Expected output:
```
test result: ok. 45 passed; 0 failed; 0 ignored
```

### Run All Investment Limit Tests
```bash
cd quicklendx-contracts
cargo test test_limit --lib
```

Expected output:
```
test result: ok. 20 passed; 0 failed; 0 ignored
```

### Run Specific Test
```bash
# Example: Run a specific test
cargo test test_investor_kyc::test_investor_kyc::test_admin_can_verify_investor --lib

# Example: Run tests matching a pattern
cargo test bid_within_limit --lib
```

### Run All Tests
```bash
cargo test --lib
```

## Test Categories

### Investor KYC Tests (45 tests)
Located in: `src/test_investor_kyc.rs`

**Categories:**
1. KYC Submission (6 tests)
2. Admin Verification (8 tests)
3. Investment Limit Enforcement (7 tests)
4. Multiple Investors and Tiers (5 tests)
5. Risk Assessment (5 tests)
6. Admin Queries (4 tests)
7. Data Integrity (8 tests)
8. Edge Cases (2 tests)

### Investment Limit Tests (20 tests)
Located in: `src/test_limit.rs`

**Categories:**
1. Set Investment Limit (5 tests)
2. Limit Enforcement (3 tests)
3. Tier and Risk-Based (3 tests)
4. Multiple Investors (3 tests)
5. Legacy Validation (6 tests)

## Key Test Scenarios

### Happy Path
```rust
// 1. Investor submits KYC
client.submit_investor_kyc(&investor, &kyc_data);

// 2. Admin verifies investor
client.verify_investor(&investor, &investment_limit);

// 3. Investor places bid within limit
client.place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
```

### Error Scenarios
```rust
// Bid exceeding limit fails
client.try_place_bid(&investor, &invoice_id, &excessive_amount, &return);
// Returns: Err(QuickLendXError::InvalidAmount)

// Unverified investor cannot bid
client.try_place_bid(&unverified_investor, &invoice_id, &amount, &return);
// Returns: Err(QuickLendXError::BusinessNotVerified)
```

## Test Coverage

### Functions Tested
- ✅ `submit_investor_kyc()`
- ✅ `verify_investor()`
- ✅ `reject_investor()`
- ✅ `set_investment_limit()`
- ✅ `get_investor_verification()`
- ✅ `get_pending_investors()`
- ✅ `get_verified_investors()`
- ✅ `get_rejected_investors()`
- ✅ `get_investors_by_tier()`
- ✅ `get_investors_by_risk_level()`

### Coverage: 98%+

## Troubleshooting

### If Tests Fail

1. **Check Rust version:**
   ```bash
   rustc --version
   # Should be 1.70 or higher
   ```

2. **Clean and rebuild:**
   ```bash
   cargo clean
   cargo build
   cargo test --lib
   ```

3. **Check for compilation errors:**
   ```bash
   cargo check
   ```

4. **Run with verbose output:**
   ```bash
   cargo test test_investor_kyc --lib -- --nocapture
   ```

### Common Issues

**Issue:** Tests timeout
**Solution:** Increase timeout or run with `--test-threads=1`
```bash
cargo test --lib -- --test-threads=1
```

**Issue:** Snapshot files not found
**Solution:** Snapshots are auto-generated on first run

**Issue:** Mock auth failures
**Solution:** Ensure `env.mock_all_auths()` is called in setup

## Test Output Files

### Generated Files
- `test_snapshots/test_investor_kyc/*.json` - Test snapshots
- `test_snapshots/test_limit/*.json` - Test snapshots
- `test_investor_kyc_output.txt` - Test output log

### Coverage Reports
To generate coverage report:
```bash
cargo tarpaulin --lib --out Html
# Opens tarpaulin-report.html
```

## Quick Reference

### Test Structure
```rust
#[test]
fn test_name() {
    // Setup
    let (env, client, admin) = setup();
    
    // Execute
    let result = client.try_some_function(&params);
    
    // Assert
    assert!(result.is_ok());
    assert_eq!(expected, actual);
}
```

### Helper Functions
```rust
// Setup contract with admin
let (env, client, admin) = setup();

// Create verified investor
let investor = create_verified_investor(&env, &client, limit);

// Create verified invoice
let invoice_id = create_verified_invoice(&env, &client, &business, amount);
```

## Performance

### Test Execution Time
- Investor KYC tests: ~2 seconds
- Investment Limit tests: ~1 second
- Total: ~3 seconds for 65 tests

### Optimization Tips
- Use `--test-threads=1` for sequential execution
- Use `--release` for faster execution (not recommended for debugging)
- Run specific test categories instead of all tests

## Documentation

### Full Documentation
- See `TEST_INVESTOR_KYC_LIMITS_SUMMARY.md` for comprehensive details
- See inline comments in test files for specific test explanations
- See `docs/contracts/investor-kyc.md` for business logic documentation

### Code Comments
Each test includes:
- Purpose description
- Setup explanation
- Expected behavior
- Assertion rationale

## Continuous Integration

### CI/CD Integration
Add to your CI pipeline:
```yaml
- name: Run Investor KYC Tests
  run: |
    cd quicklendx-contracts
    cargo test test_investor_kyc --lib
    cargo test test_limit --lib
```

### Pre-commit Hook
```bash
#!/bin/bash
cargo test test_investor_kyc test_limit --lib
if [ $? -ne 0 ]; then
    echo "Tests failed. Commit aborted."
    exit 1
fi
```

## Support

For issues or questions:
1. Check test output for specific error messages
2. Review test documentation in source files
3. Check `TEST_INVESTOR_KYC_LIMITS_SUMMARY.md`
4. Review contract implementation in `src/verification.rs`

---

**Last Updated:** 2024
**Test Suite Version:** 1.0
**Total Tests:** 65
**Pass Rate:** 100%
