# Issue #271 - Bid Placement and Withdrawal Test Output

## Branch
`test/bid-placement-withdrawal`

## Added test scenarios

### place_bid
- valid placement on verified invoice
- unverified investor rejected
- over investment limit rejected
- wrong invoice status rejected

### withdraw_bid
- owner can withdraw
- non-owner fails authorization
- already accepted bid cannot be withdrawn
- already withdrawn bid cannot be withdrawn again

## Command executed

```bash
TMPDIR=/home/knights/Documents/Project/Grantfox/quicklendx-protocol/quicklendx-contracts/target/tmp \
CARGO_INCREMENTAL=0 \
cargo test -q test_bid_placement_withdrawal --manifest-path quicklendx-contracts/Cargo.toml
```

## Output

```text
error: failed to write /home/knights/Documents/Project/Grantfox/quicklendx-protocol/quicklendx-contracts/target/debug/deps/libcfg_if-7b3c8e125d478159.rmeta: Invalid cross-device link (os error 18)

error: could not compile `cfg-if` (lib) due to 1 previous error
error: failed to write /home/knights/Documents/Project/Grantfox/quicklendx-protocol/quicklendx-contracts/target/debug/deps/libunicode_ident-4e834354f17b77fe.rmeta: Invalid cross-device link (os error 18)

error: could not compile `unicode-ident` (lib) due to 1 previous error
error: failed to write /home/knights/Documents/Project/Grantfox/quicklendx-protocol/quicklendx-contracts/target/debug/deps/libitoa-8192ccb0c3a71173.rmeta: Invalid cross-device link (os error 18)

error: could not compile `itoa` (lib) due to 1 previous error
error: failed to write /home/knights/Documents/Project/Grantfox/quicklendx-protocol/quicklendx-contracts/target/debug/deps/libryu-fd341451ca1c9ecf.rmeta: Invalid cross-device link (os error 18)

error: could not compile `ryu` (lib) due to 1 previous error
error: failed to write /home/knights/Documents/Project/Grantfox/quicklendx-protocol/quicklendx-contracts/target/debug/deps/libversion_check-53da2be791d0fddd.rmeta: Invalid cross-device link (os error 18)

error: could not compile `version_check` (lib) due to 1 previous error
error: failed to write /home/knights/Documents/Project/Grantfox/quicklendx-protocol/quicklendx-contracts/target/debug/deps/libmemchr-57ea94e995af2e65.rmeta: Invalid cross-device link (os error 18)

error: could not compile `memchr` (lib) due to 1 previous error
error: failed to write /home/knights/Documents/Project/Grantfox/quicklendx-protocol/quicklendx-contracts/target/debug/deps/liblibc-b844306efc5541aa.rmeta: Invalid cross-device link (os error 18)

error: could not compile `libc` (lib) due to 1 previous error
error: failed to write /home/knights/Documents/Project/Grantfox/quicklendx-protocol/quicklendx-contracts/target/debug/deps/libtypenum-4e859bc8fc280186.rmeta: Invalid cross-device link (os error 18)

error: could not compile `typenum` (lib) due to 1 previous error
```

## Notes
- Test execution is blocked in this environment by filesystem link behavior, not by contract/test logic.
- Please run `cargo test` in CI or a local environment without this `Invalid cross-device link` restriction to validate coverage percentage.
