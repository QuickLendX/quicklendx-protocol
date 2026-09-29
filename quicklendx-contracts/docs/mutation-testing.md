# Mutation testing for protocol arithmetic

This repository includes a non-blocking `cargo-mutants` pipeline focused on the
modules most likely to hide high-impact arithmetic or settlement mistakes:

- `src/profits.rs`
- `src/fees.rs`
- `src/settlement.rs`
- `src/escrow.rs`

The checked-in configuration lives at `.cargo/mutants.toml` and scopes the
mutation run to those files by using `examine_globs`.

## Local commands

```bash
cd quicklendx-contracts
cargo install cargo-mutants
cargo mutants --file src/profits.rs --no-shuffle
cargo mutants --in-place=false --json > mutants.json
```

`--in-place=false` runs against a copied build tree, so committed source files are
not altered by the harness. Before committing, always run `git status --short` to
confirm no source mutation leaked into the working tree.

## CI artifact

The CI job writes `quicklendx-contracts/mutants.json` and uploads it as the
`cargo-mutants-survivors` artifact. The job is advisory (`continue-on-error`) so
that the first rollout surfaces survivor data without blocking unrelated PRs.

## Survivor triage workflow

1. Open `mutants.json` and identify every surviving mutant in profits, fees,
   settlement, or escrow logic.
2. Classify the survivor:
   - **P1 correctness gap**: profit math, fee math, settlement identity, escrow
     terminality, or principal movement changed but tests still passed.
   - **Equivalent mutant**: the change is observably identical for all valid
     inputs. Document why before accepting it.
   - **Harness gap**: the changed behavior is meaningful but the generated test
     did not exercise the path.
3. Add targeted assertions or property tests for every P1 correctness gap.
4. Re-run `cargo mutants --file <module> --no-shuffle` and attach the updated
   report to the follow-up PR.

## Security note

Any surviving mutant in profit, fee, settlement, or escrow arithmetic is treated
as a P1 follow-up because these modules determine principal conservation,
platform revenue, investor return, and double-spend resistance.
