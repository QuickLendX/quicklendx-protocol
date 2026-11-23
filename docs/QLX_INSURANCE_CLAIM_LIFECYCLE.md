# QuickLendX Insurance Claim Lifecycle

This document describes the insurance lifecycle in the QuickLendX protocol for downstream integrators and smart contract developers. It explains how an investment opts into insurance and how claims are processed upon default.

## 1. Opt-In: Adding Insurance to an Investment

An investor or protocol automation can attach insurance to an active investment. Insurance bounds the downside risk in exchange for a premium.

### Entrypoint
The coverage is added via the `Investment::add_insurance` method:

```rust
// quicklendx-contracts/src/investment.rs
pub fn add_insurance(
    &mut self,
    provider: Address,
    coverage_percentage: u32,
    premium: i128,
) -> Result<i128, QuickLendXError>
```

### Concrete Example
If an investor funds a 1,000 USDC invoice and wants 50% coverage, they might invoke this method with:
- `provider`: The Soroban `Address` of the insurance pool.
- `coverage_percentage`: `50` (meaning 50% of the principal is covered).
- `premium`: The pre-computed premium amount (e.g., `20` USDC, typically 200 bps of the coverage).

This creates an active `InsuranceCoverage` record on the investment.

## 2. Settlement Constraints

Before an invoice defaults, the system enforces that all active policies remain valid. During settlement or default transitions, `require_active_insurance_at_settlement` ensures that if an investment has policies attached, they are still active. If policies were voided prematurely, the transaction will fail with `QuickLendXError::InsuranceNotActive`.

## 3. Claim Processing: The Default Lifecycle

If the borrower fails to repay and the invoice expires, any authorized operator can trigger the default flow.

### Entrypoint
The default is processed via the `process_invoice_default` function in `quicklendx-contracts/src/defaults.rs`.

During this transition, the protocol automatically processes all active insurance claims atomically:

```rust
// quicklendx-contracts/src/investment.rs
pub fn process_all_insurance_claims(&mut self, env: &Env) -> Vec<(Address, i128)>
```

### Concrete Example
When `process_invoice_default` is called for the 1,000 USDC invoice with a 50% coverage policy:
1. The investment's status is updated to `InvestmentStatus::Defaulted`.
2. `process_all_insurance_claims` iterates over all policies. For our active policy, it sets `active = false` and adds the 500 USDC claim to the payout list.
3. The protocol emits an `InsuranceClaimed` event so off-chain indexers and downstream contracts can process the actual token transfer from the insurance provider to the investor.

### Event Output
The emitted `InsuranceClaimed` event looks like this:
```rust
InsuranceClaimed {
    investment_id: BytesN<32>, // The ID of the defaulted investment
    invoice_id: BytesN<32>,    // The ID of the underlying invoice
    provider: Address,         // The provider's address
    amount: i128,              // The exact claim amount (e.g., 500_0000000)
}
```

By listening to this event, an integrating insurance provider contract knows exactly when and how much to disburse.

---

*See also: [README](../README.md) for top-level project overview.*
