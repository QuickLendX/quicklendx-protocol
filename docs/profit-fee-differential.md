# Profit and Fee Differential Testing

## Purpose

`profits.rs` and `fees.rs` have been refactored repeatedly (tiered fees, dust handling, treasury splits). The differential harness in `quicklendx-contracts/tests/profit_fee_golden.rs` captures a frozen golden-vector corpus and re-evaluates it against the live implementation on every CI run.

Any divergence fails CI and forces the author to explicitly bless the change.

## Corpus location

- **Test:** `quicklendx-contracts/tests/profit_fee_golden.rs`
- **Fixture:** `quicklendx-contracts/tests/fixtures/profit_fee_corpus.json`

Each vector records:

```
(investment_amount, payment_amount, fee_bps, tier, volume)
  -> (investor_return, platform_fee, treasury, dust, transaction_fee)
```

The harness also asserts `verify_no_dust` across the full corpus.

## Corpus refresh policy

1. **No auto-bless.** CI only verifies; it never rewrites the corpus.
2. **Intentional semantic changes** require a dedicated PR that:
   - Sets `ALLOW_PROFIT_FEE_CORPUS_REFRESH=1`
   - Runs `./scripts/refresh-profit-fee-corpus.sh`
   - Commits the updated `profit_fee_corpus.json`
   - Explains why the math change is correct (admin review)
3. **Minimum coverage:** corpus must contain at least 500 input combinations including:
   - `i128::MAX` boundary cases
   - Zero payment
   - Overpayment scenarios
   - All volume tiers (`Standard`, `Silver`, `Gold`, `Platinum`)

## Commands

```bash
# Verify (CI)
cd quicklendx-contracts
cargo test --test profit_fee_golden

# Bless (local, admin only)
ALLOW_PROFIT_FEE_CORPUS_REFRESH=1 ./scripts/refresh-profit-fee-corpus.sh
```

## Bless semantics

The corpus loader documents that expected outputs are authoritative. Regenerating the corpus is equivalent to approving a semantic change in profit/fee math. Reviewers must treat corpus diffs as first-class logic changes, not test noise.
