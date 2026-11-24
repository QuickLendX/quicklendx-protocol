use alloc::{collections::BTreeSet, string::String as StdString};

use crate::admin::AdminStorage;
use crate::payments::{Escrow, EscrowStatus, EscrowStorage};
use crate::storage::{BidStorage, InvestmentStorage, InvoiceStorage};
use crate::types::{
    Bid, BidStatus, DisputeResolution, DisputeStatus, Investment, InvestmentStatus,
    InvoiceCategory, InvoiceStatus,
};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AccessClass {
    Mutating,
    Admin,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Entrypoint {
    name: &'static str,
    access: AccessClass,
}

// Keep this list in sync with QuickLendXContract's #[contractimpl] below.
// When adding an entrypoint, classify it here and add its try_* invocation to
// the corresponding authorization test. Ported entrypoints that are not
// present in lib.rs belong in PENDING_PORTS until they are exported.
const ENTRYPOINTS: &[Entrypoint] = &[
    Entrypoint { name: "set_admin", access: AccessClass::Admin },
    Entrypoint { name: "get_admin", access: AccessClass::ReadOnly },
    Entrypoint { name: "initialize_fee_system", access: AccessClass::Admin },
    Entrypoint { name: "update_platform_fee_bps", access: AccessClass::Admin },
    Entrypoint { name: "get_platform_fee_config", access: AccessClass::ReadOnly },
    Entrypoint { name: "calculate_profit", access: AccessClass::ReadOnly },
    Entrypoint { name: "invariant_self_check", access: AccessClass::Admin },
    Entrypoint { name: "initialize_protocol_limits", access: AccessClass::Admin },
    Entrypoint { name: "pause", access: AccessClass::Admin },
    Entrypoint { name: "unpause", access: AccessClass::Admin },
    Entrypoint { name: "is_paused", access: AccessClass::ReadOnly },
    Entrypoint { name: "submit_kyc_application", access: AccessClass::Mutating },
    Entrypoint { name: "verify_business", access: AccessClass::Admin },
    Entrypoint { name: "submit_investor_kyc", access: AccessClass::Mutating },
    Entrypoint { name: "verify_investor", access: AccessClass::Admin },
    Entrypoint { name: "store_invoice", access: AccessClass::Mutating },
    Entrypoint { name: "get_invoice", access: AccessClass::ReadOnly },
    Entrypoint { name: "verify_invoice", access: AccessClass::Admin },
    Entrypoint { name: "update_invoice_status", access: AccessClass::Admin },
    Entrypoint { name: "cancel_invoice", access: AccessClass::Mutating },
    Entrypoint { name: "get_available_invoices", access: AccessClass::ReadOnly },
    Entrypoint { name: "get_business_invoices", access: AccessClass::ReadOnly },
    Entrypoint { name: "place_bid", access: AccessClass::Mutating },
    Entrypoint { name: "accept_bid", access: AccessClass::Mutating },
    Entrypoint { name: "get_bid", access: AccessClass::ReadOnly },
    Entrypoint { name: "get_bids_for_invoice", access: AccessClass::ReadOnly },
    Entrypoint { name: "get_escrow_details", access: AccessClass::ReadOnly },
    Entrypoint { name: "release_escrow", access: AccessClass::Mutating },
    Entrypoint { name: "get_investment", access: AccessClass::ReadOnly },
    Entrypoint { name: "get_invoice_investment", access: AccessClass::ReadOnly },
    Entrypoint { name: "process_partial_payment", access: AccessClass::Mutating },
    Entrypoint { name: "settle_invoice", access: AccessClass::Mutating },
    Entrypoint { name: "get_invoice_progress", access: AccessClass::ReadOnly },
    Entrypoint { name: "get_payment_count", access: AccessClass::ReadOnly },
    Entrypoint { name: "get_payment_records", access: AccessClass::ReadOnly },
    Entrypoint { name: "is_invoice_finalized", access: AccessClass::ReadOnly },
    Entrypoint { name: "get_settlement_batch_size", access: AccessClass::ReadOnly },
    Entrypoint { name: "get_settlement_batch_size_max", access: AccessClass::ReadOnly },
    Entrypoint { name: "create_dispute", access: AccessClass::Mutating },
    Entrypoint { name: "put_dispute_under_review", access: AccessClass::Admin },
    Entrypoint { name: "resolve_dispute", access: AccessClass::Admin },
    Entrypoint { name: "resolve_dispute_structured", access: AccessClass::Admin },
    Entrypoint { name: "get_dispute_details", access: AccessClass::ReadOnly },
    Entrypoint { name: "get_invoices_with_disputes", access: AccessClass::ReadOnly },
    Entrypoint { name: "get_invoices_by_dispute_status", access: AccessClass::ReadOnly },
    Entrypoint { name: "get_dispute_timeline", access: AccessClass::ReadOnly },
    Entrypoint { name: "update_dispute_evidence", access: AccessClass::Mutating },
    Entrypoint { name: "register_arbiter", access: AccessClass::Admin },
    Entrypoint { name: "unregister_arbiter", access: AccessClass::Admin },
    Entrypoint { name: "list_arbiters", access: AccessClass::ReadOnly },
    Entrypoint { name: "is_arbiter", access: AccessClass::ReadOnly },
    Entrypoint { name: "create_backup", access: AccessClass::Admin },
    Entrypoint { name: "restore_backup", access: AccessClass::Admin },
    Entrypoint { name: "set_backup_retention_policy", access: AccessClass::Admin },
    Entrypoint { name: "archive_backup", access: AccessClass::Admin },
    Entrypoint { name: "cleanup_backups", access: AccessClass::Admin },
    Entrypoint { name: "get_backups", access: AccessClass::ReadOnly },
    Entrypoint { name: "get_backup_details", access: AccessClass::ReadOnly },
    Entrypoint { name: "validate_backup", access: AccessClass::ReadOnly },
    Entrypoint { name: "preview_cleanup_backups", access: AccessClass::ReadOnly },
];

// Names of already identified entrypoints waiting to be ported into lib.rs.
// Keep these explicit rather than silently treating absent exports as covered.
const PENDING_PORTS: &[&str] = &[];

fn exported_entrypoints() -> BTreeSet<StdString> {
    let source = include_str!("lib.rs");
    let contract_impl = source
        .split_once("#[contractimpl]")
        .expect("contract implementation exists")
        .1
        .split_once("impl QuickLendXContract {")
        .expect("QuickLendXContract implementation exists")
        .1
        .split_once("\n}")
        .expect("contract implementation is closed")
        .0;

    contract_impl
        .lines()
        .filter_map(|line| {
            let declaration = line.split_once("pub fn ")?.1;
            let name = declaration
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .next()?;
            Some(StdString::from(name))
        })
        .collect()
}

fn asserted_entrypoints(macro_invocation: &str) -> BTreeSet<StdString> {
    let source = include_str!("test_entrypoint_auth_matrix.rs");
    source
        .split(macro_invocation)
        .skip(1)
        .filter_map(|invocation| {
            if !invocation.starts_with('(') {
                return None;
            }
            invocation.split('"').nth(1).map(StdString::from)
        })
        .collect()
}

#[test]
fn every_exported_entrypoint_has_exactly_one_access_class() {
    let actual = exported_entrypoints();
    let classified = ENTRYPOINTS
        .iter()
        .map(|entrypoint| entrypoint.name.to_owned())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        classified.len(),
        ENTRYPOINTS.len(),
        "an entrypoint is classified more than once"
    );
    assert_eq!(
        actual, classified,
        "update ENTRYPOINTS when the #[contractimpl] surface changes"
    );
    assert!(
        PENDING_PORTS.iter().all(|name| !actual.contains(*name)),
        "move a pending port into ENTRYPOINTS when it is exported"
    );

    let expected_auth = ENTRYPOINTS
        .iter()
        .filter(|entrypoint| entrypoint.access != AccessClass::ReadOnly)
        .map(|entrypoint| StdString::from(entrypoint.name))
        .collect::<BTreeSet<_>>();
    let expected_read_only = ENTRYPOINTS
        .iter()
        .filter(|entrypoint| entrypoint.access == AccessClass::ReadOnly)
        .map(|entrypoint| StdString::from(entrypoint.name))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        asserted_entrypoints("auth!"),
        expected_auth,
        "every mutating/admin entrypoint must have an auth assertion"
    );
    assert_eq!(
        asserted_entrypoints("read_only!"),
        expected_read_only,
        "every read-only entrypoint must have an unauthenticated success assertion"
    );
}

#[test]
fn mutating_entrypoints_require_auth_and_read_only_entrypoints_do_not() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let currency = Address::generate(&env);
    let now = env.ledger().timestamp();

    env.mock_all_auths();
    let (verified_id, funded_id, investment) = env.as_contract(&contract_id, || {
        AdminStorage::initialize(&env, &admin).expect("initialize test admin");

        let mut verified_invoice = crate::invoice::Invoice::new(
            &env,
            business.clone(),
            1_000,
            currency.clone(),
            now + 86_400,
            String::from_str(&env, "auth matrix"),
            InvoiceCategory::Services,
            Vec::new(&env),
        )
        .expect("create verified invoice");
        verified_invoice.status = InvoiceStatus::Verified;

        let mut funded_invoice = crate::invoice::Invoice::new(
            &env,
            business.clone(),
            1_000,
            currency.clone(),
            now + 86_400,
            String::from_str(&env, "auth matrix funded"),
            InvoiceCategory::Services,
            Vec::new(&env),
        )
        .expect("create funded invoice");
        funded_invoice.status = InvoiceStatus::Funded;
        funded_invoice.funded_amount = 1_000;
        funded_invoice.funded_at = Some(now);
        funded_invoice.investor = Some(investor.clone());

        let verified_id = verified_invoice.id.clone();
        let funded_id = funded_invoice.id.clone();
        InvoiceStorage::store_invoice(&env, &verified_invoice);
        InvoiceStorage::store_invoice(&env, &funded_invoice);

        let investment = Investment {
            investment_id: BytesN::from_array(&env, &[11; 32]),
            invoice_id: funded_id.clone(),
            investor: investor.clone(),
            amount: 1_000,
            funded_at: now,
            status: InvestmentStatus::Active,
            insurance: Vec::new(&env),
        };
        InvestmentStorage::store_investment(&env, &investment);

        let escrow = Escrow {
            escrow_id: BytesN::from_array(&env, &[12; 32]),
            invoice_id: funded_id.clone(),
            investor: investor.clone(),
            business: business.clone(),
            amount: 1_000,
            currency: currency.clone(),
            created_at: now,
            status: EscrowStatus::Held,
        };
        EscrowStorage::store_escrow(&env, &escrow);

        let bid = Bid {
            bid_id: BytesN::from_array(&env, &[13; 32]),
            invoice_id: verified_id.clone(),
            investor: investor.clone(),
            bid_amount: 1_000,
            expected_return: 1_100,
            timestamp: now,
            status: BidStatus::Placed,
            expiration_timestamp: now + 86_400,
        };
        BidStorage::store_bid(&env, &bid);
        (verified_id, funded_id, investment)
    });
    let missing_id = BytesN::from_array(&env, &[0; 32]);
    let nonce = BytesN::from_array(&env, &[14; 32]);
    let short_text = String::from_str(&env, "auth matrix");

    env.set_auths(&[]);
    let mut failures = alloc::vec::Vec::<StdString>::new();

    macro_rules! auth {
        ($name:literal, $call:expr) => {
            {
                let result = $call;
                if !matches!(&result, Err(Err(_))) {
                    failures.push(alloc::format!(
                        "{} must fail with a host authorization error when no auth is provided; got {:?}",
                        $name,
                        result
                    ));
                }
            }
        };
    }
    macro_rules! read_only {
        ($name:literal, $call:expr) => {
            {
                let result = $call;
                if !result.is_ok() {
                    failures.push(alloc::format!(
                        "read-only entrypoint {} must be callable without auth; got {:?}",
                        $name,
                        result
                    ));
                }
            }
        };
    }

    auth!("set_admin", client.try_set_admin(&admin));
    read_only!("get_admin", client.try_get_admin());
    auth!("initialize_fee_system", client.try_initialize_fee_system(&admin));
    auth!("update_platform_fee_bps", client.try_update_platform_fee_bps(&100));
    read_only!("get_platform_fee_config", client.try_get_platform_fee_config());
    read_only!("calculate_profit", client.try_calculate_profit(&1_000, &1_100));
    auth!("invariant_self_check", client.try_invariant_self_check(&admin));
    auth!(
        "initialize_protocol_limits",
        client.try_initialize_protocol_limits(&admin)
    );
    auth!("pause", client.try_pause(&admin));
    auth!("unpause", client.try_unpause(&admin));
    read_only!("is_paused", client.try_is_paused());
    auth!(
        "submit_kyc_application",
        client.try_submit_kyc_application(&business, &short_text)
    );
    auth!(
        "verify_business",
        client.try_verify_business(&admin, &business)
    );
    auth!(
        "submit_investor_kyc",
        client.try_submit_investor_kyc(&investor, &short_text)
    );
    auth!("verify_investor", client.try_verify_investor(&investor, &1_000));
    auth!(
        "store_invoice",
        client.try_store_invoice(
            &business,
            &1_000,
            &currency,
            &(now + 86_400),
            &short_text,
            &InvoiceCategory::Services,
            &Vec::new(&env),
            &None,
        )
    );
    read_only!("get_invoice", client.try_get_invoice(&missing_id));
    auth!("verify_invoice", client.try_verify_invoice(&verified_id));
    auth!(
        "update_invoice_status",
        client.try_update_invoice_status(&verified_id, &InvoiceStatus::Verified)
    );
    auth!("cancel_invoice", client.try_cancel_invoice(&verified_id));
    read_only!("get_available_invoices", client.try_get_available_invoices());
    read_only!(
        "get_business_invoices",
        client.try_get_business_invoices(&business)
    );
    auth!(
        "place_bid",
        client.try_place_bid(&investor, &verified_id, &1_000, &1_100, &nonce)
    );
    auth!(
        "accept_bid",
        client.try_accept_bid(&verified_id, &BytesN::from_array(&env, &[13; 32]))
    );
    read_only!("get_bid", client.try_get_bid(&missing_id));
    read_only!(
        "get_bids_for_invoice",
        client.try_get_bids_for_invoice(&verified_id)
    );
    read_only!(
        "get_escrow_details",
        client.try_get_escrow_details(&funded_id)
    );
    auth!(
        "release_escrow",
        client.try_release_escrow(&funded_id, &business)
    );
    read_only!(
        "get_investment",
        client.try_get_investment(&funded_id)
    );
    read_only!(
        "get_invoice_investment",
        client.try_get_invoice_investment(&funded_id)
    );
    auth!(
        "process_partial_payment",
        client.try_process_partial_payment(&funded_id, &1, &short_text)
    );
    auth!(
        "settle_invoice",
        client.try_settle_invoice(&funded_id, &1, &investment)
    );
    read_only!(
        "get_invoice_progress",
        client.try_get_invoice_progress(&funded_id)
    );
    read_only!(
        "get_payment_count",
        client.try_get_payment_count(&funded_id)
    );
    read_only!(
        "get_payment_records",
        client.try_get_payment_records(&funded_id, &0, &10)
    );
    read_only!(
        "is_invoice_finalized",
        client.try_is_invoice_finalized(&funded_id)
    );
    read_only!(
        "get_settlement_batch_size",
        client.try_get_settlement_batch_size()
    );
    read_only!(
        "get_settlement_batch_size_max",
        client.try_get_settlement_batch_size_max()
    );
    auth!(
        "create_dispute",
        client.try_create_dispute(&verified_id, &business, &short_text, &short_text)
    );
    auth!(
        "put_dispute_under_review",
        client.try_put_dispute_under_review(&verified_id, &admin)
    );
    auth!(
        "resolve_dispute",
        client.try_resolve_dispute(&verified_id, &admin, &short_text)
    );
    auth!(
        "resolve_dispute_structured",
        client.try_resolve_dispute_structured(
            &verified_id,
            &admin,
            &DisputeResolution::Dismissed,
            &short_text
        )
    );
    read_only!(
        "get_dispute_details",
        client.try_get_dispute_details(&missing_id)
    );
    read_only!(
        "get_invoices_with_disputes",
        client.try_get_invoices_with_disputes()
    );
    read_only!(
        "get_invoices_by_dispute_status",
        client.try_get_invoices_by_dispute_status(&DisputeStatus::None)
    );
    read_only!(
        "get_dispute_timeline",
        client.try_get_dispute_timeline(&missing_id, &0, &10)
    );
    auth!(
        "update_dispute_evidence",
        client.try_update_dispute_evidence(&verified_id, &business, &short_text)
    );
    auth!(
        "register_arbiter",
        client.try_register_arbiter(&admin, &arbiter)
    );
    auth!(
        "unregister_arbiter",
        client.try_unregister_arbiter(&admin, &arbiter)
    );
    read_only!("list_arbiters", client.try_list_arbiters());
    read_only!("is_arbiter", client.try_is_arbiter(&arbiter));
    auth!(
        "create_backup",
        client.try_create_backup(&admin, &short_text)
    );
    auth!(
        "restore_backup",
        client.try_restore_backup(&admin, &missing_id)
    );
    auth!(
        "set_backup_retention_policy",
        client.try_set_backup_retention_policy(&admin, &10, &86_400, &true)
    );
    auth!(
        "archive_backup",
        client.try_archive_backup(&admin, &missing_id)
    );
    auth!("cleanup_backups", client.try_cleanup_backups(&admin));
    read_only!("get_backups", client.try_get_backups());
    read_only!(
        "get_backup_details",
        client.try_get_backup_details(&missing_id)
    );
    read_only!(
        "validate_backup",
        client.try_validate_backup(&missing_id)
    );
    read_only!(
        "preview_cleanup_backups",
        client.try_preview_cleanup_backups()
    );

    assert!(
        failures.is_empty(),
        "entrypoint authorization matrix failures:\n{}",
        failures.join("\n")
    );
}
