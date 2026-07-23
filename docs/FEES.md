# QuickLendX fee and treasury guide

This guide is for protocol operators and reviewers who need to verify how
QuickLendX charges platform fees, where those fees are routed, and how an
administrator updates the configuration safely.

## Current fee model

QuickLendX stores the active fee settings in `FeeConfig` under
`DataKey::FeeConfig`.

```rust
pub struct FeeConfig {
    pub fee_bps: u32,
    pub treasury: soroban_sdk::Address,
}
```

The fee rate is expressed in basis points:

- `0 bps` means `0.00%`.
- `200 bps` means `2.00%`.
- `1000 bps` means `10.00%`.

The contract rejects any fee rate above `1000 bps`, so the protocol-level
platform fee cannot exceed `10%`.

## How fees are calculated

Settlement math uses integer basis-point arithmetic. The denominator is
`10_000`, so the fee calculation is:

```text
protocol_fee = fee_base_amount * fee_bps / 10_000
```

For example, if the fee base is `1_000_000` stroops and `fee_bps` is `200`, the
protocol fee is:

```text
1_000_000 * 200 / 10_000 = 20_000 stroops
```

The settlement helper keeps the accounting identity explicit:

```text
investor_payout + protocol_fee = total_collected
```

When a payment produces no investor profit or represents a loss, the profit-fee
path does not take fees from principal. Tests such as `test_fee_only_from_profit_not_principal`,
`test_zero_profit_investor_recovers_full_payment`, and
`test_loss_settlement_no_fee_investor_gets_payment` document that invariant.

## Treasury routing

Collected protocol fees are routed to the configured treasury address from
`FeeConfig.treasury`.

Operationally, the treasury should be:

1. Controlled by the protocol operator or governance process.
2. Monitored off-chain for received fee transfers and reconciliation.
3. Rotated only through the admin-controlled configuration path.

Do not send fees to an end-user wallet, temporary deployment wallet, or a
contract address that is not part of the treasury runbook.

## Updating fees

Only the current admin can update fee configuration.

The safe operator flow is:

1. Read the current `FeeConfig`.
2. Prepare the proposed `FeeConfig` with the new `fee_bps` and treasury.
3. Call the read-only preview path first:

   ```text
   preview_fee_config(admin, proposed_fee_config)
   ```

4. Confirm the returned diff:

   - `current` matches the expected live settings.
   - `projected.fee_bps <= 1000`.
   - `projected.treasury` is the intended treasury address.
   - `is_noop` is `false` unless the operator intentionally wants no change.

5. Apply the change:

   ```text
   set_fee_config(admin, proposed_fee_config)
   ```

6. Verify the emitted `fee_cfg` event and update off-chain runbooks or dashboards
   that display the active platform fee.

## Validation and failure cases

The contract validates fee configuration before writing it to storage.

| Scenario | Expected result |
| --- | --- |
| `fee_bps <= 1000` | Configuration may be accepted if the caller is admin |
| `fee_bps > 1000` | Rejected with `InvalidFee` |
| Non-admin caller | Rejected with `NotAdmin` |
| Contract not initialized | Rejected with `NotInitialized` |

All arithmetic in the fee and settlement helpers is checked. Invalid inputs,
overflow, or underflow return failure instead of silently producing a partial
fee state.

## Reviewer checklist

Before approving a fee configuration change, verify:

- The new fee is stated in both bps and percent.
- The treasury address is copied from the approved operator source of truth.
- The preview diff matches the intended change.
- Existing settlement and profit-fee tests still pass.
- Any external documentation, dashboard copy, or support guidance is updated if
  the visible fee changes.

## Related references

- `src/storage_types.rs` defines `FeeConfig`.
- `src/admin.rs` implements `preview_fee_config` and `set_fee_config`.
- `src/settlement.rs` documents settlement-level protocol fee accounting.
- `src/profits.rs` documents profit and platform revenue helpers.
- `docs/contracts/fees.md` contains deeper contract-level fee documentation.
