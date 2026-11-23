# Settlement Currencies by Invoice Type

**Audience:** Downstream Integrator

When creating an invoice via `store_invoice` or filtering invoices, it is important to know which tokens are accepted for each invoice category. The QuickLendX smart contracts enforce a global whitelist of currencies (e.g. USDC, EURC), but functionally, different categories of invoices are expected to settle in specific tokens.

## Supported Tokens per Category

| Invoice Category | Supported Settlement Tokens | Token Addresses (Example Testnet) |
| :--- | :--- | :--- |
| `Services` | USDC, EURC | `CDLZFC...` (USDC), `CA3T...` (EURC) |
| `Goods` | USDC | `CDLZFC...` (USDC) |
| `Consulting` | USDC, XLM | `CDLZFC...` (USDC), `CB64...` (XLM) |
| `Logistics` | USDC, EURC | `CDLZFC...` (USDC), `CA3T...` (EURC) |
| `Products` | USDC | `CDLZFC...` (USDC) |
| `Manufacturing` | USDC, EURC | `CDLZFC...` (USDC), `CA3T...` (EURC) |
| `Technology` | USDC, XLM | `CDLZFC...` (USDC), `CB64...` (XLM) |
| `Healthcare` | USDC | `CDLZFC...` (USDC) |
| `Other` | USDC, EURC, XLM | `CDLZFC...` (USDC), `CA3T...` (EURC), `CB64...` (XLM) |

## Example: Creating a Services Invoice

When calling `store_invoice` for a `Services` invoice, you should pass a supported currency address like the USDC contract address.

```rust
// Example: Storing a Services invoice settled in USDC
let invoice_id = contract.store_invoice(
    &env,
    business_addr,
    15000000, // $150.00 (assuming 6 decimals)
    usdc_token_address, // Must be USDC or EURC for Services
    due_date_timestamp,
    String::from_str(&env, "Q3 Retainer"),
    InvoiceCategory::Services,
    vec![&env, String::from_str(&env, "retainer")]
)?;
```

## Example: Filtering Available Invoices

When querying `get_available_invoices_paged`, you can assume the token type based on the category filter:

```rust
// Filter for Technology invoices; expect settlement in USDC or XLM
let tech_invoices = contract.get_available_invoices_paged(
    &env,
    None,
    None,
    Some(InvoiceCategory::Technology),
    0,
    10
);
```

## Rejection Behavior

Attempting to submit an invoice using a currency that is not globally whitelisted will result in an `InvalidCurrency` error from the `CurrencyWhitelist`. Downstream integrators should first check `get_whitelisted_currencies_paged` to ensure the token is active on the network.
