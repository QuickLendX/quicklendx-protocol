# Emergency Mode & Pause Behavior

## Overview

QuickLendX supports two independent circuit-breaker mechanisms:

1. **Pause Mode** — controlled by `pause::PauseControl`. When active, all user-facing state-mutating operations are rejected with `ContractPaused`. Admin recovery and governance flows remain operational.
2. **Maintenance Mode** — controlled by `maintenance::MaintenanceControl`. When active, write operations are rejected with `MaintenanceModeActive`.

These flags are **independent**: a contract can be paused without being in maintenance mode, and vice-versa. Both can be active simultaneously.

## Security Model

- **Determinism**: For any given state (paused or not), the same input always produces the same output. No timing-dependent bypass exists.
- **No user bypass**: Every state-mutating entrypoint accessible to non-admin users enforces `require_not_paused`.
- **Admin recovery**: Emergency withdraw, admin rotation, KYC review, and protocol configuration remain available during pause so the admin can diagnose and recover.
- **Read-only safety**: All query/getter functions are intentionally unguarded so external observers can always inspect protocol state.

## Error Codes

| Code | Symbol | Meaning |
|------|--------|---------|
| 2100 | `PAUSED` | `ContractPaused` — returned when a mutating user flow is invoked while the protocol is paused. |
| 2200 | `MAINT` | `MaintenanceModeActive` — returned when a write is attempted during maintenance mode. |

## Blocked Flows (User-Facing)

The following entrypoints reject with `ContractPaused` when the protocol is paused:

### Invoice Lifecycle
- `store_invoice`
- `upload_invoice`
- `verify_invoice`
- `cancel_invoice`
- `update_invoice_status`
- `update_invoice_metadata`
- `clear_invoice_metadata`
- `settle_invoice`
- `process_partial_payment`
- `fund_invoice`

### Bidding
- `place_bid`
- `accept_bid`
- `accept_bid_and_fund`
- `withdraw_bid`

### Investment & Insurance
- `add_investment_insurance`

### Disputes
- `create_dispute`

### Invoice Enrichment
- `update_invoice_category`
- `add_invoice_tag`
- `remove_invoice_tag`
- `add_invoice_rating`

### Vesting
- `create_vesting_schedule`
- `release_vested_tokens`

### KYC / Onboarding
- `submit_kyc_application`
- `submit_investor_kyc`

### Escrow
- `release_escrow_funds`
- `refund_escrow_funds`

### Currency Management
- `add_currency`
- `remove_currency`
- `set_currencies`
- `clear_currencies`

### Protocol Configuration
- `set_bid_ttl_days`

## Allowed Recovery Flows (Admin / Governance)

The following entrypoints remain available during pause:

### Pause Control
- `pause` — idempotent; can re-pause an already paused contract.
- `unpause` — idempotent; can unpause an already unpaused contract.
- `is_paused` — query, always allowed.

### Emergency Withdraw
- `initiate_emergency_withdraw` — start a timelocked withdrawal of stuck funds.
- `execute_emergency_withdraw` — complete withdrawal after timelock.
- `cancel_emergency_withdraw` — cancel a pending withdrawal.
- `get_pending_emergency_withdraw` — query pending state.
- `can_exec_emergency` — query execution readiness.

### Admin Management
- `transfer_admin` — rotate admin keys.
- `get_current_admin` — query admin address.

### KYC Review
- `verify_business`
- `reject_business`
- `verify_investor`
- `reject_investor`
- `set_investment_limit`

### Protocol Limits & Fees
- `set_protocol_limits`
- `update_protocol_limits`
- `update_limits_max_invoices`
- `initialize_protocol_limits`
- `set_platform_fee`
- `update_platform_fee_bps`
- `configure_treasury`

### Analytics & Queries
All functions prefixed with `get_`, `is_`, `query_`, `calculate_`, `validate_`, etc., are read-only and remain fully operational during pause.

## Emergency Withdraw Procedure

The emergency withdraw mechanism is a **last-resort** recovery tool for stuck funds.

### Lifecycle

1. **Initiate** (`initiate_emergency_withdraw`)
   - Admin provides: token address, amount, target address.
   - A `PendingEmergencyWithdrawal` record is created with:
     - `created_at`: current ledger timestamp
     - `execute_after`: `created_at + DEFAULT_EMERGENCY_TIMELOCK_SECS` (48 hours)
     - `expires_at`: `created_at + DEFAULT_EMERGENCY_EXPIRATION_SECS` (7 days)
     - `nonce`: monotonically increasing to prevent replay
     - `cancelled`: false
   - Emits `EmergencyWithdrawInitiated` event.

2. **Wait** (timelock)
   - `execute_emergency_withdraw` will fail until `execute_after` has passed.
   - `can_exec_emergency` returns false during this period.

3. **Execute** (`execute_emergency_withdraw`)
   - Only callable by admin.
   - Validates:
     - Pending withdrawal exists
     - Not cancelled
     - Timelock elapsed (`env.ledger().timestamp() >= execute_after`)
     - Not expired (`env.ledger().timestamp() <= expires_at`)
   - On success, pending record is removed and `EmergencyWithdrawExecuted` is emitted.

4. **Cancel** (`cancel_emergency_withdraw`)
   - Admin can cancel at any time before execution.
   - Sets `cancelled = true` and records `cancelled_at`.
   - Emits `EmergencyWithdrawCancelled` event.
   - Cancelled record remains queryable via `get_pending_emergency_withdraw` for audit purposes.

### Security Properties

- **Timelock**: 48-hour minimum delay prevents instant rug-pulls even if admin keys are compromised.
- **Expiration**: 7-day maximum lifetime prevents stale withdrawals from lingering indefinitely.
- **Nonce tracking**: Prevents replay attacks if a cancelled withdrawal is re-initiated.
- **Cancellation audit trail**: Cancelled records are retained in storage (marked `cancelled`) so operators can observe the full history.

## Determinism Guarantees

- `is_paused` reads a single boolean from instance storage. No external state influences the result.
- `require_not_paused` evaluates the same storage key every time. There are no timing-dependent code paths.
- Repeated `pause`/`unpause` cycles produce identical behavior: the same operations are blocked/allowed in the same way after each transition.

## Regression Test Checklist

The following behaviors are covered by automated tests in `test_pause.rs` and `test_emergency.rs`:

- [x] `store_invoice` blocked when paused, succeeds when unpaused
- [x] `verify_invoice` blocked when paused
- [x] `accept_bid_and_fund` blocked when paused
- [x] `release_escrow_funds` blocked when paused
- [x] `refund_escrow_funds` blocked when paused
- [x] `withdraw_bid` blocked when paused
- [x] `settle_invoice` blocked when paused
- [x] `add_investment_insurance` blocked when paused
- [x] `submit_kyc_application` blocked when paused
- [x] `submit_investor_kyc` blocked when paused
- [x] `update_invoice_category` blocked when paused
- [x] `add_invoice_tag` / `remove_invoice_tag` blocked when paused
- [x] `create_dispute` blocked when paused
- [x] `fund_invoice` blocked when paused
- [x] `place_bid` blocked when paused
- [x] `process_partial_payment` blocked when paused
- [x] `create_vesting_schedule` / `release_vested_tokens` blocked when paused
- [x] `add_invoice_rating` blocked when paused
- [x] Admin config (`set_bid_ttl_days`, `add_currency`, `update_protocol_limits`) allowed during pause
- [x] KYC review (`verify_business`, `verify_investor`) allowed during pause
- [x] Emergency withdraw lifecycle works during pause
- [x] Admin rotation and unpause by new admin works during pause
- [x] All query functions work during pause
- [x] Pause/unpause cycles are deterministic and idempotent
- [x] Pause and maintenance mode are independent flags
- [x] No bypass via internal or indirect function calls

