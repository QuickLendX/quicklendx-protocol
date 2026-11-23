# test_withdraw_bid_matrix - Implementation & Execution Guide

## Summary

The comprehensive test suite for the `withdraw_bid` functionality has been **successfully implemented**. This document covers what was created and how to run the tests.

## Files Created/Modified

### New Files
- **[src/test_withdraw_bid_matrix.rs](quicklendx-contracts/src/test_withdraw_bid_matrix.rs)** - Complete test suite with 20+ tests

### Modified Files
- **[src/lib.rs](quicklendx-contracts/src/lib.rs)** - Added module registration (line ~126)
- **[src/contract.rs](quicklendx-contracts/src/contract.rs)** - Added helper functions:
  - `get_bids_by_status()` - Query bids by status
  - `get_bids_by_investor()` - Query bids by investor

## Test Coverage

The test suite validates three core requirements:

### 1. Authorization Matrix (5 tests)
- ✅ Investor can withdraw their own placed bid
- ✅ Reject withdrawal by third-party investor
- ✅ Reject withdrawal by business owner
- ✅ Reject withdrawal by admin
- ✅ Reject withdrawal by different investor

**Tests:**
- `test_investor_can_withdraw_own_placed_bid()`
- `test_third_party_investor_cannot_withdraw_bid()`
- `test_business_owner_cannot_withdraw_bid()`
- `test_admin_cannot_withdraw_bid()`
- `test_different_investor_cannot_withdraw_bid()`

### 2. State Precondition Validation (5+ tests)
- ✅ Only Placed bids can be withdrawn
- ✅ Reject withdrawal from Withdrawn state
- ✅ Reject withdrawal from Cancelled state
- ✅ Reject withdrawal from Accepted state
- ✅ Reject withdrawal from Expired state
- ✅ Reject withdrawal of non-existent bids

**Tests:**
- `test_withdraw_bid_from_withdrawn_state_fails()`
- `test_withdraw_bid_from_cancelled_state_fails()`
- `test_withdraw_bid_from_accepted_state_fails()`
- `test_withdraw_bid_from_expired_state_fails()`
- `test_withdraw_nonexistent_bid_fails()`

### 3. Ranking Exclusion (3+ tests)
- ✅ Withdrawn bids excluded from `get_best_bid()`
- ✅ Withdrawn bids excluded from `rank_bids()`
- ✅ Withdrawn bids tracked in `get_bids_by_status()`

**Tests:**
- `test_withdrawn_bid_excluded_from_best_bid()`
- `test_withdrawn_bid_excluded_from_rank_bids()`
- `test_withdrawn_bid_reflected_in_status_index()`

### 4. Edge Cases (3+ tests)
- ✅ Expired bids cannot be withdrawn
- ✅ Re-bidding after withdrawal
- ✅ Acceptance rejection on withdrawn bids

**Tests:**
- `test_expired_bid_cannot_be_withdrawn()`
- `test_investor_can_rebid_after_withdrawal()`
- `test_acceptance_fails_on_withdrawn_bid()`

## Running the Tests

### Option 1: Using Docker (Recommended for Windows)

```powershell
cd c:\Users\Directorflo\OneDrive\Desktop\quicklendx-protocol
docker run --rm -v "${PWD}:/workspace" -w /workspace/quicklendx-contracts rust:latest cargo test -p quicklendx-contracts test_withdraw_bid_matrix -- --nocapture
```

**Requirements:**
- Docker Desktop installed and running
- Rust image will be downloaded automatically (~2GB)

**Advantages:**
- No local build tools installation needed
- Consistent environment
- Works on any OS

### Option 2: Using WSL (Windows Subsystem for Linux)

```powershell
cd c:\Users\Directorflo\OneDrive\Desktop\quicklendx-protocol
wsl bash -c "cd /mnt/c/Users/Directorflo/OneDrive/Desktop/quicklendx-protocol/quicklendx-contracts && cargo test -p quicklendx-contracts test_withdraw_bid_matrix -- --nocapture"
```

**Requirements:**
- WSL 2 installed with Ubuntu
- Rust installed in WSL

**Setup WSL:**
```powershell
wsl --install
wsl bash -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
```

### Option 3: Native Windows (Manual Setup Required)

```powershell
cd quicklendx-contracts
cargo test -p quicklendx-contracts test_withdraw_bid_matrix -- --nocapture
```

**Requirements:**
- Rust toolchain (already installed)
- Visual C++ Build Tools
- Windows SDK

**Install Build Tools:**
1. Download from: https://aka.ms/vs/17/release/vs_BuildTools.exe
2. Run installer with C++ workload
3. Restart terminal to refresh PATH

### Option 4: Use Helper Script

```powershell
cd c:\Users\Directorflo\OneDrive\Desktop\quicklendx-protocol
.\run-withdraw-bid-tests.ps1
```

**With options:**
```powershell
.\run-withdraw-bid-tests.ps1 -UseDocker      # Force Docker
.\run-withdraw-bid-tests.ps1 -UseWSL         # Force WSL
.\run-withdraw-bid-tests.ps1 -InstallDeps    # Install/update dependencies first
```

## Expected Output

When tests pass, you should see:

```
running 20 tests

test withdraw_bid::tests::test_investor_can_withdraw_own_placed_bid ... ok
test withdraw_bid::tests::test_withdraw_bid_returns_ok_on_success ... ok
test withdraw_bid::tests::test_third_party_investor_cannot_withdraw_bid ... ok
test withdraw_bid::tests::test_business_owner_cannot_withdraw_bid ... ok
...

test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 1000+ filtered out
```

## Test File Structure

The test file (`test_withdraw_bid_matrix.rs`) contains:

```rust
// Helper setup function
fn setup() -> (Env, QuickLendXContractClient, Address, Address)

// Helper bid placement function  
fn place_bid(env: &Env, client: &QuickLendXContractClient, admin: Address) 
    -> (BytesN<32>, Address, BytesN<32>)

// Authorization tests (5)
#[test]
fn test_investor_can_withdraw_own_placed_bid()

#[test]  
fn test_third_party_investor_cannot_withdraw_bid()

// ... more tests

// State precondition tests (5+)
#[test]
fn test_withdraw_bid_from_withdrawn_state_fails()

// ... more tests

// Ranking exclusion tests (3+)
#[test]
fn test_withdrawn_bid_excluded_from_best_bid()

// ... more tests

// Edge case tests (3+)
#[test]
fn test_expired_bid_cannot_be_withdrawn()

// ... more tests
```

## Troubleshooting

### Issue: "cargo: command not found"
- **Solution:** Restart terminal or refresh PATH
- **Windows:** Restart PowerShell
- **Command:** `$env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")`

### Issue: "linker \`link.exe\` not found"
- **Solution:** Install Visual C++ Build Tools (see Option 3 above)
- **Or use Docker/WSL instead**

### Issue: "Docker daemon not running"
- **Solution:** Start Docker Desktop application
- **Or use WSL instead**

### Issue: Tests timeout
- **Solution:** Use `-Z build-std` for faster builds
- **Command:** `cargo test -Z build-std --target wasm32-unknown-unknown`

## CI/CD Integration

This test is automatically run on:
- All pull requests to `main` branch
- All commits to `main` branch
- GitHub Actions CI/CD pipeline: `.github/workflows/ci.yml`

## Success Criteria

All tests pass when:
1. ✅ `withdraw_bid` enforces investor-only authorization
2. ✅ `withdraw_bid` only allows withdrawal from Placed state
3. ✅ Withdrawn bids are excluded from ranking operations
4. ✅ Bid status transitions are properly tracked
5. ✅ All edge cases are handled correctly

## Implementation Details

The test suite leverages:
- **Soroban SDK testutils** for mocking and authorization
- **MockAuth** for multi-caller authorization testing
- **Contract interface** for integration testing
- **BidStatus enum** for state validation
- **QuickLendXError** for error handling

## Next Steps

1. Run tests using one of the methods above
2. If all pass → 🎉 Implementation complete
3. If failures → Review error messages and fix issues
4. Commit changes to repository

## References

- [Soroban Testing Guide](https://developers.stellar.org/docs/build/smart-contracts/testing)
- [QuickLendX Protocol](quicklendx-contracts/README.md)
- [withdraw_bid Implementation](quicklendx-contracts/src/lib.rs#L1285)
