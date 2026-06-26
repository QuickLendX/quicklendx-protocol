# TODO: Hostile reentrancy fault-injection suite

- [x] Create `quicklendx-contracts/src/test_reentrancy_fault_injection.rs`
  - [x] Implement test harness + HostileToken contract(s) or helper pattern
  - [x] Implement hostile token behavior: re-enter QuickLendX during token transfer
  - [x] Drive guarded entrypoints:
    - [x] accept_bid_and_fund
    - [x] process_partial_payment
    - [x] settle_invoice
    - [x] refund_escrow
    - [x] release_escrow
  - [x] Assertions: any re-entry fails pre-mutation with `OperationNotAllowed`
  - [x] Edge cases: deeply nested + alternating entrypoints
  - [x] Add security doc comments + P0 classification
- [x] Create `docs/reentrancy-fault-injection.md`
  - [x] Explain guard mechanism and hostile token approach
  - [x] Document P0 note
- [ ] Run `cargo test test_reentrancy_fault_injection`
- [ ] Fix compile/test failures until green

