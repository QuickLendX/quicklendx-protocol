//! Aggregated, admin-callable protocol invariant self-check.
//!
//! [`invariant_self_check`] composes the cross-module integrity checks that
//! already exist throughout the contract into a single, read-only "heartbeat"
//! intended for incident response. One call returns an [`InvariantReport`] - a
//! structured list of `(check_name, passed, evidence)` rows - so an operator can
//! confirm protocol health (or pinpoint a violated invariant) without issuing
//! many separate queries.
//!
//! ## Security
//! - **Read-only.** None of the checks mutate state. They only read storage via
//!   existing getters (`get_*`) and validators (`validate_*`). A failing or
//!   unauthorized call therefore cannot alter the ledger - see the
//!   `failures never modify state` tests in `test_invariant_self_check.rs`.
//! - **Admin-gated.** The entrypoint authenticates the caller as the stored
//!   admin (`require_auth` + admin-equality) *before* running any check, so the
//!   heartbeat cannot be used by arbitrary callers to probe internal state.
//! - **Incident-response usage.** Treat a `passed == false` row as a signal to
//!   pause the protocol and investigate; the `evidence` string names the failure
//!   mode. The report is a diagnostic, not a remediation - it never repairs the
//!   inconsistency it detects.

use soroban_sdk::{contracttype, Address, BytesN, Env, String, Vec};

use crate::admin::AdminStorage;
use crate::audit::AuditStorage;
use crate::errors::QuickLendXError;
use crate::investment::InvestmentStorage;
use crate::payments::{EscrowStatus, EscrowStorage};
use crate::storage::InvoiceStorage;
use crate::types::InvoiceStatus;

/// A single invariant check result row.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvariantCheck {
    /// Stable, machine-readable identifier of the invariant (e.g.
    /// `"no_orphan_investments"`). Safe to match on in tooling.
    pub check_name: String,
    /// `true` when the invariant holds, `false` when a violation was observed.
    pub passed: bool,
    /// Human-readable description of what was verified and, on failure, the
    /// detected failure mode. Diagnostic only.
    pub evidence: String,
}

/// Aggregated result of all invariant checks.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvariantReport {
    /// One [`InvariantCheck`] row per composed invariant, in execution order.
    pub checks: Vec<InvariantCheck>,
    /// `true` only when every row in `checks` passed. The single field an
    /// operator should alert on.
    pub all_passed: bool,
    /// Ledger timestamp (seconds) at which the report was produced.
    pub checked_at: u64,
}

/// Build one result row from `&str` literals.
fn row(env: &Env, name: &str, passed: bool, evidence: &str) -> InvariantCheck {
    InvariantCheck {
        check_name: String::from_str(env, name),
        passed,
        evidence: String::from_str(env, evidence),
    }
}

/// Every entry in the active-investment index must still carry
/// `InvestmentStatus::Active`. A terminal-status entry indicates a transition
/// path failed to de-index the record (an orphan).
fn check_no_orphan_investments(env: &Env) -> InvariantCheck {
    let passed = InvestmentStorage::validate_no_orphan_investments(env);
    let evidence = if passed {
        "All active-investment index entries carry InvestmentStatus::Active."
    } else {
        "Orphan detected: an active-index entry has a terminal status (deindex path failed)."
    };
    row(env, "no_orphan_investments", passed, evidence)
}

/// Every invoice's audit trail must hash-chain validate: no missing entries and
/// every entry passes `validate_integrity` (timestamps/heights/values).
fn check_audit_chain_integrity(env: &Env) -> InvariantCheck {
    let mut passed = true;
    for id in InvoiceStorage::get_all_invoice_ids(env).iter() {
        // A validation error is treated as a failure: integrity could not be
        // confirmed. `unwrap_or(false)` keeps this check read-only and total.
        if !AuditStorage::validate_invoice_audit_integrity(env, &id).unwrap_or(false) {
            passed = false;
            break;
        }
    }
    let evidence = if passed {
        "Every invoice audit trail validated (no missing or tampered entries)."
    } else {
        "An invoice audit entry is missing or fails integrity validation."
    };
    row(env, "audit_chain_integrity", passed, evidence)
}

/// Accounting soundness: no active investment may carry non-positive principal,
/// and no `Funded` invoice may be funded beyond its own face value (which would
/// represent the protocol owing more than the underlying asset is worth).
fn check_solvency(env: &Env) -> InvariantCheck {
    let mut passed = true;

    for id in InvestmentStorage::get_active_investment_ids(env).iter() {
        if let Some(inv) = InvestmentStorage::get_investment(env, &id) {
            if inv.amount <= 0 {
                passed = false;
                break;
            }
        }
    }

    if passed {
        for id in InvoiceStorage::get_by_status(env, InvoiceStatus::Funded).iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(env, &id) {
                if invoice.funded_amount <= 0 || invoice.funded_amount > invoice.amount {
                    passed = false;
                    break;
                }
            }
        }
    }

    let evidence = if passed {
        "Active principals are positive and no funded invoice exceeds its face value."
    } else {
        "Insolvency signal: non-positive active principal or funded_amount > invoice amount."
    };
    row(env, "solvency", passed, evidence)
}

/// Storage-index coherence: each invoice must live in exactly one status index
/// whose status equals the invoice's actual status, and the de-duplicated total
/// across status indexes must equal the full id set (no drift / double-count).
fn check_storage_index_coherence(env: &Env) -> InvariantCheck {
    let statuses = [
        InvoiceStatus::Pending,
        InvoiceStatus::Verified,
        InvoiceStatus::Funded,
        InvoiceStatus::Paid,
        InvoiceStatus::Defaulted,
        InvoiceStatus::Cancelled,
        InvoiceStatus::Refunded,
    ];

    let mut passed = true;
    let mut indexed_total: u32 = 0;

    for status in statuses.iter() {
        for id in InvoiceStorage::get_by_status(env, *status).iter() {
            indexed_total += 1;
            match InvoiceStorage::get_invoice(env, &id) {
                // Index says one status; the record must agree.
                Some(invoice) => {
                    if invoice.status != *status {
                        passed = false;
                    }
                }
                // Index references a record that no longer exists.
                None => passed = false,
            }
        }
    }

    // A duplicate across two status indexes inflates `indexed_total` above the
    // de-duplicated id set produced by `get_all_invoice_ids`.
    if passed && indexed_total != InvoiceStorage::get_all_invoice_ids(env).len() {
        passed = false;
    }

    let evidence = if passed {
        "Each invoice sits in exactly one matching status index; index counts agree."
    } else {
        "Status-index drift: an invoice is misindexed, missing, or double-counted."
    };
    row(env, "storage_index_coherence", passed, evidence)
}

/// Sum of all active investments must be less than or equal to the sum of all invoice amounts.
///
/// **Cost:** O(N_active + N_all) persistent reads to iterate active investments and invoices.
fn check_sum_investments_le_sum_invoices(env: &Env) -> InvariantCheck {
    let mut sum_investments: i128 = 0;
    let mut sum_invoices: i128 = 0;
    let mut passed = true;

    for id in InvestmentStorage::get_active_investment_ids(env).iter() {
        if let Some(inv) = InvestmentStorage::get_investment(env, &id) {
            sum_investments = match sum_investments.checked_add(inv.amount) {
                Some(val) => val,
                None => {
                    passed = false;
                    break;
                }
            };
        }
    }

    if passed {
        for id in InvoiceStorage::get_all_invoice_ids(env).iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(env, &id) {
                sum_invoices = match sum_invoices.checked_add(invoice.amount) {
                    Some(val) => val,
                    None => {
                        passed = false;
                        break;
                    }
                };
            }
        }
    }

    if passed && sum_investments > sum_invoices {
        passed = false;
    }

    let evidence = if passed {
        "Sum of active investments is less than or equal to sum of invoice amounts."
    } else {
        "Accounting discrepancy: sum of active investments exceeds sum of invoice amounts."
    };
    row(env, "sum_investments_le_sum_invoices", passed, evidence)
}

/// Escrow mapping uniqueness check: verifies that each invoice has at most one escrow mapping
/// and that any present escrow correctly points back to that invoice ID.
///
/// **Cost:** O(N_all) persistent reads to scan all invoice escrow mappings.
fn check_escrow_uniqueness(env: &Env) -> InvariantCheck {
    let mut passed = true;
    for id in InvoiceStorage::get_all_invoice_ids(env).iter() {
        let invoice_key = (soroban_sdk::symbol_short!("escrow"), &id);
        if let Some(escrow_id) = env
            .storage()
            .persistent()
            .get::<_, BytesN<32>>(&invoice_key)
        {
            if let Some(escrow) = EscrowStorage::get_escrow(env, &escrow_id) {
                if escrow.invoice_id != id {
                    passed = false;
                    break;
                }
            } else {
                passed = false;
                break;
            }
        }
    }

    let evidence = if passed {
        "Every present escrow mapping is unique and correctly references its invoice ID."
    } else {
        "Escrow uniqueness violation: mismatch or orphan escrow pointer detected."
    };
    row(env, "escrow_uniqueness", passed, evidence)
}

/// Settlement accounting identity: recalculate fees and returns for settled/Paid invoices
/// and verify that `investor_return + platform_fee == total_paid`.
///
/// **Cost:** O(N_paid) persistent reads to inspect all Paid invoices and their corresponding investments.
fn check_settlement_accounting_identity(env: &Env) -> InvariantCheck {
    let mut passed = true;
    for id in InvoiceStorage::get_by_status(env, InvoiceStatus::Paid).iter() {
        if let Some(invoice) = InvoiceStorage::get_invoice(env, &id) {
            if let Some(investment) = InvestmentStorage::get_investment_by_invoice(env, &id) {
                let (investor_return, platform_fee) =
                    match crate::fees::FeeManager::calculate_platform_fee(
                        env,
                        investment.amount,
                        invoice.total_paid,
                    ) {
                        Ok(result) => result,
                        Err(crate::errors::QuickLendXError::StorageKeyNotFound) => {
                            crate::profits::calculate_profit(
                                env,
                                investment.amount,
                                invoice.total_paid,
                            )
                        }
                        Err(_) => {
                            passed = false;
                            break;
                        }
                    };

                let disbursement_total = match investor_return.checked_add(platform_fee) {
                    Some(val) => val,
                    None => {
                        passed = false;
                        break;
                    }
                };

                if disbursement_total != invoice.total_paid {
                    passed = false;
                    break;
                }
            } else {
                passed = false;
                break;
            }
        }
    }

    let evidence = if passed {
        "All Paid invoices satisfy investor_return + platform_fee == total_paid."
    } else {
        "Accounting identity violation: investor_return + platform_fee != total_paid on a settled invoice."
    };
    row(env, "settlement_accounting_identity", passed, evidence)
}

/// Run every composed invariant check and assemble the report.
///
/// Read-only and independent of admin gating, so tests can exercise it directly
/// (including under intentionally broken state). The public entrypoint is
/// [`invariant_self_check`].
pub fn run_invariant_checks(env: &Env) -> InvariantReport {
    let mut checks = Vec::new(env);
    checks.push_back(check_no_orphan_investments(env));
    checks.push_back(check_audit_chain_integrity(env));
    checks.push_back(check_solvency(env));
    checks.push_back(check_storage_index_coherence(env));
    checks.push_back(check_sum_investments_le_sum_invoices(env));
    checks.push_back(check_escrow_uniqueness(env));
    checks.push_back(check_settlement_accounting_identity(env));

    let mut all_passed = true;
    for c in checks.iter() {
        if !c.passed {
            all_passed = false;
        }
    }

    InvariantReport {
        checks,
        all_passed,
        checked_at: env.ledger().timestamp(),
    }
}

/// Admin-gated protocol heartbeat. Authenticates `admin` as the stored protocol
/// admin, then runs every composed invariant check read-only.
///
/// Returns [`InvariantReport`] on success, or `QuickLendXError::NotAdmin` /
/// `OperationNotAllowed` when the caller is not the initialized admin. Because
/// gating happens before any check and the checks never write, an unauthorized
/// or failing call leaves the ledger unchanged.
pub fn invariant_self_check(
    env: &Env,
    admin: &Address,
) -> Result<InvariantReport, QuickLendXError> {
    AdminStorage::require_admin_auth(env, admin)?;
    Ok(run_invariant_checks(env))
}
