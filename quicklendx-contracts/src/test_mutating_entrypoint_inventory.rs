//! Exhaustive inventory guard for public contract entrypoints.
//!
//! Every public method on QuickLendXContract must be classified as either a
//! state-mutating entrypoint or a read-only/query entrypoint. This fails when a
//! new public method is added without updating the inventory. Runtime event
//! and audit parity is covered by the existing domain-specific parity suites.

#![cfg(test)]

use std::collections::BTreeSet;

const CONTRACT_SOURCE: &str = include_str!("contract.rs");

const MUTATING_ENTRYPOINTS: &[&str] = &[
    "initialize",
    "initialize_protocol_limits",
    "set_fee_config",
    "set_treasury",
    "transfer_admin",
    "initiate_admin_transfer",
    "accept_admin_transfer",
    "cancel_admin_transfer",
    "set_two_step_enabled",
    "store_invoice",
    "upload_invoice",
    "update_invoice_status",
    "verify_invoice",
    "place_bid",
    "accept_bid_and_fund",
    "accept_bid_and_fund_with_key",
    "accept_bid",
    "withdraw_bid",
    "cleanup_expired_bids",
    "submit_kyc_application",
    "freeze_invoice",
    "set_invoice_lock",
    "verify_business",
    "delete_business",
    "submit_investor_kyc",
    "verify_investor",
    "reject_business",
    "reject_investor",
    "update_invoice_metadata",
    "cancel_invoice",
    "complete_invoice",
    "clear_invoice_metadata",
    "refund_escrow_funds",
    "withdraw_investment",
    "approve_early_escrow_release",
    "release_escrow_funds",
    "create_backup",
    "restore_backup",
    "set_backup_retention_policy",
    "archive_backup",
    "add_invoice_tag",
    "remove_invoice_tag",
    "update_invoice_category",
];

const READ_ONLY_ENTRYPOINTS: &[&str] = &[
    "is_initialized",
    "get_version",
    "get_protocol_config",
    "preview_protocol_config",
    "get_fee_bps",
    "get_treasury",
    "get_min_invoice_amount",
    "get_max_due_date_days",
    "get_grace_period_seconds",
    "get_corridors",
    "get_current_admin",
    "is_two_step_enabled",
    "get_pending_admin",
    "get_invoice",
    "get_bid",
    "get_bids_for_invoice",
    "get_ranked_bids",
    "get_ranked_bids_paged",
    "get_ranked_bids_offset",
    "get_best_bid",
    "get_bids_by_status",
    "get_bids_by_investor",
    "get_investor_active_exposure",
    "get_investor_available_capacity",
    "validate_funding_commitment",
    "get_invoice_lock",
    "get_businesses_paged",
    "get_investors_paged",
    "get_investor_investments_paged",
    "get_available_invoices",
    "get_business_invoices",
    "get_business_invoices_paged",
    "get_total_invoice_count",
    "get_invoice_count_by_status",
    "get_invoices_by_customer",
    "get_invoices_by_tax_id",
    "get_invoices_by_status_batch",
    "get_escrow_details",
    "get_escrow_status",
    "get_investment",
    "get_invoice_investment",
    "get_investments_by_investor",
    "get_backups",
    "validate_backup",
    "get_backup_details",
];

fn declared_public_entrypoints(source: &str) -> BTreeSet<String> {
    source
        .lines()
        .filter_map(|line| {
            let declaration = line.trim().strip_prefix("pub fn ")?;
            let name = declaration.split('(').next()?.trim();
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

#[test]
fn every_public_entrypoint_is_classified_exactly_once() {
    let declared = declared_public_entrypoints(CONTRACT_SOURCE);
    let mut classified = BTreeSet::new();

    for name in MUTATING_ENTRYPOINTS.iter().chain(READ_ONLY_ENTRYPOINTS.iter()) {
        assert!(
            classified.insert(*name),
            "contract entrypoint {name} is classified more than once"
        );
    }

    let unclassified: Vec<_> = declared.difference(&classified).cloned().collect();
    let stale: Vec<_> = classified.difference(&declared).cloned().collect();

    assert!(
        unclassified.is_empty(),
        "new public contract entrypoints need an explicit mutating/read-only classification: {unclassified:?}"
    );
    assert!(
        stale.is_empty(),
        "entrypoint inventory contains methods no longer exported by QuickLendXContract: {stale:?}"
    );
}

#[test]
fn mutating_entrypoint_inventory_is_not_empty() {
    assert!(!MUTATING_ENTRYPOINTS.is_empty());
}
