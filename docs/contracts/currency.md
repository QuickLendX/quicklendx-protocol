# Currency Whitelist

Admin-managed list of token addresses allowed for invoice currency and bids.

## Behavior

- Empty whitelist → all currencies allowed (backward compat)
- Non-empty whitelist → only listed tokens accepted on invoice creation and bids
- Duplicate adds are ignored (idempotent)

## Functions

| Function                                          | Auth   | Description                   |
| ------------------------------------------------- | ------ | ----------------------------- |
| `add_currency(admin, currency)`                   | Admin  | Add token to whitelist        |
| `remove_currency(admin, currency)`                | Admin  | Remove token from whitelist   |
| `set_currencies(admin, currencies)`               | Admin  | Atomic bulk replace (deduped) |
| `clear_currencies(admin)`                         | Admin  | Reset to allow-all state      |
| `is_allowed_currency(currency)`                   | Public | Check if token is whitelisted |
| `get_whitelisted_currencies()`                    | Public | Return full list              |
| `get_whitelisted_currencies_paged(offset, limit)` | Public | Paginated read                |
| `currency_count()`                                | Public | Return list length            |

## Security

- Every write requires `admin.require_auth()` + admin storage check
- No user can modify the whitelist
- Use `set_currencies` for bulk updates to avoid partial state

## Errors

| Error             | Cause                                                |
| ----------------- | ---------------------------------------------------- |
| `NotAdmin`        | Caller is not the registered admin                   |
| `InvalidCurrency` | Token not in whitelist (when whitelist is non-empty) |
