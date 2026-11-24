# QuickLendX contract fuzzing

This directory contains the three maintained `cargo-fuzz` targets for the
contract operations covered by issue #3051:

- `accept_bid_and_fund` — bid acceptance, repeated acceptance, escrow and
  investment state invariants.
- `settlement` — partial payments, full settlement, stale investment
  snapshots, progress and payment-record queries.
- `paged_queries` — safe bounds, limits, pagination metadata and cursor
  encoding/stability helpers.

## Prerequisites

Install the nightly Rust toolchain and cargo-fuzz:

```sh
rustup toolchain install nightly
cargo +nightly install cargo-fuzz
```

Run the following commands from `quicklendx-contracts/`.

## Build all maintained targets

```sh
cargo +nightly fuzz build
```

## Run targets

A short smoke run for each target:

```sh
cargo +nightly fuzz run accept_bid_and_fund -- -max_total_time=30
cargo +nightly fuzz run settlement -- -max_total_time=30
cargo +nightly fuzz run paged_queries -- -max_total_time=30
```

For longer runs, increase `-max_total_time` or omit it to let libFuzzer
continue until interrupted. Crash inputs are saved under
`fuzz/artifacts/<target>/` and can be replayed by passing the artifact path
to the matching `cargo +nightly fuzz run <target>` command.

The fuzz manifest intentionally registers only these maintained targets, so
`cargo +nightly fuzz build` does not try to compile the older
`contracttype_roundtrip.rs` experiment, which uses obsolete Soroban XDR APIs.
