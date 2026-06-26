# Authorization Matrix

This document maps the contract entrypoints in `quicklendx-contracts/src/lib.rs`
to the role or signer that must authorize each operation. It is written for
contributors reviewing access-control changes before opening a PR.

## Role Model

| Role | How it is proven in code | Typical responsibility |
| --- | --- | --- |
| Admin | `AdminStorage::require_admin`, `AdminStorage::require_admin_auth`, `AdminStorage::with_admin_auth`, or `admin.require_auth()` after loading the stored admin | Protocol configuration, KYC decisions, emergency operations, maintenance, backups, disputes, and support-only inspection |
| Business | `business.require_auth()` or `invoice.business.require_auth()` | Upload and maintain invoices, accept bids, cancel owned invoices, manage invoice metadata, and trigger business-side actions |
| Investor | `investor.require_auth()` or `investment.investor.require_auth()` | Submit investor KYC, place or withdraw bids, manage active investments, and release vested tokens when the investor is the beneficiary |
| Dispute creator | `creator.require_auth()` plus creator matching on the stored dispute | Open a dispute and update evidence before admin review |
| Public reader | No signer required; state is read only | Query protocol state, indexes, analytics, reports, escrow status, health, and derived values |
| Public maintenance caller | No signer required, but the call is bounded and cannot bypass business/admin/investor ownership checks | Time-based cleanup, expiration, default, settlement, and reconciliation helpers |

Most mutating entrypoints also call `pause::PauseControl::require_not_paused`.
When the protocol is paused, those entrypoints reject before changing state.
Read-only getters remain callable while paused unless their implementation
documents a narrower rule.

## Admin-Only Entrypoints

These calls require the stored protocol admin, either directly through an
`admin` argument or by loading the current admin from storage.

| Area | Entrypoints | Authorization source |
| --- | --- | --- |
| Protocol initialization and configuration | `initialize`, `initialize_admin`, `transfer_admin`, `set_protocol_config`, `set_fee_config`, `preview_protocol_config`, `set_treasury`, `set_bid_ttl_days`, `reset_bid_ttl_to_default`, `set_max_active_bids_per_investor`, `set_protocol_limits`, `update_protocol_limits`, `update_limits_max_invoices` | `init::ProtocolInitializer`, `AdminStorage`, or stored admin + `require_auth` |
| Currency whitelist | `add_currency`, `remove_currency`, `add_currencies_batch`, `remove_currencies_batch`, `set_currencies`, `clear_currencies` | `currency::CurrencyWhitelist` + admin auth |
| Emergency and incident controls | `initiate_emergency_withdraw`, `execute_emergency_withdraw`, `cancel_emergency_withdraw`, `pause`, `unpause`, `enter_incident_mode`, `exit_incident_mode`, `extend_protocol_ttl`, `invariant_self_check` | `EmergencyWithdraw`, `PauseControl`, `MaintenanceControl`, `IncidentMode`, or invariant helpers with admin auth |
| Verification decisions | `verify_business`, `reject_business`, `verify_investor`, `reject_investor`, `set_investment_limit`, `recompute_investor_tier`, `verify_invoice` | Stored admin + `admin.require_auth()` |
| Invoice lifecycle administration | `handle_default`, `mark_invoice_defaulted`, `clear_all_invoices`, `admin_get_escrow`, `rebuild_invoice_indexes`, `prune_terminal_invoices`, `repair_held_escrow_reserve` | Stored admin or `AdminStorage::require_admin_auth` |
| Backup and retention | `create_backup`, `restore_backup`, `archive_backup`, `cleanup_backups`, `set_backup_retention_policy` | `AdminStorage::require_admin` |
| Vesting and revenue controls | `create_vesting_schedule`, `distribute_revenue_vested`, `initialize_fee_system`, `configure_treasury`, `update_platform_fee_bps`, `update_fee_structure`, `configure_revenue_distribution`, `distribute_revenue`, `collect_transaction_fees`, `update_user_transaction_volume` | Admin-gated fee or vesting helpers |
| Dispute moderation | `put_dispute_under_review`, `resolve_dispute`, `resolve_dispute_structured` | `AdminStorage::require_admin` |

## Business-Owned Entrypoints

These calls require the business address carried by the request or loaded from
the invoice. Business KYC state is checked where the lifecycle requires it.

| Entrypoint | Required signer | Notes |
| --- | --- | --- |
| `upload_invoice` | `business` | Calls `business.require_auth()` and rejects pending KYC via `require_business_not_pending`. |
| `store_invoice` | None in the current implementation | This lower-level storage path validates invoice data and rejects self-call addresses, but it does not call `business.require_auth()`. Contributors should treat it separately from the user-facing upload flow. |
| `cancel_invoice` | `invoice.business` | Only the invoice owner can cancel before funding. |
| `update_invoice_metadata`, `clear_invoice_metadata`, `update_invoice_category`, `add_invoice_tag`, `remove_invoice_tag` | `invoice.business` | Metadata and taxonomy updates are owned by the invoice business. |
| `accept_bid`, `accept_bid_and_fund` | `invoice.business` through the escrow/bid flow | Business accepts a placed bid for its invoice; payment paths also use the reentrancy guard. |
| `refund_escrow_funds` | Admin or invoice business | Delegates to escrow refund logic with the caller address. |
| `generate_business_report` | Public in the exported wrapper | Generates and stores a report for the supplied business; no signer check is visible in the wrapper. |

## Investor-Owned Entrypoints

These calls require the investor address or the investor stored on an investment.

| Entrypoint | Required signer | Notes |
| --- | --- | --- |
| `submit_investor_kyc` | `investor` | Calls `investor.require_auth()`. |
| `place_bid` | `investor` | Calls `investor.require_auth()`, rejects self-call addresses, and requires verified investor status and capacity. |
| `cancel_bid`, `withdraw_bid` | Bid investor | Uses the investor stored on the bid. |
| `withdraw_investment` | `investor` | Delegates to escrow withdrawal logic for the investor. |
| `add_investment_insurance` | `investment.investor` | The investment owner authorizes the insurance update. |
| `release_vested_tokens` | `beneficiary` | The vesting beneficiary must authorize token release. |
| `update_notification_preferences` | `user` | Notification preference updates require the user signer. |
| `generate_investor_report` | Public in the exported wrapper | Generates a report for the supplied investor; no signer check is visible in the wrapper. |

## Verification and KYC Entrypoints

| Entrypoint | Role | Authorization |
| --- | --- | --- |
| `submit_kyc_application` | Business | `business.require_auth()` |
| `verify_business`, `reject_business` | Admin | `admin.require_auth()` plus admin validation |
| `submit_investor_kyc` | Investor | `investor.require_auth()` |
| `verify_investor`, `reject_investor` | Admin | `admin.require_auth()` plus admin validation |
| `is_business_verified`, `get_business_verification_status`, `get_pending_businesses`, `get_rejected_businesses`, `is_investor_verified`, `get_verified_investors`, `get_pending_investors`, `get_rejected_investors`, `get_investors_by_tier`, `get_investors_by_risk_level`, `calculate_investor_risk_score`, `compute_investor_tier`, `calculate_investment_limit`, `validate_investor_investment`, `get_investor_analytics` | Public reader | Read-only or pure calculation wrappers |

## Public Read-Only Entrypoints

These entrypoints expose state or derived values without requiring a signer.

| Area | Entrypoints |
| --- | --- |
| Protocol config and health | `is_initialized`, `get_version`, `get_protocol_limits`, `get_current_admin`, `get_fee_bps`, `get_treasury`, `get_min_invoice_amount`, `get_max_due_date_days`, `get_grace_period_seconds`, `get_bid_ttl_days`, `get_bid_ttl_config`, `get_max_active_bids_per_investor`, `get_bid_limit_config`, `is_paused`, `is_maintenance_mode`, `get_maintenance_reason`, `get_health_status`, `get_protocol_health`, `get_protocol_diagnostics`, `get_freshness` |
| Currency and indexes | `is_allowed_currency`, `get_whitelisted_currencies`, `currency_count`, `get_whitelisted_currencies_paged` |
| Invoice queries | `get_invoice`, `get_invoice_by_business`, `get_business_invoices`, `get_invoices_by_customer`, `get_invoices_by_tax_id`, `search_invoices`, `get_invoices_by_status`, `get_available_invoices`, `get_invoice_count_by_status`, `get_total_invoice_count`, `get_category_breakdown`, `get_invoices_by_tag`, `get_invoices_by_tags`, `get_invoice_count_by_category`, `get_invoice_count_by_tag`, `get_invoice_tags`, `invoice_has_tag`, `get_business_invoices_paged`, `get_available_invoices_paged` |
| Bid and investment queries | `get_bid`, `get_best_bid`, `get_ranked_bids`, `get_bids_by_status`, `get_bids_by_investor`, `get_bids_for_invoice`, `get_all_bids_by_investor`, `get_invoice_investment`, `get_investment`, `get_active_investment_ids`, `validate_no_orphan_investments`, `query_investment_insurance`, `get_investments_by_investor`, `get_investor_portfolio_summary`, `get_bid_history`, `get_bid_history_paged`, `get_investor_bids_paged` |
| Escrow, backup, vesting, analytics | `get_escrow_details`, `get_escrow_status`, `validate_backup`, `get_backup_details`, `get_backups`, `get_backup_retention_policy`, `get_vesting_schedule`, `get_vesting_vested`, `get_vesting_releasable`, `get_vesting_summary`, `get_user_behavior_metrics`, `get_platform_metrics`, `export_analytics_snapshot`, `get_performance_metrics`, `get_business_report`, `get_investor_report`, `get_fee_structure`, `calculate_transaction_fees`, `get_user_volume_data`, `get_revenue_split_config`, `get_fee_analytics`, `validate_fee_parameters` |
| Dispute, audit, notification, freshness | `get_invoice_dispute_status`, `get_dispute_details`, `get_invoices_with_disputes`, `get_dispute_timeline`, `get_invoices_by_dispute_status`, `get_invoice_audit_trail`, `get_audit_entry`, `get_audit_entries_by_operation`, `get_audit_entries_by_actor`, `query_audit_logs`, `get_audit_stats`, `validate_invoice_audit_integrity`, `verify_audit_chain`, `first_audit_chain_divergence`, `get_notification`, `get_user_notifications`, `get_notification_preferences`, `get_user_notification_stats` |

## Public or Automated Maintenance Entrypoints

The following calls are callable without an explicit signer in the exported
wrapper, but they are bounded and operate only on protocol state that already
satisfies the lifecycle checks in their implementation.

| Entrypoint | Guardrail |
| --- | --- |
| `settle_invoice`, `process_partial_payment`, `make_payment`, `release_escrow_funds` | Payment reentrancy guard plus invoice/escrow state validation |
| `expire_invoice`, `clean_expired_bids`, `cleanup_expired_bids`, `cleanup_expired_bids_paged`, `check_overdue_invoices`, `check_overdue_invoices_grace`, `handle_overdue_invoices`, `check_invoice_expiration` | Timestamp, status, pagination, and protocol-limit checks |
| `update_invoice_status` | Status transition validation in invoice storage logic |
| `add_invoice_rating` | Invoice existence and rating validation |

## Contributor Checklist

When adding or modifying an entrypoint:

1. Identify the expected role before changing storage.
2. Add the matching signer check (`require_auth`, `AdminStorage::*`, or owner
   comparison) before the first mutation.
3. Call `pause::PauseControl::require_not_paused` for mutating operations unless
   the entrypoint is intentionally available during incident mode.
4. Use `require_not_self` before trusting user-controlled `Address` values that
   later call `require_auth`.
5. Keep read-only getters signer-free unless they expose private support data;
   support-only inspection should use an explicit admin-gated wrapper.
6. Update this matrix and the relevant tests whenever authorization behavior
   changes.
