# QLX Invoice Lock Time Limits

> Audience: contributors who need the practical answer to “how long can an invoice remain locked before it is auto-released?”

The short version is: there is no single invoice lock timer that always auto-releases funds. QuickLendX has two lock styles, and both behave differently:

- **Admin freeze**: a permanent lock that does not expire on its own.
- **Escrow hold after funding**: a lock that remains until an explicit release, refund, or withdrawal action.
- **Default eligibility**: the closest thing to a time-based release window, but it still requires an explicit trigger.

## 1. Admin freeze: effectively indefinite

An admin can freeze an invoice with the `freeze_invoice` entrypoint. Once set, the invoice is blocked for financial writes and is treated as locked until a separate on-chain intervention changes that state.

### Concrete behaviour

- Entry point: `freeze_invoice(env, admin, invoice_id)`
- Effect: the invoice gets the `InvoiceFrozen` flag.
- Result: operations such as `place_bid`, `accept_bid`, `record_payment`, and `settle_invoice` fail with `InvoiceFrozen`.

There is no public `unfreeze_invoice` entrypoint in the contract interface. The underlying storage helper exists, but the public contract surface does not expose a time-based auto-release for this state.

### Practical takeaway

If you are tracing a lock that came from admin freeze, treat it as **indefinite until manual intervention** rather than as a countdown-based lock.

## 2. Escrow hold: no auto-release timer

When `accept_bid_and_fund` succeeds, the invoice moves into a funded/escrow-held state. The investor’s funds stay in escrow and the invoice remains effectively locked from a funds-release perspective until one of the following is called explicitly:

- `release_escrow_funds`
- `refund_escrow_funds`
- `withdraw_investment`

### Concrete behaviour

- The escrow status becomes `Held` after funding.
- The funds remain locked until one of the explicit terminal actions above is invoked.
- There is no built-in expiry window for this hold.

### Practical takeaway

If you are debugging an invoice that is still “locked” after funding, the answer is usually “it is waiting for an explicit release or refund path,” not “it will expire automatically after $N$ seconds.”

## 3. Default path: the only bounded time window

The closest thing to a time-based lock release is the default path. Once the invoice reaches its due date and then passes the configured grace period, the invoice becomes eligible for default handling.

### How the timer is resolved

The grace deadline is resolved in this order:

1. An explicit override passed to `mark_invoice_defaulted`
2. The configured protocol grace period (`grace_period_seconds`)
3. The default fallback of `7 days` (`604_800` seconds)

The implementation also bounds the maximum grace period at `30 days` (`2_592_000` seconds).

### Concrete example

Suppose an invoice is due on `2026-07-15` and the protocol uses the default grace period of `7 days`.

| Invoice due date | Grace period | Default becomes eligible after |
|------------------|--------------|-------------------------------|
| `2026-07-15`     | `7 days`     | `2026-07-22`                  |

At that point, anyone can call the default entrypoint to transition the invoice to a defaulted state and refund the escrow. This is still a **manual trigger**, not an automatic on-chain release.

## 4. Contributor guidance

When you are implementing or reviewing invoice-lock logic, use this rule of thumb:

- If the lock came from an admin freeze, assume it is **indefinite**.
- If the lock came from funded escrow, assume it is **waiting for an explicit release/refund/withdraw action**.
- If the invoice is past due, check the **grace period + default path** rather than looking for a general auto-release timer.

## Related docs

- [INVOICE_LOCK.md](INVOICE_LOCK.md) for the broader invoice-lock overview
- [INVOICE_LIFECYCLE.md](INVOICE_LIFECYCLE.md) for the invoice state machine
- [ESCROW.md](ESCROW.md) for the escrow lifecycle and terminal actions
- [ERROR_CODES.md](ERROR_CODES.md) for the `InvoiceFrozen` error reference
