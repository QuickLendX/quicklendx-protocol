//! Regression tests: dispute resolution outcomes must be **enforced**, not
//! merely recorded (issue #2963).
//!
//! `resolution_policy.rs` specifies what each [`DisputeResolution`] outcome
//! must do to invoice, settlement, escrow, and investment state. Before this
//! issue, `resolve_dispute_structured` only stored the outcome and set
//! `dispute_status = Resolved`, so a `FavorInvestor` ruling left the escrow
//! `Held`, the investment `Active`, and settlement/default fully open — the
//! arbiter's decision had no effect.
//!
//! These tests exercise the enforcement wired by
//! `dispute::apply_resolution_policy` and
//! `settlement::require_no_blocking_resolution`:
//!
//! | Test | Boundary locked in |
//! |---|---|
//! | `favor_investor_refunds_and_blocks` | `FavorInvestor` refunds escrow to the investor, transitions invoice+investment to `Refunded`, and blocks payments/defaults afterwards |
//! | `favor_business_unblocks_settlement` | `FavorBusiness` leaves escrow/investment untouched; a full payment settles the invoice and releases escrow to the business |
//! | `split_holds_everything` | `Split` keeps escrow `Held`, invoice `Funded`, investment `Active`, and blocks both settlement and default |
//! | `unstructured_resolution_not_release` | the free-text `resolve_dispute` maps to `Split` (held for admin), never to a release |
//! | `policy_table_matches_enforcement` | for **every** `DisputeResolution` variant, observed effects equal `ResolutionPolicy::for_outcome(variant)` — ties the spec to the code |

#![cfg(test)]

// Balance-delta reporting below uses `std::println!` (the crate is `no_std`).
extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

use crate::defaults::mark_invoice_defaulted;
use crate::dispute::{create_dispute, put_dispute_under_review, resolve_dispute, resolve_dispute_structured};
use crate::errors::QuickLendXError;
use crate::investment::InvestmentStorage;
use crate::payments::{create_escrow, EscrowStatus, EscrowStorage};
use crate::resolution_policy::{
    EscrowEffect, InvoiceEffect, InvestmentEffect, ResolutionPolicy, SettlementEffect,
};
use crate::settlement::process_partial_payment;
use crate::storage::InvoiceStorage;
use crate::types::{
    Dispute, DisputeResolution, DisputeStatus, Invoice, InvoiceCategory, InvoiceStatus, Investment,
    InvestmentStatus,
};
use crate::QuickLendXContract;

// ============================================================================
// Fixture
// ============================================================================

const SECONDS_PER_DAY: u64 = 86_400;
/// Invoice face value == investment principal, so the profit pool (and thus
/// the platform fee) is zero and settlement arithmetic stays exact.
const AMOUNT: i128 = 10_000;
/// Well past `due_date` (30d) + `DEFAULT_GRACE_PERIOD` (7d), so the default
/// path is temporally open and only the dispute guard can block it.
const LONG_PAST_DUE: u64 = 40 * 86_400;

struct Fixture {
    env: Env,
    contract: Address,
    admin: Address,
    business: Address,
    investor: Address,
    currency: Address,
    invoice_id: BytesN<32>,
    investor_starting_balance: i128,
}

fn invoice_id(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0x42u8; 32])
}

/// 64-character hex string accepted by `validate_transaction_hash`.
/// (`seed` selects the repeated hex digit; each fixture gets a fresh `Env`,
/// so one successful payment per env never trips replay protection.)
fn nonce(env: &Env, seed: u8) -> String {
    let ch = b"0123456789abcdef"[(seed % 16) as usize];
    let buf = [ch; 64];
    let s = core::str::from_utf8(&buf).expect("ascii hex is valid utf-8");
    String::from_str(env, s)
}

fn load_invoice(f: &Fixture) -> Invoice {
    f.env
        .as_contract(&f.contract, || {
            InvoiceStorage::get_invoice(&f.env, &f.invoice_id).expect("invoice must exist")
        })
}

fn load_escrow(f: &Fixture) -> EscrowStatus {
    f.env
        .as_contract(&f.contract, || {
            EscrowStorage::get_escrow_by_invoice(&f.env, &f.invoice_id)
                .expect("escrow must exist")
                .status
        })
}

fn load_investment_status(f: &Fixture) -> InvestmentStatus {
    f.env
        .as_contract(&f.contract, || {
            InvestmentStorage::get_investment_by_invoice(&f.env, &f.invoice_id)
                .expect("investment must exist")
                .status
        })
}

/// Attempt a small business payment; returns the settlement-layer result.
fn try_partial_payment(f: &Fixture, amount: i128) -> Result<(), QuickLendXError> {
    let tx = nonce(&f.env, 0xab);
    f.env.as_contract(&f.contract, || {
        process_partial_payment(&f.env, &f.invoice_id, amount, tx)
    })
}

/// Build a funded invoice with a `Held` escrow, an `Active` investment, and a
/// dispute that has completed the mandatory review step — i.e. the exact
/// pre-state `resolve_dispute_structured` requires.
fn setup_under_review_dispute() -> Fixture {
    let env = Env::default();
    // Recording auth rejects a second `require_auth` for the same address
    // within one contract frame ("frame is already authorized"), so every
    // auth-bearing step below gets its own `as_contract` frame — mirroring
    // how each of these would be a separate entrypoint call on-chain.
    env.mock_all_auths();
    let now = 1_000_000u64;
    env.ledger().set_timestamp(now);

    let contract = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    // Stellar Asset Contract fixture: investor funds the escrow, business
    // keeps a balance + allowance to back its own repayment transfers.
    let token_admin = Address::generate(&env);
    let currency = env.register_stellar_asset_contract_v2(token_admin).address();
    let sac = token::StellarAssetClient::new(&env, &currency);
    let tok = token::Client::new(&env, &currency);
    let starting = AMOUNT * 10;
    sac.mint(&investor, &starting);
    sac.mint(&business, &starting);
    let approval_expiry = env.ledger().sequence() + 100_000;
    tok.approve(&investor, &contract, &starting, &approval_expiry);
    tok.approve(&business, &contract, &starting, &approval_expiry);

    let id = invoice_id(&env);
    let due_date = now + 30 * SECONDS_PER_DAY;

    env.as_contract(&contract, || {
        crate::admin::AdminStorage::initialize(&env, &admin).expect("admin init");
    });
    env.as_contract(&contract, || {
        crate::arbiter::ArbiterStorage::register_arbiter(&env, &admin, &admin)
            .expect("arbiter registration");
    });

    env.as_contract(&contract, || {
        let invoice = Invoice {
            id: id.clone(),
            business: business.clone(),
            amount: AMOUNT,
            currency: currency.clone(),
            due_date,
            status: InvoiceStatus::Funded,
            created_at: now,
            description: String::from_str(&env, "resolution policy enforcement fixture"),
            metadata_customer_name: None,
            metadata_customer_address: None,
            metadata_tax_id: None,
            metadata_notes: None,
            metadata_line_items: Vec::new(&env),
            category: InvoiceCategory::Services,
            tags: Vec::new(&env),
            funded_amount: AMOUNT,
            funded_at: Some(now),
            investor: Some(investor.clone()),
            settled_at: None,
            average_rating: None,
            total_ratings: 0,
            ratings: Vec::new(&env),
            dispute_status: DisputeStatus::None,
            dispute: Dispute {
                created_by: business.clone(),
                created_at: 0,
                reason: String::from_str(&env, ""),
                evidence: String::from_str(&env, ""),
                resolution: String::from_str(&env, ""),
                resolved_by: business.clone(),
                resolved_at: 0,
                resolution_outcome: DisputeResolution::None,
            },
            total_paid: 0,
            payment_history: Vec::new(&env),
            origination_fee_bps: None,
            late_payment_penalty_bps: None,
            early_payment_discount_bps: None,
        };
        InvoiceStorage::update_invoice(&env, &invoice);

        create_escrow(&env, &id, &investor, &business, AMOUNT, &currency)
            .expect("create_escrow");

        let investment = Investment {
            investment_id: BytesN::from_array(&env, &[0x99u8; 32]),
            invoice_id: id.clone(),
            investor: investor.clone(),
            amount: AMOUNT,
            funded_at: now,
            status: InvestmentStatus::Active,
            insurance: Vec::new(&env),
        };
        InvestmentStorage::store_investment(&env, &investment);
    });

    // Real dispute lifecycle: investor opens (its own auth frame), admin
    // puts under review (`require_admin` is identity-only, no auth).
    let reason = String::from_str(&env, "work not delivered");
    let evidence = String::from_str(&env, "delivery-log-hash");
    env.as_contract(&contract, || {
        create_dispute(&env, &id, &investor, &reason, &evidence).expect("create_dispute")
    });
    env.as_contract(&contract, || {
        put_dispute_under_review(&env, &admin, &id).expect("put_dispute_under_review")
    });

    Fixture {
        investor_starting_balance: tok.balance(&investor),
        env,
        contract,
        admin,
        business,
        investor,
        currency,
        invoice_id: id,
    }
}

fn resolve_structured(f: &Fixture, outcome: DisputeResolution) -> Result<(), QuickLendXError> {
    let note = String::from_str(&f.env, "ruled by arbiter");
    f.env.as_contract(&f.contract, || {
        resolve_dispute_structured(&f.env, &f.admin, &f.invoice_id, outcome, &note)
    })
}

// ============================================================================
// FavorInvestor: refund + permanent block
// ============================================================================

/// The reproduction from issue #2963. Pre-fix: escrow stayed `Held`, the
/// investment stayed `Active`, the investor's balance was unchanged, and the
/// business could still pay and default the invoice. Post-fix: the escrow is
/// refunded to the investor, invoice + investment become `Refunded`, and
/// every later payment/default attempt fails with the dispute-specific error.
#[test]
fn favor_investor_refunds_and_blocks() {
    let f = setup_under_review_dispute();
    let tok = token::Client::new(&f.env, &f.currency);
    let contract_balance_before = tok.balance(&f.contract);

    let investor_before = tok.balance(&f.investor);

    resolve_structured(&f, DisputeResolution::FavorInvestor).expect("resolution must succeed");

    // Balance deltas (printed for the issue's PR evidence):
    // investor +escrow amount, contract -escrow amount.
    std::println!(
        "balance delta: investor {} -> {} (+{})",
        investor_before,
        tok.balance(&f.investor),
        tok.balance(&f.investor) - investor_before
    );
    std::println!(
        "balance delta: contract  {} -> {} (-{})",
        contract_balance_before,
        tok.balance(&f.contract),
        contract_balance_before - tok.balance(&f.contract)
    );

    // Escrow refunded: investor is whole again, contract holds nothing.
    assert_eq!(
        tok.balance(&f.investor),
        f.investor_starting_balance + AMOUNT,
        "investor balance must increase by exactly the escrow amount"
    );
    assert_eq!(
        tok.balance(&f.contract),
        contract_balance_before - AMOUNT,
        "contract must no longer hold the escrowed funds"
    );
    assert_eq!(load_escrow(&f), EscrowStatus::Refunded, "escrow must be Refunded");

    // Invoice + investment terminal.
    let invoice = load_invoice(&f);
    assert_eq!(invoice.status, InvoiceStatus::Refunded, "invoice must be Refunded");
    assert_eq!(
        invoice.dispute_status,
        DisputeStatus::Resolved,
        "dispute must be Resolved"
    );
    assert_eq!(
        invoice.dispute.resolution_outcome,
        DisputeResolution::FavorInvestor,
        "outcome must be recorded"
    );
    assert_eq!(
        load_investment_status(&f),
        InvestmentStatus::Refunded,
        "investment must be Refunded"
    );

    // Payments are now refused with the dispute-specific error.
    assert_eq!(
        try_partial_payment(&f, 100),
        Err(QuickLendXError::DisputeResolutionBlocksSettlement),
        "payment after FavorInvestor must fail with the blocked-resolution error"
    );

    // Defaulting after due_date + grace is also refused.
    f.env
        .ledger()
        .set_timestamp(invoice.due_date + 10 * SECONDS_PER_DAY);
    let defaulted = f
        .env
        .as_contract(&f.contract, || mark_invoice_defaulted(&f.env, &f.invoice_id, None));
    assert!(
        defaulted.is_err(),
        "default after FavorInvestor must be blocked, got Ok"
    );
}

// ============================================================================
// FavorBusiness: unblocked settlement
// ============================================================================

/// After `FavorBusiness` nothing is refunded and nothing is blocked: a full
/// business payment settles the invoice, escrow is released to the business,
/// and the investment completes.
#[test]
fn favor_business_unblocks_settlement() {
    let f = setup_under_review_dispute();
    let tok = token::Client::new(&f.env, &f.currency);

    resolve_structured(&f, DisputeResolution::FavorBusiness).expect("resolution must succeed");

    // Policy: nothing moves at resolution time.
    assert_eq!(load_escrow(&f), EscrowStatus::Held, "escrow stays Held until settlement");
    assert_eq!(load_invoice(&f).status, InvoiceStatus::Funded, "invoice stays Funded");
    assert_eq!(
        load_investment_status(&f),
        InvestmentStatus::Active,
        "investment stays Active"
    );

    // A full payment must settle the invoice and release escrow.
    try_partial_payment(&f, AMOUNT).expect("full payment after FavorBusiness must settle");

    let invoice = load_invoice(&f);
    assert_eq!(invoice.status, InvoiceStatus::Paid, "invoice must be Paid");
    assert_eq!(invoice.total_paid, AMOUNT, "full amount must be recorded");
    assert_eq!(
        load_escrow(&f),
        EscrowStatus::Released,
        "escrow must be released to the business"
    );
    assert_eq!(
        load_investment_status(&f),
        InvestmentStatus::Completed,
        "investment must be Completed"
    );
    // Accounting: business got its principal back out of escrow and repaid
    // from its own balance; the investor is made whole again — the escrowed
    // principal comes back as the settlement disbursement.
    assert_eq!(
        tok.balance(&f.investor),
        f.investor_starting_balance + AMOUNT,
        "investor ends whole after settlement"
    );
}

// ============================================================================
// Split: hold everything pending admin follow-up
// ============================================================================

/// After `Split` the funds stay frozen: escrow `Held`, invoice `Funded`,
/// investment `Active`, and both settlement and default are blocked until an
/// admin follow-up.
#[test]
fn split_holds_everything() {
    let f = setup_under_review_dispute();
    let tok = token::Client::new(&f.env, &f.currency);

    resolve_structured(&f, DisputeResolution::Split).expect("resolution must succeed");

    assert_eq!(load_escrow(&f), EscrowStatus::Held, "escrow must stay Held");
    assert_eq!(load_invoice(&f).status, InvoiceStatus::Funded, "invoice stays Funded");
    assert_eq!(
        load_investment_status(&f),
        InvestmentStatus::Active,
        "investment stays Active"
    );
    assert_eq!(
        tok.balance(&f.contract),
        AMOUNT,
        "contract keeps holding the disputed funds"
    );

    // Settlement blocked.
    assert_eq!(
        try_partial_payment(&f, 100),
        Err(QuickLendXError::DisputeResolutionBlocksSettlement),
        "payment after Split must fail with the blocked-resolution error"
    );

    // Default blocked (invoice is still Funded and temporally overdue, so
    // only the dispute-outcome guard can reject it).
    f.env
        .ledger()
        .set_timestamp(load_invoice(&f).due_date + LONG_PAST_DUE);
    assert_eq!(
        f.env.as_contract(&f.contract, || mark_invoice_defaulted(&f.env, &f.invoice_id, None)),
        Err(QuickLendXError::DisputeResolutionBlocksSettlement),
        "default after Split must fail with the blocked-resolution error"
    );
}

// ============================================================================
// Unstructured resolution: never a release
// ============================================================================

/// `resolve_dispute` accepts free text. The policy mapping used to record
/// `DisputeResolution::None`, whose policy is "release to business, settle
/// normally" — documenting the opposite of e.g. a "refund investor in full"
/// ruling. Decision for issue #2963: the unstructured variant maps to
/// `Split` (funds held for admin follow-up), never to a release.
#[test]
fn unstructured_resolution_not_release() {
    let f = setup_under_review_dispute();
    let tok = token::Client::new(&f.env, &f.currency);
    let resolution = String::from_str(&f.env, "refund investor in full");

    f.env
        .as_contract(&f.contract, || {
            resolve_dispute(&f.env, &f.admin, &f.invoice_id, &resolution)
        })
        .expect("unstructured resolution must still succeed");

    let invoice = load_invoice(&f);
    assert_eq!(
        invoice.dispute.resolution_outcome,
        DisputeResolution::Split,
        "unstructured resolution must map to Split (held for admin)"
    );
    assert_eq!(
        invoice.dispute.resolution,
        resolution,
        "free-text resolution must be preserved"
    );
    assert_eq!(load_escrow(&f), EscrowStatus::Held, "no refund happens on a free-text ruling");
    assert_eq!(invoice.status, InvoiceStatus::Funded, "invoice is not released or refunded");
    assert_eq!(
        tok.balance(&f.contract),
        AMOUNT,
        "funds remain in the contract pending admin follow-up"
    );
    assert_eq!(
        try_partial_payment(&f, 100),
        Err(QuickLendXError::DisputeResolutionBlocksSettlement),
        "settlement must be blocked, never unblocked, after a free-text ruling"
    );
}

// ============================================================================
// Spec/code tie: every variant matches its documented policy
// ============================================================================

/// For every [`DisputeResolution`] variant, the *observed* effects after
/// resolution must equal [`ResolutionPolicy::for_outcome`]. This ties the
/// specification table in `resolution_policy.rs` to the enforcement in
/// `dispute::apply_resolution_policy` + `settlement::require_no_blocking_resolution`.
#[test]
fn policy_table_matches_enforcement() {
    let variants = [
        DisputeResolution::None,
        DisputeResolution::FavorBusiness,
        DisputeResolution::FavorInvestor,
        DisputeResolution::Split,
        DisputeResolution::Dismissed,
    ];

    for outcome in variants {
        let f = setup_under_review_dispute();
        resolve_structured(&f, outcome).unwrap_or_else(|e| {
            panic!("resolution {:?} must succeed, got {:?}", outcome, e)
        });
        let policy = ResolutionPolicy::for_outcome(outcome);

        // Invoice effect.
        let invoice = load_invoice(&f);
        match policy.invoice_effect {
            InvoiceEffect::TransitionToRefunded => assert_eq!(
                invoice.status,
                InvoiceStatus::Refunded,
                "{:?}: invoice must be Refunded",
                outcome
            ),
            InvoiceEffect::StaysFunded
            | InvoiceEffect::PendingAdminAction
            | InvoiceEffect::RevertToPreDisputeStatus => assert_eq!(
                invoice.status,
                InvoiceStatus::Funded,
                "{:?}: invoice must stay Funded",
                outcome
            ),
        }

        // Escrow effect.
        match policy.escrow_effect {
            EscrowEffect::RefundToInvestor => assert_eq!(
                load_escrow(&f),
                EscrowStatus::Refunded,
                "{:?}: escrow must be Refunded",
                outcome
            ),
            EscrowEffect::ReleaseToBusiness | EscrowEffect::HeldPendingAdmin => assert_eq!(
                load_escrow(&f),
                EscrowStatus::Held,
                "{:?}: escrow must stay Held",
                outcome
            ),
        }

        // Investment effect.
        match policy.investment_effect {
            InvestmentEffect::TransitionToRefunded => assert_eq!(
                load_investment_status(&f),
                InvestmentStatus::Refunded,
                "{:?}: investment must be Refunded",
                outcome
            ),
            InvestmentEffect::StaysActive | InvestmentEffect::PendingAdminAction => assert_eq!(
                load_investment_status(&f),
                InvestmentStatus::Active,
                "{:?}: investment must stay Active",
                outcome
            ),
        }

        // Settlement effect: probe with a small payment attempt.
        let probe = try_partial_payment(&f, 100);
        match policy.settlement_effect {
            SettlementEffect::Unblocked => assert!(
                probe.is_ok(),
                "{:?}: settlement must be unblocked, got {:?}",
                outcome,
                probe
            ),
            SettlementEffect::PermanentlyBlocked | SettlementEffect::BlockedPendingAdmin => {
                assert_eq!(
                    probe,
                    Err(QuickLendXError::DisputeResolutionBlocksSettlement),
                    "{:?}: settlement must be blocked by the resolution",
                    outcome
                )
            }
        }
    }
}
