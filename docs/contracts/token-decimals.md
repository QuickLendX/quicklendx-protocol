# Token Decimals

**Audience:** Contract integrators building clients or off-chain tooling that calls QuickLendX entry-points.

## Summary

The QuickLendX contract stores and compares every amount as a raw `i128` integer — the token's smallest indivisible unit — and performs **no decimal conversion internally**. Callers are responsible for expressing amounts in the correct atomic unit before passing them to the contract.

## How Stellar token amounts work

Every SEP-0041 / Stellar Asset Contract token has a `decimals` field that declares how many places sit to the right of the decimal point in the human-readable representation. The relationship is:

```
atomic_units = human_amount × 10^decimals
```

Common values:

| Token | Decimals | 1 human unit = N atomic units |
|-------|----------|-------------------------------|
| XLM (native SAC) | 7 | 10 000 000 (1 stroop = 0.0000001 XLM) |
| USDC on Stellar | 7 | 10 000 000 |
| A custom 6-decimal token | 6 | 1 000 000 |

The contract never calls `token.decimals()` at runtime. Amounts are passed in, stored verbatim, and compared with ordinary integer arithmetic.

## What the contract does with an amount

1. **`store_invoice` / `upload_invoice`** — The `amount` argument is stored as-is in the `Invoice` struct and validated only for sign and lower-bound (`>= min_invoice_amount`).
2. **`place_bid`** — Bid amounts are stored verbatim and ranked by integer comparison.
3. **`accept_bid_and_fund`** — Calls `token.transfer_from(investor, contract, bid_amount)`. The integer passed to the token contract is the stored bid amount unchanged.
4. **Settlement / partial payments** — Arithmetic (`total_paid`, `remaining_due`, fee split) is all integer arithmetic in atomic units. No scaling step exists.

Concretely, `transfer_funds` in `payments.rs` forwards the raw `i128` directly to the Soroban token interface:

```rust
// payments.rs — simplified
token_client.transfer_from(&contract_address, from, to, &amount);
```

The protocol default minimum invoice amount constant reflects the 6-decimal convention used in tests:

```rust
// init.rs
const DEFAULT_MIN_INVOICE_AMOUNT: i128 = 1_000_000; // 1 token (6 decimals)
```

For a 7-decimal token like XLM, "1 token" is `10_000_000` atomic units, so that constant would need to be set accordingly at initialization via `set_protocol_config`.

## Integration examples

### Store a $100 USDC invoice (7 decimals)

```rust
// USDC on Stellar uses 7 decimal places.
// $100.00 = 100 × 10^7 = 1_000_000_000 atomic units
let amount: i128 = 1_000_000_000;

contract.store_invoice(
    &env,
    business_address,
    amount,           // raw atomic units — no scaling by the contract
    usdc_address,
    due_date,
    description,
    category,
    tags,
)?;
```

### Place a bid on a 6-decimal token invoice

```rust
// A 6-decimal stablecoin: $95.50 = 9_550_000 atomic units
let bid_amount: i128 = 9_550_000;
let expected_return: i128 = 10_000_000; // $100.00

contract.place_bid(
    &env,
    investor_address,
    invoice_id,
    bid_amount,
    expected_return,
)?;
```

### Reading amounts back

The amount in a returned `Invoice` or `Escrow` is still in atomic units. Convert to a human-readable value in your client:

```typescript
// TypeScript off-chain example
const atomicAmount: bigint = invoice.amount; // e.g. 1_000_000_000n
const decimals = 7; // query token.decimals() once and cache it
const humanAmount = Number(atomicAmount) / 10 ** decimals; // 100.0
```

## Fee calculations

All fee formulas (profit split, platform fee, revenue distribution) operate in atomic units throughout. For example:

```
platform_fee = floor(gross_profit × fee_bps / 10_000)
```

`gross_profit`, `platform_fee`, and `investor_return` are all `i128` values in atomic units. The only unit that matters for correctness is that _every_ amount in one transaction refers to the same token and the same decimal convention; the contract does not validate cross-token consistency beyond checking that the `currency` address stored on the invoice matches the `currency` address used in the bid.

## What to check before calling the contract

1. **Query `token.decimals()`** once per token type and cache it in your client.
2. **Convert human amounts to atomic units** (`amount × 10^decimals`) before passing them to any contract entry-point.
3. **Ensure `min_invoice_amount`** was configured for the token's decimal scale during protocol initialization. Call `get_min_invoice_amount()` to read the current value.
4. **Cross-token bids**: Bids must use the same `currency` as the invoice. The contract stores both and the escrow transfer uses the bid's currency address verbatim.

## Related documentation

- [`docs/contracts/payments.md`](./payments.md) — `transfer_funds` security prechecks and escrow lifecycle.
- [`docs/contracts/fees.md`](./fees.md) — Fee formula and basis-point arithmetic.
- [`docs/contracts/initialization.md`](./initialization.md) — How `min_invoice_amount` is set and validated at init time.
- [`docs/contracts/arithmetic-safety.md`](./arithmetic-safety.md) — Overflow safety and `checked_*` / `saturating_*` conventions.
