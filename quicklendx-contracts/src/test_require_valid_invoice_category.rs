//! Tests for `require_valid_invoice_category` — the category-allowlist helper.
//!
//! # Context
//!
//! The `InvoiceCategory` enum defines nine variants.  `require_valid_invoice_category`
//! explicitly accepts eight of them and rejects `InvoiceCategory::Other` as a
//! "reserved" catch-all that is too generic for new invoices.  This design
//! forces callers to choose a meaningful category.
//!
//! # Negative test
//!
//! `require_valid_invoice_category_rejects_other_as_reserved` — this is the
//! negative test: `Other` is rejected with `InvalidTag`.  Before the addition
//! of `require_valid_invoice_category`, `Other` was accepted by the existing
//! `validate_invoice_category` and no helper surfaced a typed error for it.
//!
//! # Threat mitigated
//!
//! Without this rejection, a business could default to `Other` for every
//! invoice, defeating the categorisation that investors and the protocol rely
//! on for risk assessment.  By treating `Other` as reserved, we force callers
//! to select a specific category or add a new variant to the enum.

#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::types::InvoiceCategory;
use crate::verification::require_valid_invoice_category;
use soroban_sdk::Env;

fn setup() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

// ============================================================================
// Accepted categories
// ============================================================================

#[test]
fn require_valid_invoice_category_accepts_services() {
    require_valid_invoice_category(&InvoiceCategory::Services).expect("Services must be accepted");
}

#[test]
fn require_valid_invoice_category_accepts_goods() {
    require_valid_invoice_category(&InvoiceCategory::Goods).expect("Goods must be accepted");
}

#[test]
fn require_valid_invoice_category_accepts_consulting() {
    require_valid_invoice_category(&InvoiceCategory::Consulting)
        .expect("Consulting must be accepted");
}

#[test]
fn require_valid_invoice_category_accepts_logistics() {
    require_valid_invoice_category(&InvoiceCategory::Logistics)
        .expect("Logistics must be accepted");
}

#[test]
fn require_valid_invoice_category_accepts_products() {
    require_valid_invoice_category(&InvoiceCategory::Products).expect("Products must be accepted");
}

#[test]
fn require_valid_invoice_category_accepts_manufacturing() {
    require_valid_invoice_category(&InvoiceCategory::Manufacturing)
        .expect("Manufacturing must be accepted");
}

#[test]
fn require_valid_invoice_category_accepts_technology() {
    require_valid_invoice_category(&InvoiceCategory::Technology)
        .expect("Technology must be accepted");
}

#[test]
fn require_valid_invoice_category_accepts_healthcare() {
    require_valid_invoice_category(&InvoiceCategory::Healthcare)
        .expect("Healthcare must be accepted");
}

// ============================================================================
// Reserved category — negative test
// ============================================================================

/// NEGATIVE TEST — `InvoiceCategory::Other` is treated as reserved and must be
/// rejected with `InvalidTag`.
#[test]
fn require_valid_invoice_category_rejects_other_as_reserved() {
    let err = require_valid_invoice_category(&InvoiceCategory::Other)
        .expect_err("Other must be rejected as reserved");

    assert_eq!(
        err,
        QuickLendXError::InvalidTag,
        "reserved category must return InvalidTag"
    );
}
