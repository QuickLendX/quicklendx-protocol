# Multi-Currency Whitelist

Admin-managed whitelist of token addresses allowed for invoice currency. Invoice creation and bidding are rejected when the invoice’s currency is not whitelisted (when the whitelist is non-empty).

## Entrypoints

| Entrypoint                   | Visibility     | Description                                                          |
| ---------------------------- | -------------- | -------------------------------------------------------------------- |
| `add_currency`               | Public (admin) | Add a token address to the whitelist. Idempotent if already present. |
| `remove_currency`            | Public (admin) | Remove a token address from the whitelist.                           |
| `is_allowed_currency`        | Public         | Return whether a token is currently whitelisted.                     |
| `get_whitelisted_currencies` | Public         | Return the full list of whitelisted token addresses.                 |

## Enforcement

- **Invoice creation** (`store_invoice`, `upload_invoice`): Before creating an invoice, the contract calls `require_allowed_currency(env, &currency)`. If the whitelist is non-empty and the currency is not in it, the call fails with `InvalidCurrency`.
- **Bidding** (`place_bid`): Before accepting a bid, the contract checks the invoice’s currency with `require_allowed_currency`. Bids on invoices whose currency is not whitelisted (when the whitelist is set) fail with `InvalidCurrency`.

## Backward Compatibility

When the whitelist is **empty**, all currencies are allowed. This keeps existing deployments and tests working without an initial admin setup. Once at least one currency is added, only whitelisted tokens are accepted for new invoices and bids.

## Admin-Only

Only the contract admin (from `AdminStorage::get_admin`) may call `add_currency` and `remove_currency`. The caller must pass the admin address and that address must match the stored admin; `require_auth()` is required for that address. Non-admin callers receive `NotAdmin`.

## Supported Use Case

Supports USDC, EURC, and other stablecoins: admin adds each token address to the whitelist; only those tokens can be used as invoice currency and for bids.
