# Mutation testing

Mutation testing measures whether tests detect small behavioral changes. The
checked-in `.cargo/mutants.toml` scopes analysis to `profits.rs`, `fees.rs`,
`settlement.rs`, and `escrow.rs`, runs only `cargo test --lib`, and excludes
generated `__spec` items. Mutation testing is opt-in and is not part of default
CI.

## Run locally

Install the tool once:

```bash
cargo install --locked cargo-mutants
```

From `quicklendx-contracts`, list the selected mutations:

```bash
bash scripts/mutation-test.sh --list
```

Run the mutation suite:

```bash
bash scripts/mutation-test.sh
```

By default, each mutant test is limited to 300 seconds, each build to 900
seconds, and two mutants run concurrently. Override these limits with
`MUTANTS_TIMEOUT`, `MUTANTS_BUILD_TIMEOUT`, and `MUTANTS_JOBS`. Results are
written under `target/mutants.out`, which is ignored by Git. The script uses
cargo-mutants' copied-tree mode so mutations do not edit the working tree.

## Baseline

`bash scripts/mutation-test.sh --list` selects 654 mutants across the four
configured modules and excludes all generated `__spec` items.

The initial `cargo test --lib` attempt did not compile: the crate currently
reports 89 errors, including duplicate audit variants and contract definitions
and unresolved APIs in existing source files. Consequently, no mutants have
been executed and there is not yet a measured kill/survivor count. Rerun the
script after the library test suite compiles.

## Triage

Review surviving mutants by behavior, not just count:

- Add a focused example or property assertion when a meaningful behavior
  change survives.
- Mark equivalent mutants only when their equivalence is justified.
- Treat survivors affecting fee arithmetic, repayment allocation, or escrow
  terminality as correctness gaps until disproven.
