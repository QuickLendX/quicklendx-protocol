# QuickLendX Authorization Matrix

This matrix is for contributors and reviewers auditing the exported Soroban
contract surface in `quicklendx-contracts/src/lib.rs`.

Scope:

- Public `QuickLendXContract` entrypoints exported by the `#[contractimpl]`
  blocks in `quicklendx-contracts/src/lib.rs`.
- Helper-module requirements where the wrapper delegates authorization to
  `admin.rs`, `verification.rs`, `currency.rs`, `fees.rs`, `escrow.rs`,
  `protocol_limits.rs`, `pause.rs`, `maintenance.rs`, and `emergency.rs`.
- Commented-out functions in `lib.rs` are intentionally omitted.

## Role Legend

| Role or gate | Meaning |
| --- | --- |
| Admin signer | The stored protocol/admin address must authorize the call with `require_auth()`. |
| Admin identity check | The supplied address is compared with stored admin state, but the checked helper does not itself call `require_auth()`. Treat this as current-source behavior, not a recommended pattern. |
| Business signer | The invoice business or submitted business address must authorize the call. |
| Investor signer | The investor or investment beneficiary must authorize the call. |
| Caller signer | The supplied caller/creator/user address must authorize the call; extra role checks may apply. |
| State gate | No signer role, but the entrypoint is constrained by invoice, bid, escrow, timelock, pause, maintenance, or pagination state. |
| Public read | No signer required and no state mutation intended. |
| Public/internal write | Exported write-style helper with no signer in the current wrapper. Keep it isolated or harden it in a separate change. |

Most mutating entrypoints also call `pause::PauseControl::require_not_paused`
before state changes. The matrix calls that out as `Pause gate` where it is
part of the top-level wrapper.

## Entry Point Matrix

| Area | Entrypoints | Required role or signer | Additional gates and notes |
| --- | --- | --- | --- |
| Bootstrap | `initialize`, `initialize_admin` | Admin signer | One-time setup. `initialize` delegates to `ProtocolInitializer::initialize`; `initialize_admin` delegates to `AdminStorage::initialize`. |
| Admin transfer | `transfer_admin` | Current admin signer | Current admin is loaded from storage and signs inside `AdminStorage::transfer_admin`. |
| Admin read | `get_current_admin`, `get_admin` | Public read | `get_current_admin` reads `AdminStorage`; `get_admin` reads the legacy business-verification admin. |
| Legacy admin setter | `set_admin` | Current admin signer if one exists; new admin signer on first setup | Uses `BusinessVerificationStorage`, not `AdminStorage`. |
| Protocol config | `set_protocol_config`, `set_fee_config`, `preview_protocol_config`, `set_treasury`, `update_minimum_bid`, `initialize_protocol_limits`, `set_protocol_limits`, `update_protocol_limits`, `update_limits_max_invoices` | Admin signer | Bounds are enforced by `init` or `protocol_limits` helpers. Most wrappers are pause-gated when mutating live limits. |
| Protocol config reads | `is_initialized`, `get_version`, `get_protocol_limits`, `get_fee_bps`, `get_treasury`, `get_min_invoice_amount`, `get_max_due_date_days`, `get_grace_period_seconds`, `get_operational_limits` | Public read | Read-only snapshots or constants. |
| Bid configuration | `set_bid_ttl_days`, `reset_bid_ttl_to_default`, `set_max_active_bids_per_investor` | Admin signer | Wrapper derives current admin from storage; `BidStorage` helpers call `admin.require_auth()` and `AdminStorage::require_admin`. |
| Bid configuration reads | `get_bid_ttl_days`, `get_bid_ttl_config`, `get_max_active_bids_per_investor`, `get_bid_limit_config` | Public read | Read-only configuration snapshots. |
| Emergency withdrawal | `initiate_emergency_withdraw`, `execute_emergency_withdraw`, `cancel_emergency_withdraw` | Admin signer | Timelock, expiration, nonce, reserve-completeness, and payment reentrancy guards apply. |
| Emergency withdrawal reads | `get_pending_emergency_withdraw`, `can_exec_emergency`, `emg_time_until_unlock`, `emg_time_until_expire` | Public read | Read pending withdrawal state and derived timing. |
| Pause and incident mode | `pause`, `unpause`, `set_maintenance_mode`, `enter_incident_mode`, `exit_incident_mode`, `extend_protocol_ttl`, `invariant_self_check` | Admin signer | Incident mode composes pause and maintenance. `extend_protocol_ttl` and invariant checks are operational admin calls. |
| Pause and health reads | `is_paused`, `is_entrypoint_paused`, `is_maintenance_mode`, `get_maintenance_reason`, `get_health_status`, `get_protocol_health`, `get_protocol_diagnostics` | Public read | Diagnostics is declared in later `#[contractimpl]` blocks and is listed once here. |
| Currency whitelist writes with signer | `remove_currency`, `remove_currencies_batch`, `clear_currencies` | Admin signer | Pause-gated. Helpers compare admin and call `admin.require_auth()`. |
| Currency whitelist identity-only writes | `add_currency`, `add_currencies_batch`, `set_currencies` | Admin identity check | Pause-gated. Current helpers call `AdminStorage::require_admin` but do not call `admin.require_auth()` before writing. |
| Currency whitelist reads | `is_allowed_currency`, `get_whitelisted_currencies`, `currency_count`, `get_whitelisted_currencies_paged` | Public read | Empty whitelist means allow-all for backward compatibility. |
| Invoice creation | `upload_invoice` | Business signer | Pause-gated. Requires business KYC not pending, invoice validation, currency whitelist, tag/category validation, and business active-invoice limit. |
| Legacy invoice creation | `store_invoice` | Public/internal write | Pause-gated and validates invoice fields/currency, but the source comments mark it unauthenticated; prefer `upload_invoice` for business flow. |
| Invoice admin/status writes | `verify_invoice`, `handle_default`, `mark_invoice_defaulted` | Admin signer | Pause-gated. `verify_invoice` can release escrow when a funded invoice is verified. |
| Invoice status identity-only write | `update_invoice_status` | Admin identity check | Pause-gated. Loads stored admin but does not require that admin's signature in the wrapper before state changes. |
| Invoice business writes | `cancel_invoice`, `update_invoice_metadata`, `clear_invoice_metadata`, `update_invoice_category`, `add_invoice_tag`, `remove_invoice_tag` | Business signer | Pause-gated. The stored invoice business signs; some flows also reject pending KYC or validate metadata/category/tag bounds. |
| Invoice public maintenance writes | `expire_invoice`, `check_invoice_expiration`, `check_overdue_invoices`, `check_overdue_invoices_grace`, `handle_overdue_invoices` | State gate | Pause-gated where the wrapper mutates invoice state. These are bounded default/expiration maintenance flows. |
| Invoice public/internal write | `clear_all_invoices` | Public/internal write | Pause-gated but no admin signer or admin identity check in the current wrapper. It is intended for restore operations and should stay isolated or be hardened separately. |
| Invoice reads and search | `get_invoice`, `get_invoice_by_business`, `get_business_invoices`, `get_invoices_by_customer`, `get_invoices_by_tax_id`, `search_invoices`, `get_invoices_by_status`, `get_available_invoices`, `get_invoice_count_by_status`, `get_total_invoice_count`, `get_category_breakdown`, `get_invoices_by_tag`, `get_invoices_by_tags`, `get_invoice_count_by_category`, `get_invoice_count_by_tag`, `get_invoice_tags`, `invoice_has_tag`, `get_business_invoices_paged`, `get_available_invoices_paged` | Public read | Pagination and query-limit guards apply to paged endpoints. |
| Bid lifecycle writes | `place_bid`, `withdraw_bid`, `cancel_bid` | Investor signer | Pause-gated. `place_bid` requires verified investor status, bid limits, invoice verified status, and currency whitelist. `cancel_bid` delegates to `BidStorage::cancel_bid`, which signs the stored bid investor. |
| Business bid acceptance | `accept_bid`, `accept_bid_and_fund` | Business signer | Pause-gated and payment reentrancy guarded. The stored invoice business signs; bid/invoice status and one-escrow-per-invoice guards apply. |
| Bid cleanup writes | `cleanup_expired_bids`, `cleanup_expired_bids_paged`, `clean_expired_bids` | State gate | Permissionless cleanup of expired bids with bounded or paged work. |
| Bid reads | `get_bid`, `get_best_bid`, `get_ranked_bids`, `get_bids_by_status`, `get_bids_by_investor`, `get_bids_for_invoice`, `get_all_bids_by_investor`, `get_bid_history_paged`, `get_investor_bids_paged`, `get_bid_history` | Public read | Read-only bid indexes, with pagination guards on paged endpoints. |
| Investment/escrow investor writes | `add_investment_insurance`, `withdraw_investment` | Investor signer | Pause-gated. The stored investment investor signs; escrow and investment state gates apply. |
| Investment settlement/payment writes | `settle_invoice`, `process_partial_payment`, `make_payment`, `release_escrow_funds` | State gate | Pause-gated and payment reentrancy guarded. No caller signer is checked in these wrappers; invoice/escrow state controls the transition. |
| Escrow refund writes | `refund_escrow_funds` | Caller signer | Pause-gated and payment reentrancy guarded. Caller must sign and be either current admin or the invoice business. |
| Escrow refund alias | `refund_escrow` | Admin signer | Pause-gated and payment reentrancy guarded. Wrapper loads current admin and delegates to the signed refund helper. |
| Escrow admin read | `admin_get_escrow` | Admin signer | Uses `AdminStorage::require_admin_auth`; read-only support inspection. |
| Investment and escrow reads | `get_invoice_investment`, `get_investment`, `get_active_investment_ids`, `validate_no_orphan_investments`, `query_investment_insurance`, `get_escrow_details`, `get_escrow_status`, `get_investments_by_investor`, `get_investor_investments_paged`, `get_investor_portfolio_summary`, `get_address_summary`, `get_total_locked_escrow` | Public read | Query and integrity-audit helpers. `get_total_locked_escrow` bounds aggregation by caller-supplied `max_currencies`. |
| Business KYC self-service | `submit_kyc_application` | Business signer | Pause-gated through wrapper; helper signs submitted business and stores pending KYC. |
| Investor KYC self-service | `submit_investor_kyc` | Investor signer | Pause-gated through wrapper; helper signs submitted investor and stores pending KYC. |
| Business KYC admin writes | `verify_business`, `reject_business` | Admin signer | Pause-gated. Verification helpers call `admin.require_auth()`. |
| Investor KYC admin writes | `verify_investor`, `reject_investor`, `revoke_investor_kyc`, `set_investment_limit`, `recompute_investor_tier` | Admin signer | Pause-gated. Verification helpers require admin auth and update verification/tier state. |
| Verification reads | `get_verified_businesses`, `get_business_verification_status`, `get_pending_businesses`, `get_rejected_businesses`, `get_verified_investors`, `get_pending_investors`, `get_rejected_investors`, `get_investor_verification`, `get_investor_analytics`, `get_investors_by_tier`, `get_investors_by_risk_level`, `calculate_investor_risk_score`, `compute_investor_tier`, `calculate_investment_limit`, `validate_investor_investment`, `is_investor_verified` | Public read | Some functions perform validation calculations and may return errors, but do not require a signer. |
| Exported analytics helper write | `update_investor_analytics` | Public/internal write | Exposed helper used by tests/internal accounting; no signer is checked in the top-level wrapper. |
| Fee system writes with signer | `initialize_fee_system`, `configure_treasury`, `update_platform_fee_bps`, `update_fee_structure`, `configure_revenue_distribution`, `distribute_revenue`, `distribute_revenue_vested` | Admin signer | Fee helpers call `admin.require_auth()`; some wrappers use the legacy business-verification admin source before delegating. |
| Fee system public/internal writes | `update_user_transaction_volume`, `collect_transaction_fees` | Public/internal write | Exported accounting helpers update fee/volume state without a caller signature in the wrapper. |
| Fee reads and calculators | `get_platform_fee`, `get_platform_fee_config`, `get_treasury_address`, `get_fee_structure`, `calculate_transaction_fees`, `get_user_volume_data`, `get_revenue_split_config`, `get_fee_analytics`, `validate_fee_parameters`, `calculate_profit`, `get_financial_metrics`, `get_analytics_summary` | Public read | Read-only fee config, fee math, and analytics calculations. |
| Backup admin writes | `create_backup`, `restore_backup`, `archive_backup`, `cleanup_backups`, `set_backup_retention_policy` | Admin identity check | Pause-gated, but these wrappers call `AdminStorage::require_admin` without `admin.require_auth()`. |
| Backup reads | `validate_backup`, `get_backup_details`, `get_backups`, `preview_cleanup_backups`, `get_backup_retention_policy` | Public read | Backup validation, preview, and retention snapshots. |
| Vesting admin writes | `create_vesting_schedule` | Admin signer | Pause-gated and payment reentrancy guarded; `Vesting::create_schedule` requires admin auth. |
| Vesting beneficiary writes | `release_vested_tokens` | Beneficiary signer | Pause-gated and payment reentrancy guarded. |
| Vesting revenue write | `distribute_revenue_vested` | Admin signer | Pause-gated and payment reentrancy guarded; distributes revenue and creates a developer vesting schedule when applicable. |
| Vesting reads | `get_vesting_schedule`, `get_vesting_vested`, `get_vesting_releasable`, `get_vesting_summary` | Public read | Read-only vesting schedule and summary helpers. |
| Rating write | `add_invoice_rating` | Public/internal write | Pause-gated but does not require the supplied `rater` to sign in the current wrapper. |
| Analytics report writes | `generate_business_report`, `generate_investor_report` | Public/internal write | Generate/store report state without caller signatures. |
| Analytics reads | `get_user_behavior_metrics`, `get_platform_metrics`, `export_analytics_snapshot`, `get_performance_metrics`, `get_business_report`, `get_investor_report` | Public read | Snapshot/report reads; `export_analytics_snapshot` composes platform/performance metrics without writes. |
| Dispute participant writes | `create_dispute`, `update_dispute_evidence` | Caller signer | Pause-gated. `create_dispute` signs the supplied creator; `update_dispute_evidence` requires the original dispute creator. |
| Dispute admin writes | `put_dispute_under_review`, `resolve_dispute`, `resolve_dispute_structured` | Admin identity check | Current wrappers call `AdminStorage::require_admin` but do not call `admin.require_auth()`. |
| Dispute reads | `get_invoice_dispute_status`, `get_dispute_details`, `get_invoices_with_disputes`, `get_dispute_timeline`, `get_invoices_by_dispute_status` | Public read | Read-only dispute status and timeline views. |
| Audit reads | `get_invoice_audit_trail`, `get_audit_entry`, `get_audit_entries_by_operation`, `get_audit_entries_by_actor`, `query_audit_logs`, `get_audit_stats`, `validate_invoice_audit_integrity`, `verify_audit_chain`, `first_audit_chain_divergence` | Public read | Audit log and hash-chain inspection. |
| Notification user write | `update_notification_preferences` | User signer | Supplied user signs before preferences are updated. |
| Notification public/internal write | `update_notification_status` | Public/internal write | Status update is exported without a caller signature in the wrapper. |
| Notification reads | `get_notification`, `get_user_notifications`, `get_notification_preferences`, `get_user_notification_stats`, `get_notification_unread_count` | Public read | Notification body, preference, and count queries. |
| Freshness read | `get_freshness` | Public read | Rejects `indexed_ledger_seq == 0`; otherwise returns bounded freshness metadata. |
| Admin recovery writes | `rebuild_invoice_indexes`, `prune_terminal_invoices`, `repair_held_escrow_reserve` | Admin signer | Explicit `admin.require_auth()` followed by `AdminStorage::require_admin`; operations are paginated and bounded. |

## Reviewer Notes

This file documents the current exported behavior. It intentionally does not
change authorization code.

Items worth separate hardening review:

- `clear_all_invoices` is exported, mutates storage, and has no admin signer
  check in the current wrapper.
- `add_currency`, `add_currencies_batch`, and `set_currencies` perform admin
  identity checks but do not require the admin signature in the helper.
- Backup writes and dispute admin writes use `AdminStorage::require_admin`
  without `require_auth()` in their top-level wrappers.
- Several internal-style accounting/reporting helpers are exported without
  signer checks: `update_investor_analytics`, `update_user_transaction_volume`,
  `collect_transaction_fees`, `add_invoice_rating`, `generate_business_report`,
  `generate_investor_report`, and `update_notification_status`.

If any of these are intended to remain externally callable, add explicit tests
for that contract. If not, harden them in a focused follow-up PR instead of
mixing behavior changes into this documentation matrix.

## Refresh Checklist

When adding a new contract entrypoint:

1. Add it to the matching row above, or create a new row if it introduces a new
   role pattern.
2. Record whether the code enforces a signer with `require_auth()` or only an
   identity/state check.
3. Link the new entrypoint to related docs such as
   `docs/contracts/access-control.md`, `docs/GOVERNANCE.md`,
   `docs/CURRENCY_WHITELIST.md`, or `docs/INVOICE_LIFECYCLE.md`.
4. Add or update role tests for any mutating entrypoint.
