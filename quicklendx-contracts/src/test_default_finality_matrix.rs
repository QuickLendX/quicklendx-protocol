use super::*;
use crate::errors::QuickLendXError;
use crate::payments::{EscrowStatus, EscrowStorage};
use crate::storage::InvoiceStorage;
use crate::types::InvoiceStatus;
use alloc::string::{String as AllocString, ToString};
use alloc::vec::Vec as AllocVec;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

const GRACE_PERIOD: u64 = 7 * 24 * 60 * 60;
const MATRIX_START: &str = "<!-- DEFAULT_FINALITY_MATRIX:START -->";
const MATRIX_END: &str = "<!-- DEFAULT_FINALITY_MATRIX:END -->";
const MATRIX_DOC: &str = include_str!("../../docs/default-finality-matrix.md");
const DEFAULT_HANDLING_DOC: &str = include_str!("../../docs/contracts/default-handling.md");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MatrixEscrowStatus {
    Held,
    Released,
    Refunded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MatrixOutcome {
    Allow,
    Deny(QuickLendXError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MatrixCase {
    invoice_status: InvoiceStatus,
    settlement_finalized: bool,
    escrow_status: MatrixEscrowStatus,
    expected: MatrixOutcome,
}

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
}

fn create_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "kyc"));
    client.verify_business(admin, &business);
    business
}

fn create_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    _admin: &Address,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "kyc"));
    client.verify_investor(&investor, &limit);
    investor
}

fn create_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> (BytesN<32>, Address, Address, i128, Address) {
    let business = create_verified_business(env, client, admin);
    let investor = create_verified_investor(env, client, admin, 100_000);

    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let token_client = token::Client::new(env, &currency);

    client.add_currency(admin, &currency);

    let initial_balance = 100_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);

    let expiry = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &client.address, &initial_balance, &expiry);
    token_client.approve(&investor, &client.address, &initial_balance, &expiry);

    let amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Default matrix invoice"),
        &crate::invoice::InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &amount,
        &(amount + 1_000),
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    client.accept_bid(&invoice_id, &bid_id);

    (invoice_id, business, investor, amount, currency)
}

fn extract_matrix_block(doc: &str) -> AllocString {
    let start = doc.find(MATRIX_START).expect("matrix start marker missing");
    let end = doc.find(MATRIX_END).expect("matrix end marker missing");
    let body = &doc[start + MATRIX_START.len()..end];

    let mut lines = AllocVec::<AllocString>::new();
    for line in body.lines() {
        let trimmed = line.trim_end();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
        }
    }

    let mut block = AllocString::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            block.push('\n');
        }
        block.push_str(line);
    }

    block
}

fn parse_matrix_cases(doc: &str) -> AllocVec<MatrixCase> {
    let block = extract_matrix_block(doc);
    let mut cases = AllocVec::new();

    for line in block.lines() {
        if !line.starts_with('|') {
            continue;
        }
        if line.contains("Invoice status") || line.contains("---") {
            continue;
        }

        let columns = line
            .split('|')
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .collect::<AllocVec<_>>();

        if columns.len() < 4 {
            continue;
        }

        cases.push(MatrixCase {
            invoice_status: parse_invoice_status(columns[0]),
            settlement_finalized: parse_bool(columns[1]),
            escrow_status: parse_escrow_status(columns[2]),
            expected: parse_outcome(columns[3]),
        });
    }

    cases
}

fn parse_invoice_status(value: &str) -> InvoiceStatus {
    match value {
        "Pending" => InvoiceStatus::Pending,
        "Verified" => InvoiceStatus::Verified,
        "Funded" => InvoiceStatus::Funded,
        "Paid" => InvoiceStatus::Paid,
        "Defaulted" => InvoiceStatus::Defaulted,
        "Cancelled" => InvoiceStatus::Cancelled,
        "Refunded" => InvoiceStatus::Refunded,
        _ => panic!("unknown invoice status in matrix: {value}"),
    }
}

fn parse_bool(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        _ => panic!("unknown bool in matrix: {value}"),
    }
}

fn parse_escrow_status(value: &str) -> MatrixEscrowStatus {
    match value {
        "Held" => MatrixEscrowStatus::Held,
        "Released" => MatrixEscrowStatus::Released,
        "Refunded" => MatrixEscrowStatus::Refunded,
        _ => panic!("unknown escrow status in matrix: {value}"),
    }
}

fn parse_outcome(value: &str) -> MatrixOutcome {
    match value {
        "Allow" => MatrixOutcome::Allow,
        "Deny: InvoiceAlreadyDefaulted" => {
            MatrixOutcome::Deny(QuickLendXError::InvoiceAlreadyDefaulted)
        }
        "Deny: InvoiceNotAvailableForFunding" => {
            MatrixOutcome::Deny(QuickLendXError::InvoiceNotAvailableForFunding)
        }
        "Deny: InvalidStatus" => MatrixOutcome::Deny(QuickLendXError::InvalidStatus),
        _ => panic!("unknown outcome in matrix: {value}"),
    }
}

fn is_invoice_finalized_via_module(
    env: &Env,
    contract_id: &Address,
    invoice_id: &BytesN<32>,
) -> bool {
    env.as_contract(contract_id, || {
        crate::settlement::is_invoice_finalized(env, invoice_id)
            .expect("finalization lookup should succeed")
    })
}

fn set_invoice_status_for_case(
    env: &Env,
    contract_id: &Address,
    invoice_id: &BytesN<32>,
    status: InvoiceStatus,
    investor: &Address,
    amount: i128,
) {
    env.as_contract(contract_id, || {
        let mut invoice =
            InvoiceStorage::get_invoice(env, invoice_id).expect("invoice should exist");
        let previous_status = invoice.status;

        if previous_status != status {
            InvoiceStorage::remove_from_status_invoices(env, previous_status, invoice_id);
        }

        match status {
            InvoiceStatus::Pending => {
                invoice.status = InvoiceStatus::Pending;
                invoice.funded_amount = 0;
                invoice.funded_at = None;
                invoice.investor = None;
                invoice.total_paid = 0;
                invoice.settled_at = None;
                invoice.payment_history = Vec::new(env);
            }
            InvoiceStatus::Verified => {
                invoice.status = InvoiceStatus::Verified;
                invoice.funded_amount = 0;
                invoice.funded_at = None;
                invoice.investor = None;
                invoice.total_paid = 0;
                invoice.settled_at = None;
                invoice.payment_history = Vec::new(env);
            }
            InvoiceStatus::Funded => {
                invoice.status = InvoiceStatus::Funded;
                invoice.funded_amount = amount;
                invoice.funded_at = Some(env.ledger().timestamp());
                invoice.investor = Some(investor.clone());
                invoice.total_paid = 0;
                invoice.settled_at = None;
                invoice.payment_history = Vec::new(env);
            }
            InvoiceStatus::Paid => {
                invoice.status = InvoiceStatus::Paid;
                invoice.funded_amount = amount;
                invoice.funded_at = Some(env.ledger().timestamp());
                invoice.investor = Some(investor.clone());
                invoice.total_paid = amount;
                invoice.settled_at = Some(env.ledger().timestamp());
            }
            InvoiceStatus::Defaulted => {
                invoice.status = InvoiceStatus::Defaulted;
                invoice.funded_amount = amount;
                invoice.funded_at = Some(env.ledger().timestamp());
                invoice.investor = Some(investor.clone());
                invoice.total_paid = 0;
                invoice.settled_at = None;
                invoice.payment_history = Vec::new(env);
            }
            InvoiceStatus::Cancelled => {
                invoice.status = InvoiceStatus::Cancelled;
                invoice.funded_amount = 0;
                invoice.funded_at = None;
                invoice.investor = None;
                invoice.total_paid = 0;
                invoice.settled_at = None;
                invoice.payment_history = Vec::new(env);
            }
            InvoiceStatus::Refunded => {
                invoice.status = InvoiceStatus::Refunded;
                invoice.funded_amount = 0;
                invoice.funded_at = None;
                invoice.investor = None;
                invoice.total_paid = 0;
                invoice.settled_at = None;
                invoice.payment_history = Vec::new(env);
            }
        }

        InvoiceStorage::update_invoice(env, &invoice);

        if previous_status != status {
            InvoiceStorage::add_to_status_invoices(env, status, invoice_id);
        }
    });
}

fn set_escrow_status_for_case(
    env: &Env,
    contract_id: &Address,
    invoice_id: &BytesN<32>,
    escrow_status: MatrixEscrowStatus,
) {
    env.as_contract(contract_id, || {
        let mut escrow = EscrowStorage::get_escrow_by_invoice(env, invoice_id)
            .expect("escrow should exist for matrix setup");
        escrow.status = match escrow_status {
            MatrixEscrowStatus::Held => EscrowStatus::Held,
            MatrixEscrowStatus::Released => EscrowStatus::Released,
            MatrixEscrowStatus::Refunded => EscrowStatus::Refunded,
        };
        EscrowStorage::update_escrow(env, &escrow);
    });
}

fn prepare_case(
    env: &Env,
    client: &QuickLendXContractClient,
    invoice_id: &BytesN<32>,
    investor: &Address,
    amount: i128,
    case: MatrixCase,
) {
    if case.settlement_finalized {
        client.settle_invoice(invoice_id, &amount);
    }

    set_invoice_status_for_case(
        env,
        &client.address,
        invoice_id,
        case.invoice_status,
        investor,
        amount,
    );
    set_escrow_status_for_case(env, &client.address, invoice_id, case.escrow_status);

    let invoice = client.get_invoice(invoice_id);
    env.ledger()
        .set_timestamp(invoice.due_date + GRACE_PERIOD + 1);
}

#[test]
fn test_default_finality_matrix_docs_stay_in_sync() {
    let matrix_block = extract_matrix_block(MATRIX_DOC);
    let default_handling_block = extract_matrix_block(DEFAULT_HANDLING_DOC);

    assert_eq!(
        matrix_block, default_handling_block,
        "default finality matrix drifted between docs/default-finality-matrix.md and docs/contracts/default-handling.md"
    );
}

#[test]
fn test_default_finality_matrix_matches_contract_behavior() {
    let cases = parse_matrix_cases(MATRIX_DOC);
    assert_eq!(
        cases.len(),
        42,
        "matrix must cover every status/finality/escrow combination"
    );

    for case in cases {
        let (env, client, admin) = setup();
        let (invoice_id, _business, investor, amount, _currency) =
            create_funded_invoice(&env, &client, &admin);

        prepare_case(&env, &client, &invoice_id, &investor, amount, case);

        let invoice_before = client.get_invoice(&invoice_id);
        let finalized_before = is_invoice_finalized_via_module(&env, &client.address, &invoice_id);
        let escrow_before = client.get_escrow_status(&invoice_id);

        let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(GRACE_PERIOD));

        match case.expected {
            MatrixOutcome::Allow => {
                assert!(
                    result.is_ok(),
                    "expected default to succeed for {:?}, got {:?}",
                    case,
                    result
                );

                let invoice_after = client.get_invoice(&invoice_id);
                let investment_after = client.get_invoice_investment(&invoice_id);
                let escrow_after = client.get_escrow_status(&invoice_id);

                assert_eq!(invoice_after.status, InvoiceStatus::Defaulted);
                assert_eq!(
                    investment_after.status,
                    crate::investment::InvestmentStatus::Defaulted,
                    "successful default must move the linked investment into Defaulted"
                );
                assert_eq!(
                    escrow_after,
                    EscrowStatus::Held,
                    "defaulting must not release or refund escrow as a side effect"
                );
                assert!(
                    !finalized_before,
                    "only non-finalized funded rows may succeed"
                );
            }
            MatrixOutcome::Deny(expected_error) => {
                assert!(
                    matches!(result, Err(Ok(actual_error)) if actual_error == expected_error),
                    "unexpected outcome for {:?}: {:?}",
                    case,
                    result
                );

                let invoice_after = client.get_invoice(&invoice_id);
                let finalized_after =
                    is_invoice_finalized_via_module(&env, &client.address, &invoice_id);
                let escrow_after = client.get_escrow_status(&invoice_id);

                assert_eq!(
                    invoice_after.status, invoice_before.status,
                    "blocked default attempts must leave invoice status untouched"
                );
                assert_eq!(
                    finalized_after, finalized_before,
                    "blocked default attempts must not mutate settlement finality"
                );
                assert_eq!(
                    escrow_after, escrow_before,
                    "blocked default attempts must not mutate escrow state"
                );
            }
        }
    }
}

#[test]
fn test_default_finality_matrix_preserves_duplicate_default_guard() {
    let (env, client, admin) = setup();
    let (invoice_id, _business, _investor, _amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    let invoice = client.get_invoice(&invoice_id);
    env.ledger()
        .set_timestamp(invoice.due_date + GRACE_PERIOD + 1);

    client.mark_invoice_defaulted(&invoice_id, &Some(GRACE_PERIOD));

    let retry = client.try_mark_invoice_defaulted(&invoice_id, &Some(GRACE_PERIOD));
    assert!(
        matches!(retry, Err(Ok(QuickLendXError::DuplicateDefaultTransition))),
        "double-default retry must be blocked by the transition guard to avoid duplicate finality side effects"
    );
}
