# Coverage Drift Report

The Coverage Drift Report is a CI tool that compares the code coverage of a Pull Request against the `main` branch on a per-module basis. This helps reviewers identify coverage regressions in specific files, even if the global coverage remains above the threshold.

## How it works

1.  **Baseline Generation**: In a PR workflow, CI fetches the `main` branch and runs `cargo llvm-cov --json` to get the baseline coverage data.
2.  **PR Coverage Generation**: CI runs `cargo llvm-cov --json` on the current PR state.
3.  **Drift Calculation**: The `scripts/coverage-drift.sh` script parses both JSON reports and calculates the delta for each module.
4.  **PR Comment**: A markdown table is posted as a PR comment, highlighting changes and alerting on regressions in security-critical modules.

## Security-Critical Modules

Certain modules are flagged with a warning (⚠️) if their coverage decreases:
-   `escrow.rs`
-   `settlement.rs`
-   `reentrancy.rs` (or any reentrancy protection logic)

## Coverage Gate

**Important**: This report is informational. The hard requirement for PR approval remains a **95% global line coverage** gate. The drift report encourages maintainability but does not replace the global minimum.

## Running Locally

To generate a comparison report locally, you need `cargo-llvm-cov` installed:

```bash
cargo install cargo-llvm-cov
```

Then you can run:

```bash
# Generate baseline (on main branch)
git checkout main
cargo llvm-cov --json > main_coverage.json

# Generate PR coverage (on your branch)
git checkout your-branch
cargo llvm-cov --json > pr_coverage.json

# Run the drift script
./scripts/coverage-drift.sh main_coverage.json pr_coverage.json
```

## Troubleshooting

-   **Empty Report**: Ensure that `cargo llvm-cov` is producing valid JSON. Check if tests are actually running and hitting the code.
-   **Missing Modules**: If a module was deleted, it will show up with 0% coverage in the PR (or be missing from the PR list).
-   **New Modules**: If a module is new, it will show up with 0% coverage in the Main column.
