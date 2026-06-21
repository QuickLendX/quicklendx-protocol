# TODO: Hostile reentrancy fault-injection suite

- [ ] Create `quicklendx-contracts/src/test_reentrancy_fault_injection.rs`
  - [ ] Implement test harness + HostileToken contract(s) or helper pattern
  - [ ] Implement hostile token behavior: re-enter QuickLendX during token transfer
  - [ ] Drive guarded entrypoints:
    - [ ] accept_bid_and_fund
    - [ ] process_partial_payment
    - [ ] settle_invoice
    - [ ] refund_escrow
    - [ ] release_escrow
  - [ ] Assertions: any re-entry fails pre-mutation with `OperationNotAllowed`
  - [ ] Edge cases: deeply nested + alternating entrypoints
  - [ ] Add security doc comments + P0 classification
- [ ] Create `docs/reentrancy-fault-injection.md`
  - [ ] Explain guard mechanism and hostile token approach
  - [ ] Document P0 note
- [ ] Run `cargo test test_reentrancy_fault_injection`
- [ ] Fix compile/test failures until green

