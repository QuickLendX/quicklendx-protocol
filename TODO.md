# CI Test Fix TODO

## Failures to fix (9 total)

### [x] 1. `profits.rs` - test_investor_platform_treasury_sum_invariant
   - [x] Replace `PlatformFee::calculate_breakdown(&env, ...)` with `calculate_breakdown_with_fee_bps(..., 200)` (pure function, no storage access)

### [x] 2. `payments.rs` - transfer_funds MIN_TRANSFER validation
   - [x] Add MIN_TRANSFER check in `transfer_funds` function

### [x] 3. `payments.rs` - Non-existent token address handling
   - [x] Handle `token_client.balance(from)` failure gracefully (unregistered token)

### [x] 4. `lib.rs` - get_escrow_status unwrap
   - [x] Replace `.unwrap()` with `.ok_or(QuickLendXError::StorageKeyNotFound)?`

### [x] 5. `test_cancel_invoice_matrix.rs` - Invoice::new outside contract
   - [x] Register a contract and wrap `Invoice::new()` in `env.as_contract()`

## Verification
- [ ] Run `cargo test -p quicklendx-contracts --lib` to verify all 9 tests pass
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings`

