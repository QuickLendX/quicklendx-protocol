# Operator Handbook

Every admin/operator-facing entrypoint with concrete CLI examples.

**Audience:** Protocol operators — the people who deploy, configure, monitor,
and incident-manage the QuickLendX Soroban contracts.

> **Conventions in this document:**
> - `$CONTRACT_ID` — deployed contract address (set via `export CONTRACT_ID=...`).
> - `--source admin` — the administrator keypair (pass `--source admin` or `--source /path/to/admin.json`).
> - `--network testnet` / `--network mainnet` — target network.
> - All examples use `soroban contract invoke`.

---

## Table of Contents

1. [Initialization & Protocol Config](#1-initialization--protocol-config)
2. [Admin Management](#2-admin-management)
3. [Pause, Maintenance & Incident Mode](#3-pause-maintenance--incident-mode)
4. [Emergency Withdrawal](#4-emergency-withdrawal)
5. [Currency Whitelist](#5-currency-whitelist)
6. [Protocol Limits](#6-protocol-limits)
7. [Invoice Operations](#7-invoice-operations)
8. [Bid Operations](#8-bid-operations)
9. [Escrow Operations](#9-escrow-operations)
10. [Dispute Resolution](#10-dispute-resolution)
11. [Fees & Revenue](#11-fees--revenue)
12. [KYC / Verification](#12-kyc--verification)
13. [Default Handling](#13-default-handling)
14. [Backup & Restore](#14-backup--restore)
15. [Upgrade Management](#15-upgrade-management)
16. [Vesting](#16-vesting)
17. [Maintenance & Indexing](#17-maintenance--indexing)
18. [Health & Monitoring](#18-health--monitoring)
19. [Audit Trail](#19-audit-trail)
20. [Diagnostics](#20-diagnostics)
21. [Common Workflows](#21-common-workflows)

---

## 1. Initialization & Protocol Config

One-time setup after deployment, then periodic configuration updates.

### Initialize the contract

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  initialize \
  --admin $ADMIN_ADDRESS \
  --treasury $TREASURY_ADDRESS \
  --fee_bps 150
```

### Check initialization status

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  is_initialized
```

### Get protocol version

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_version
```

### Update full protocol configuration (dry-run first)

```bash
# Preview what would change
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  preview_protocol_config \
  --min_invoice_amount 100 \
  --max_due_date_days 365 \
  --grace_period_seconds 86400

# Apply the config
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_protocol_config \
  --min_invoice_amount 100 \
  --max_due_date_days 365 \
  --grace_period_seconds 86400
```

### Read current protocol limits

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_protocol_limits
```

### Set treasury address

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_treasury \
  --treasury $NEW_TREASURY_ADDRESS
```

### Cancel pending treasury rotation

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  cancel_treasury_rotation
```

### Read treasury info

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_treasury

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_pending_treasury
```

### Run protocol heartbeat (invariant self-check)

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  invariant_self_check
```

---

## 2. Admin Management

Transfer admin role with one-step or two-step handover.

### One-step admin transfer

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  transfer_admin \
  --new_admin $NEW_ADMIN_ADDRESS
```

### Two-step admin transfer (initiate then accept)

```bash
# Step 1: current admin initiates
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  initiate_admin_transfer \
  --new_admin $NEW_ADMIN_ADDRESS

# Step 2: new admin accepts (run from new admin keypair)
soroban contract invoke \
  --id $CONTRACT_ID \
  --source new_admin \
  --network testnet \
  -- \
  accept_admin_transfer
```

### Enable / disable two-step transfers

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_two_step_enabled \
  --enabled true
```

### Read current admin

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_current_admin
```

---

## 3. Pause, Maintenance & Incident Mode

Circuit breakers for contract operations.

### Pause the entire contract

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  pause
```

### Unpause

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  unpause
```

### Check pause state

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  is_paused
```

### Check per-entrypoint pause

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  is_entrypoint_paused \
  --entrypoint settle_invoice
```

### Enter maintenance mode

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_maintenance_mode \
  --enabled true \
  --reason "Storage migration in progress"
```

### Exit maintenance mode

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_maintenance_mode \
  --enabled false \
  --reason ""
```

### Check maintenance state

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  is_maintenance_mode

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_maintenance_reason
```

### Enter incident mode (hard pause + maintenance)

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  enter_incident_mode \
  --reason "Suspected exploit - pausing all writes"
```

### Exit incident mode

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  exit_incident_mode
```

---

## 4. Emergency Withdrawal

Timelocked fund extraction for emergencies.

### Initiate emergency withdrawal

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  initiate_emergency_withdraw \
  --token $TOKEN_ADDRESS \
  --amount 1000000 \
  --target $SAFE_ADDRESS
```

### Check pending withdrawal

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_pending_emergency_withdraw
```

### Check execution readiness

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  can_exec_emergency
```

### Check timelock status

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  emg_time_until_unlock

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  emg_time_until_expire
```

### Execute emergency withdrawal (after timelock)

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  execute_emergency_withdraw
```

### Cancel emergency withdrawal

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  cancel_emergency_withdraw
```

---

## 5. Currency Whitelist

Manage which tokens are accepted by the protocol.

### Add a single currency

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  add_currency \
  --currency $USDC_ADDRESS
```

### Remove a currency

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  remove_currency \
  --currency $OLD_TOKEN_ADDRESS
```

### Batch add / remove currencies

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  add_currencies_batch \
  --currencies ["$TOKEN_A","$TOKEN_B"]

soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  remove_currencies_batch \
  --currencies ["$OLD_TOKEN"]
```

### Replace entire whitelist

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_currencies \
  --currencies ["$USDC_ADDRESS","$XLM_ADDRESS"]
```

### Clear whitelist

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  clear_currencies
```

### Read whitelist

```bash
# Full list
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_whitelisted_currencies

# Count
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  currency_count

# Paginated
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_whitelisted_currencies_paged \
  --offset 0 \
  --limit 10

# Check single currency
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  is_allowed_currency \
  --currency $USDC_ADDRESS
```

---

## 6. Protocol Limits

Adjustable numeric bounds for invoices, bids, and grace periods.

### Set limits (preserves bid settings)

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_protocol_limits \
  --min_invoice_amount 50 \
  --max_due_date_days 180 \
  --grace_period_seconds 172800
```

### Set limits with invoice cap

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  update_limits_max_invoices \
  --min_invoice_amount 50 \
  --max_due_date_days 180 \
  --grace_period_seconds 172800 \
  --max_invoices_per_business 500
```

### Full limits update

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_protocol_limits_full \
  --min_invoice_amount 50 \
  --min_bid_amount 10 \
  --min_fee_bps 50 \
  --max_due_date_days 180 \
  --grace_period_seconds 172800 \
  --max_invoices_per_business 500
```

### Update minimum bid amount

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  update_minimum_bid \
  --amount 25
```

### Read current limits

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_min_invoice_amount

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_max_due_date_days

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_grace_period_seconds

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_protocol_limits
```

### Bid TTL configuration

```bash
# Set bid TTL
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_bid_ttl_days \
  --days 14

# Reset to default
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  reset_bid_ttl_to_default

# Read config
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_bid_ttl_config
```

### Max active bids per investor

```bash
# Set limit
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_max_active_bids_per_investor \
  --limit 10

# Read limit
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_max_active_bids_per_investor

# Reset to default
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  reset_investor_bid_limit
```

---

## 7. Invoice Operations

Admin-level invoice management (verification, status updates, freeze/unfreeze, expiration).

### Verify an invoice

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  verify_invoice \
  --invoice_id $INVOICE_ID
```

### Update invoice status

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  update_invoice_status \
  --invoice_id $INVOICE_ID \
  --status verified
```

### Freeze an invoice

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  freeze_invoice \
  --invoice_id $INVOICE_ID \
  --reason "Regulatory hold"
```

### Unfreeze an invoice

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  unfreeze_invoice \
  --invoice_id $INVOICE_ID
```

### Check freeze status

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_invoice_freeze_info \
  --invoice_id $INVOICE_ID
```

### Expire an overdue invoice

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  expire_invoice \
  --invoice_id $INVOICE_ID
```

### Clear all invoices (destructive)

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  clear_all_invoices
```

### Read invoices

```bash
# Get single invoice
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_invoice \
  --invoice_id $INVOICE_ID

# Invoices by status
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_invoices_by_status \
  --status verified

# Available (verified, unfunded) invoices
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_available_invoices

# Count by status
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_invoice_count_by_status \
  --status funded

# Total count
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_total_invoice_count

# Business default history
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_business_default_history \
  --business $BUSINESS_ADDRESS
```

---

## 8. Bid Operations

Admin can clean expired bids and read bid state.

### Cleanup expired bids

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  cleanup_expired_bids \
  --invoice_id $INVOICE_ID

# Paginated cleanup for large bid sets
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  cleanup_expired_bids_paged \
  --invoice_id $INVOICE_ID \
  --offset 0 \
  --limit 20
```

### Read bid data

```bash
# Single bid
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_bid \
  --bid_id $BID_ID

# Best ranked bid for an invoice
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_best_bid \
  --invoice_id $INVOICE_ID

# All ranked bids
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_ranked_bids \
  --invoice_id $INVOICE_ID

# Bids by status
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_bids_by_status \
  --invoice_id $INVOICE_ID \
  --status active

# Paginated bid history
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_bid_history_paged \
  --invoice_id $INVOICE_ID \
  --status_filter active \
  --offset 0 \
  --limit 10

# All bids by an investor
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_all_bids_by_investor \
  --investor $INVESTOR_ADDRESS
```

---

## 9. Escrow Operations

Admin escrow management and early release controls.

### Refund escrow

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  refund_escrow_funds \
  --invoice_id $INVOICE_ID \
  --caller $ADMIN_ADDRESS
```

### Extend escrow expiry

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  extend_escrow_expiry \
  --invoice_id $INVOICE_ID \
  --new_due_date $NEW_DUE_DATE
```

### Read escrow data

```bash
# Escrow details
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_escrow_details \
  --invoice_id $INVOICE_ID

# Escrow status
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_escrow_status \
  --invoice_id $INVOICE_ID

# Admin escrow lookup
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  admin_get_escrow \
  --escrow_id $ESCROW_ID

# Total locked escrow across currencies
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_total_locked_escrow \
  --currencies ["$USDC_ADDRESS"] \
  --max_currencies 5
```

---

## 10. Dispute Resolution

Admin dispute review and resolution.

### Put dispute under review

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  put_dispute_under_review \
  --invoice_id $INVOICE_ID \
  --admin $ADMIN_ADDRESS
```

### Resolve dispute

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  resolve_dispute \
  --invoice_id $INVOICE_ID \
  --admin $ADMIN_ADDRESS \
  --resolution resolved_for_buyer
```

### Structured dispute resolution

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  resolve_dispute_structured \
  --invoice_id $INVOICE_ID \
  --admin $ADMIN_ADDRESS \
  --outcome resolved_for_seller \
  --note "Evidence supports seller claim"
```

### Read dispute data

```bash
# Dispute status
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_invoice_dispute_status \
  --invoice_id $INVOICE_ID

# Dispute details
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_dispute_details \
  --invoice_id $INVOICE_ID

# All invoices with active disputes
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_invoices_with_disputes

# Dispute timeline
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_dispute_timeline \
  --invoice_id $INVOICE_ID \
  --offset 0 \
  --limit 10
```

---

## 11. Fees & Revenue

Platform fee configuration and revenue distribution.

### Set platform fee

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_platform_fee \
  --new_fee_bps 250
```

### Read fee configuration

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_platform_fee

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_platform_fee_config

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_fee_bps
```

### Configure revenue distribution

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  configure_revenue_distribution \
  --platform_bps 7000 \
  --investor_bps 2000 \
  --developer_bps 1000
```

### Read revenue split config

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_revenue_split_config
```

### Distribute revenue

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  distribute_revenue \
  --period 2024-Q1
```

### Distribute with vesting

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  distribute_revenue_vested \
  --period 2024-Q1 \
  --developer $DEV_ADDRESS \
  --token $TOKEN_ADDRESS \
  --vesting_start 1704067200 \
  --vesting_cliff 7776000 \
  --vesting_end 1735689600
```

### Fee analytics

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_fee_analytics \
  --period 2024-Q1
```

---

## 12. KYC / Verification

Business and investor verification gates.

### Verify a business

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  verify_business \
  --admin $ADMIN_ADDRESS \
  --business $BUSINESS_ADDRESS
```

### Reject a business

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  reject_business \
  --admin $ADMIN_ADDRESS \
  --business $BUSINESS_ADDRESS \
  --reason "Incomplete documentation"
```

### Verify an investor

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  verify_investor \
  --investor $INVESTOR_ADDRESS \
  --investment_limit 50000
```

### Reject an investor

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  reject_investor \
  --investor $INVESTOR_ADDRESS \
  --reason "KYC documents expired"
```

### Revoke investor KYC

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  revoke_investor_kyc \
  --investor $INVESTOR_ADDRESS \
  --reason "Sanctions list match"
```

### Set investment limit

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_investment_limit \
  --investor $INVESTOR_ADDRESS \
  --new_limit 100000
```

### Recompute investor tier

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  recompute_investor_tier \
  --investor $INVESTOR_ADDRESS
```

### Read KYC state

```bash
# Business verification
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_business_verification_status \
  --business $BUSINESS_ADDRESS

# Investor verification
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_investor_verification \
  --investor $INVESTOR_ADDRESS

# List verified entities
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_verified_businesses

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_verified_investors

# List pending
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_pending_businesses

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_pending_investors

# Investors by tier
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_investors_by_tier \
  --tier gold
```

---

## 13. Default Handling

Invoice default detection and processing.

### Handle an invoice default

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  handle_default \
  --invoice_id $INVOICE_ID
```

### Mark invoice as defaulted

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  mark_invoice_defaulted \
  --invoice_id $INVOICE_ID \
  --grace_period 172800
```

### Scan for overdue invoices

```bash
# Standard scan
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  check_overdue_invoices

# Scan with custom grace period
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  check_overdue_invoices_grace \
  --grace_period 172800
```

### Read scan state

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_overdue_scan_cursor

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_overdue_scan_batch_limit

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_overdue_scan_batch_limit_max
```

---

## 14. Backup & Restore

On-chain state snapshots for disaster recovery.

### Create a backup

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  create_backup
```

### List backups

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_backups
```

### Get backup details

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_backup_details \
  --backup_id $BACKUP_ID
```

### Validate backup integrity

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  validate_backup \
  --backup_id $BACKUP_ID
```

### Restore from backup

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  restore_backup \
  --backup_id $BACKUP_ID
```

### Archive a backup

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  archive_backup \
  --backup_id $BACKUP_ID
```

### Cleanup old backups

```bash
# Preview what would be deleted
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  preview_cleanup_backups

# Execute cleanup
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  cleanup_backups
```

### Set retention policy

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_backup_retention_policy \
  --max_backups 10 \
  --max_age_days 90 \
  --auto_cleanup true

# Read current policy
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_backup_retention_policy
```

---

## 15. Upgrade Management

WASM contract upgrades with quiesce protocol.

### Schedule an upgrade

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  schedule_upgrade \
  --wasm_hash $NEW_WASM_HASH
```

### Execute a scheduled upgrade

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  execute_upgrade
```

### Cancel a pending upgrade

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  cancel_upgrade
```

### Extend storage TTLs before upgrade

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  extend_protocol_ttl
```

> See [docs/UPGRADE_QUIESCE.md](UPGRADE_QUIESCE.md) for the full drain-and-upgrade checklist.

---

## 16. Vesting

Token vesting schedule management.

### Create a vesting schedule

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  create_vesting_schedule \
  --token $TOKEN_ADDRESS \
  --beneficiary $BENEFICIARY_ADDRESS \
  --amount 1000000 \
  --start 1704067200 \
  --cliff 7776000 \
  --end 1735689600
```

### Read vesting state

```bash
# Schedule by ID
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_vesting_schedule \
  --id $VESTING_ID

# Vested amount so far
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_vesting_vested \
  --id $VESTING_ID

# Currently releasable
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_vesting_releasable \
  --id $VESTING_ID

# Full summary for a user
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_vesting_summary \
  --user $BENEFICIARY_ADDRESS
```

### Release vested tokens

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source beneficiary \
  --network testnet \
  -- \
  release_vested_tokens \
  --beneficiary $BENEFICIARY_ADDRESS \
  --id $VESTING_ID
```

---

## 17. Maintenance & Indexing

Secondary index rebuilds and data pruning.

### Rebuild invoice indexes

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  rebuild_invoice_indexes \
  --offset 0 \
  --limit 100
```

### Prune terminal invoices (irreversible)

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  prune_terminal_invoices \
  --older_than_secs 7776000 \
  --offset 0 \
  --limit 50
```

### Repair held escrow reserve

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  repair_held_escrow_reserve \
  --currency $USDC_ADDRESS \
  --offset 0 \
  --limit 50
```

---

## 18. Health & Monitoring

Operational health checks and protocol metrics.

### Health status

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_health_status
```

### Protocol health

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_protocol_health
```

### Operational limits

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_operational_limits
```

### Platform metrics

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_platform_metrics
```

### Performance metrics

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_performance_metrics
```

### Analytics summary

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_analytics_summary
```

### Export analytics snapshot

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  export_analytics_snapshot
```

### Cross-role address summary

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_address_summary \
  --addr $ADDRESS
```

---

## 19. Audit Trail

On-chain audit log queries for compliance and debugging.

### Invoice audit trail

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_invoice_audit_trail \
  --invoice_id $INVOICE_ID
```

### Single audit entry

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_audit_entry \
  --audit_id $AUDIT_ID
```

### Query by operation or actor

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_audit_entries_by_operation \
  --operation invoice_verified

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_audit_entries_by_actor \
  --actor $ADMIN_ADDRESS
```

### Query audit logs with filter

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  query_audit_logs \
  --filter "invoice_verified" \
  --limit 20
```

### Audit statistics

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_audit_stats
```

### Verify audit chain integrity

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  validate_invoice_audit_integrity \
  --invoice_id $INVOICE_ID

soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  verify_audit_chain \
  --invoice_id $INVOICE_ID

# Find first divergence point
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  first_audit_chain_divergence \
  --invoice_id $INVOICE_ID
```

---

## 20. Diagnostics

Rich diagnostic snapshot (feature-gated: `diagnostics`).

```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_protocol_diagnostics
```

---

## 21. Common Workflows

### New deployment checklist

```bash
# 1. Build WASM
cd quicklendx-contracts
cargo build --target wasm32-unknown-unknown --release

# 2. Deploy contract
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm \
  --source admin \
  --network testnet

# 3. Initialize
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  initialize \
  --admin $ADMIN_ADDRESS \
  --treasury $TREASURY_ADDRESS \
  --fee_bps 150

# 4. Set protocol limits
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_protocol_limits \
  --min_invoice_amount 50 \
  --max_due_date_days 365 \
  --grace_period_seconds 172800

# 5. Whitelist currencies
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  add_currency \
  --currency $USDC_ADDRESS

# 6. Run heartbeat
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  invariant_self_check
```

### Emergency incident response

```bash
# 1. Enter incident mode (pauses all writes + maintenance)
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  enter_incident_mode \
  --reason "Suspected exploit"

# 2. Create backup before any changes
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  create_backup

# 3. Investigate (read-only)
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_health_status

# 4. If funds at risk, initiate emergency withdrawal
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  initiate_emergency_withdraw \
  --token $TOKEN_ADDRESS \
  --amount $AT_RISK_AMOUNT \
  --target $SAFE_ADDRESS

# 5. After investigation, exit incident mode
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  exit_incident_mode
```

### Pre-upgrade checklist

```bash
# 1. Create backup
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  create_backup

# 2. Extend TTLs
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  extend_protocol_ttl

# 3. Enter maintenance (drain writes)
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_maintenance_mode \
  --enabled true \
  --reason "Pre-upgrade drain"

# 4. Build new WASM
cargo build --target wasm32-unknown-unknown --release --profile release

# 5. Schedule upgrade
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  schedule_upgrade \
  --wasm_hash $NEW_WASM_HASH

# 6. Execute upgrade
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  execute_upgrade

# 7. Exit maintenance
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_maintenance_mode \
  --enabled false \
  --reason ""
```

### Daily health check

```bash
# Run heartbeat / invariant self-check
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  invariant_self_check

# Check protocol health
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  get_protocol_health

# Scan for overdue invoices
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  check_overdue_invoices

# Clean expired bids
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  cleanup_expired_bids \
  --invoice_id $INVOICE_ID
```
