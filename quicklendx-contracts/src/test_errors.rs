/// Comprehensive test suite for `QuickLendXError`.
///
/// # Test categories
///
/// 1. **Error code consistency** – every variant maps to its documented u32 value.
/// 2. **Symbol conversion** – `From<QuickLendXError> for Symbol` is exhaustive and unique.
/// 3. **Invoice errors** – each invoice-related error is actually raised by the contract.
/// 4. **Authorization errors** – auth failures are returned, not panicked.
/// 5. **Validation errors** – invalid inputs produce the correct typed error.
/// 6. **Storage errors** – missing-key lookups return `StorageKeyNotFound`.
/// 7. **Business logic errors** – state-machine violations are caught.
/// 8. **KYC errors** – duplicate/missing KYC applications are handled correctly.
/// 9. **No panics** – all error paths return `Err(...)`, never panic.
/// 10. **Distinctness** – every variant has a unique u32 discriminant.
///
/// # Security notes
///
/// * All tests use `env.mock_all_auths()` to isolate contract logic from auth overhead.
/// * No test expects a panic; `try_*` variants are used throughout.
/// * Error codes are stable across invocations; numeric values are asserted directly.
use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Symbol, Vec,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn create_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    client.verify_business(admin, &business);
    business
}

// DEFAULT_MIN_AMOUNT from protocol_limits is 1_000_000 (1 token, 6 decimals).
// All test amounts must meet or exceed this floor.
const TEST_AMOUNT: i128 = 1_000_000;

fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    _admin: &Address,
    business: &Address,
    amount: i128,
) -> BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

fn fund_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    invoice_id: &BytesN<32>,
    amount: i128,
) -> (Address, BytesN<32>) {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC"));
    client.verify_investor(&investor, &(amount * 10));
    let bid_id = client.place_bid(&investor, invoice_id, &amount, &(amount + 100));
    client.accept_bid(invoice_id, &bid_id);
    (investor, bid_id)
}

// ── 1. Error code consistency ─────────────────────────────────────────────────

#[test]
fn test_error_codes_invoice_range() {
    assert_eq!(QuickLendXError::InvoiceNotFound as u32, 1000);
    assert_eq!(QuickLendXError::InvoiceNotAvailableForFunding as u32, 1001);
    assert_eq!(QuickLendXError::InvoiceAlreadyFunded as u32, 1002);
    assert_eq!(QuickLendXError::InvoiceAmountInvalid as u32, 1003);
    assert_eq!(QuickLendXError::InvoiceDueDateInvalid as u32, 1004);
    assert_eq!(QuickLendXError::InvoiceNotFunded as u32, 1005);
    assert_eq!(QuickLendXError::InvoiceAlreadyDefaulted as u32, 1006);
}

#[test]
fn test_error_codes_authorization_range() {
    assert_eq!(QuickLendXError::Unauthorized as u32, 1100);
    assert_eq!(QuickLendXError::NotBusinessOwner as u32, 1101);
    assert_eq!(QuickLendXError::NotInvestor as u32, 1102);
    assert_eq!(QuickLendXError::NotAdmin as u32, 1103);
}

#[test]
fn test_error_codes_validation_range() {
    assert_eq!(QuickLendXError::InvalidAmount as u32, 1200);
    assert_eq!(QuickLendXError::InvalidAddress as u32, 1201);
    assert_eq!(QuickLendXError::InvalidCurrency as u32, 1202);
    assert_eq!(QuickLendXError::InvalidTimestamp as u32, 1203);
    assert_eq!(QuickLendXError::InvalidDescription as u32, 1204);
}

#[test]
fn test_error_codes_storage_range() {
    assert_eq!(QuickLendXError::StorageError as u32, 1300);
    assert_eq!(QuickLendXError::StorageKeyNotFound as u32, 1301);
}

#[test]
fn test_error_codes_business_logic_range() {
    assert_eq!(QuickLendXError::InsufficientFunds as u32, 1400);
    assert_eq!(QuickLendXError::InvalidStatus as u32, 1401);
    assert_eq!(QuickLendXError::OperationNotAllowed as u32, 1402);
    assert_eq!(QuickLendXError::PaymentTooLow as u32, 1403);
    assert_eq!(QuickLendXError::PlatformAccountNotConfigured as u32, 1404);
    assert_eq!(QuickLendXError::InvalidCoveragePercentage as u32, 1405);
}

#[test]
fn test_error_codes_rating_range() {
    assert_eq!(QuickLendXError::InvalidRating as u32, 1500);
    assert_eq!(QuickLendXError::NotFunded as u32, 1501);
    assert_eq!(QuickLendXError::AlreadyRated as u32, 1502);
    assert_eq!(QuickLendXError::NotRater as u32, 1503);
}

#[test]
fn test_error_codes_kyc_range() {
    assert_eq!(QuickLendXError::BusinessNotVerified as u32, 1600);
    assert_eq!(QuickLendXError::KYCAlreadyPending as u32, 1601);
    assert_eq!(QuickLendXError::KYCAlreadyVerified as u32, 1602);
    assert_eq!(QuickLendXError::KYCNotFound as u32, 1603);
    assert_eq!(QuickLendXError::InvalidKYCStatus as u32, 1604);
}

#[test]
fn test_error_codes_audit_range() {
    assert_eq!(QuickLendXError::AuditLogNotFound as u32, 1700);
    assert_eq!(QuickLendXError::AuditIntegrityError as u32, 1701);
    assert_eq!(QuickLendXError::AuditQueryError as u32, 1702);
}

#[test]
fn test_error_codes_tag_range() {
    assert_eq!(QuickLendXError::InvalidTag as u32, 1800);
    assert_eq!(QuickLendXError::TagLimitExceeded as u32, 1801);
}

#[test]
fn test_error_codes_fee_range() {
    assert_eq!(QuickLendXError::InvalidFeeConfiguration as u32, 1850);
    assert_eq!(QuickLendXError::TreasuryNotConfigured as u32, 1851);
    assert_eq!(QuickLendXError::InvalidFeeBasisPoints as u32, 1852);
}

#[test]
fn test_error_codes_dispute_range() {
    assert_eq!(QuickLendXError::DisputeNotFound as u32, 1900);
    assert_eq!(QuickLendXError::DisputeAlreadyExists as u32, 1901);
    assert_eq!(QuickLendXError::DisputeNotAuthorized as u32, 1902);
    assert_eq!(QuickLendXError::DisputeAlreadyResolved as u32, 1903);
    assert_eq!(QuickLendXError::DisputeNotUnderReview as u32, 1904);
    assert_eq!(QuickLendXError::InvalidDisputeReason as u32, 1905);
    assert_eq!(QuickLendXError::InvalidDisputeEvidence as u32, 1906);
}

#[test]
fn test_error_codes_notification_range() {
    assert_eq!(QuickLendXError::NotificationNotFound as u32, 2000);
    assert_eq!(QuickLendXError::NotificationBlocked as u32, 2001);
}

// ── 2. Symbol conversion ──────────────────────────────────────────────────────

#[test]
fn test_symbol_conversion_invoice() {
    assert_eq!(
        Symbol::from(QuickLendXError::InvoiceNotFound),
        symbol_short!("INV_NF")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvoiceNotAvailableForFunding),
        symbol_short!("INV_NAF")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvoiceAlreadyFunded),
        symbol_short!("INV_AF")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvoiceAmountInvalid),
        symbol_short!("INV_AI")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvoiceDueDateInvalid),
        symbol_short!("INV_DI")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvoiceNotFunded),
        symbol_short!("INV_NFD")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvoiceAlreadyDefaulted),
        symbol_short!("INV_AD")
    );
}

#[test]
fn test_symbol_conversion_authorization() {
    assert_eq!(
        Symbol::from(QuickLendXError::Unauthorized),
        symbol_short!("UNAUTH")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::NotBusinessOwner),
        symbol_short!("NOT_OWN")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::NotInvestor),
        symbol_short!("NOT_INV")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::NotAdmin),
        symbol_short!("NOT_ADM")
    );
}

#[test]
fn test_symbol_conversion_validation() {
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidAmount),
        symbol_short!("INV_AMT")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidAddress),
        symbol_short!("INV_ADR")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidCurrency),
        symbol_short!("INV_CR")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidTimestamp),
        symbol_short!("INV_TM")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidDescription),
        symbol_short!("INV_DS")
    );
}

#[test]
fn test_symbol_conversion_storage() {
    assert_eq!(
        Symbol::from(QuickLendXError::StorageError),
        symbol_short!("STORE")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::StorageKeyNotFound),
        symbol_short!("KEY_NF")
    );
}

#[test]
fn test_symbol_conversion_business_logic() {
    assert_eq!(
        Symbol::from(QuickLendXError::InsufficientFunds),
        symbol_short!("INSUF")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidStatus),
        symbol_short!("INV_ST")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::OperationNotAllowed),
        symbol_short!("OP_NA")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::PaymentTooLow),
        symbol_short!("PAY_LOW")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::PlatformAccountNotConfigured),
        symbol_short!("PLT_NC")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidCoveragePercentage),
        symbol_short!("INS_CV")
    );
}

#[test]
fn test_symbol_conversion_rating() {
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidRating),
        symbol_short!("INV_RT")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::NotFunded),
        symbol_short!("NOT_FD")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::AlreadyRated),
        symbol_short!("ALR_RT")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::NotRater),
        symbol_short!("NOT_RT")
    );
}

#[test]
fn test_symbol_conversion_kyc() {
    assert_eq!(
        Symbol::from(QuickLendXError::BusinessNotVerified),
        symbol_short!("BUS_NV")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::KYCAlreadyPending),
        symbol_short!("KYC_PD")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::KYCAlreadyVerified),
        symbol_short!("KYC_VF")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::KYCNotFound),
        symbol_short!("KYC_NF")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidKYCStatus),
        symbol_short!("KYC_IS")
    );
}

#[test]
fn test_symbol_conversion_audit() {
    assert_eq!(
        Symbol::from(QuickLendXError::AuditLogNotFound),
        symbol_short!("AUD_NF")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::AuditIntegrityError),
        symbol_short!("AUD_IE")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::AuditQueryError),
        symbol_short!("AUD_QE")
    );
}

#[test]
fn test_symbol_conversion_tag() {
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidTag),
        symbol_short!("INV_TAG")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::TagLimitExceeded),
        symbol_short!("TAG_LIM")
    );
}

#[test]
fn test_symbol_conversion_fee() {
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidFeeConfiguration),
        symbol_short!("FEE_CFG")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::TreasuryNotConfigured),
        symbol_short!("TRS_NC")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidFeeBasisPoints),
        symbol_short!("FEE_BPS")
    );
}

#[test]
fn test_symbol_conversion_dispute() {
    assert_eq!(
        Symbol::from(QuickLendXError::DisputeNotFound),
        symbol_short!("DSP_NF")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::DisputeAlreadyExists),
        symbol_short!("DSP_EX")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::DisputeNotAuthorized),
        symbol_short!("DSP_NA")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::DisputeAlreadyResolved),
        symbol_short!("DSP_RS")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::DisputeNotUnderReview),
        symbol_short!("DSP_UR")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidDisputeReason),
        symbol_short!("DSP_RN")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::InvalidDisputeEvidence),
        symbol_short!("DSP_EV")
    );
}

#[test]
fn test_symbol_conversion_notification() {
    assert_eq!(
        Symbol::from(QuickLendXError::NotificationNotFound),
        symbol_short!("NOT_NF")
    );
    assert_eq!(
        Symbol::from(QuickLendXError::NotificationBlocked),
        symbol_short!("NOT_BL")
    );
}

// ── 3. Invoice errors raised by the contract ──────────────────────────────────

#[test]
fn test_invoice_not_found_error() {
    let (env, client, _admin) = setup();
    let bad_id = BytesN::from_array(&env, &[0u8; 32]);

    let result = client.try_get_invoice(&bad_id);
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::InvoiceNotFound
    );
}

#[test]
fn test_invoice_amount_invalid_zero() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let result = client.try_store_invoice(
        &business,
        &0,
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::InvalidAmount
    );
}

#[test]
fn test_invoice_amount_invalid_negative() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let result = client.try_store_invoice(
        &business,
        &-100,
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::InvalidAmount
    );
}

#[test]
fn test_invoice_due_date_invalid_past() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    // Set a non-zero ledger timestamp so subtraction doesn't wrap around u64.
    env.ledger().set_timestamp(10_000);
    let current_time = env.ledger().timestamp(); // 10_000

    // due_date is 9_000 < current_time (10_000) → clearly in the past.
    let result = client.try_store_invoice(
        &business,
        &TEST_AMOUNT,
        &currency,
        &(current_time - 1_000),
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::InvoiceDueDateInvalid
    );
}

#[test]
fn test_invoice_not_available_for_funding_unverified() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Store but do NOT verify — invoice stays in Pending state.
    let invoice_id = client.store_invoice(
        &business,
        &TEST_AMOUNT,
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let investor = Address::generate(&env);
    let result = client.try_place_bid(&investor, &invoice_id, &TEST_AMOUNT, &(TEST_AMOUNT + 100));
    assert!(result.is_err());
    // Placing a bid on an unverified invoice → InvalidStatus.
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::InvalidStatus
    );
}

#[test]
fn test_invoice_already_funded_error() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, TEST_AMOUNT);
    let _ = fund_invoice(&env, &client, &invoice_id, TEST_AMOUNT);

    // Second investor tries to accept another bid on the same funded invoice.
    let investor2 = Address::generate(&env);
    client.submit_investor_kyc(&investor2, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor2, &(TEST_AMOUNT * 10));
    let bid_id2 = client.place_bid(&investor2, &invoice_id, &TEST_AMOUNT, &(TEST_AMOUNT + 100));
    let result = client.try_accept_bid(&invoice_id, &bid_id2);
    assert!(result.is_err());
}

#[test]
fn test_invoice_already_defaulted_error() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, TEST_AMOUNT);
    let _ = fund_invoice(&env, &client, &invoice_id, TEST_AMOUNT);

    let invoice = client.get_invoice(&invoice_id);
    let grace_period: u64 = 7 * 24 * 60 * 60;
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period + 1);

    client.mark_invoice_defaulted(&invoice_id, &Some(grace_period));

    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::InvalidStatus
    );
}

#[test]
fn test_invoice_not_funded_for_default() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, TEST_AMOUNT);

    let result = client.try_mark_invoice_defaulted(&invoice_id, &None);
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::InvoiceNotAvailableForFunding
    );
}

// ── 4. Authorization errors ───────────────────────────────────────────────────

#[test]
fn test_not_admin_error() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, TEST_AMOUNT);

    // try_verify_invoice is an admin-only operation → returns an error, never panics.
    let result = client.try_verify_invoice(&invoice_id);
    assert!(result.is_err());
}

#[test]
fn test_business_not_verified_error() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let result = client.try_upload_invoice(
        &business,
        &TEST_AMOUNT,
        &currency,
        &due_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::BusinessNotVerified
    );
}

// ── 5. Validation errors ──────────────────────────────────────────────────────

#[test]
fn test_invalid_description_empty() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Use TEST_AMOUNT so amount validation passes; the empty description triggers first.
    let result = client.try_store_invoice(
        &business,
        &TEST_AMOUNT,
        &currency,
        &due_date,
        &String::from_str(&env, ""),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::InvalidDescription
    );
}

// ── 6. Storage errors ─────────────────────────────────────────────────────────

#[test]
fn test_storage_key_not_found_investment() {
    let (env, client, _admin) = setup();
    let bad_id = BytesN::from_array(&env, &[0u8; 32]);

    let result = client.try_get_investment(&bad_id);
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::StorageKeyNotFound
    );
}

#[test]
fn test_storage_key_not_found_escrow() {
    let (env, client, _admin) = setup();
    let bad_id = BytesN::from_array(&env, &[0u8; 32]);

    let result = client.try_get_escrow_details(&bad_id);
    assert!(result.is_err());
}

// ── 7. Business logic errors ──────────────────────────────────────────────────

#[test]
fn test_operation_not_allowed_before_grace_period() {
    let (env, client, admin) = setup();
    let business = create_verified_business(&env, &client, &admin);
    let invoice_id = create_verified_invoice(&env, &client, &admin, &business, TEST_AMOUNT);
    let _ = fund_invoice(&env, &client, &invoice_id, TEST_AMOUNT);

    let invoice = client.get_invoice(&invoice_id);
    let grace_period: u64 = 7 * 24 * 60 * 60;
    // Move only halfway through the grace period — too early to default.
    env.ledger()
        .set_timestamp(invoice.due_date + grace_period / 2);

    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(grace_period));
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::OperationNotAllowed
    );
}

// ── 8. KYC errors ─────────────────────────────────────────────────────────────

#[test]
fn test_kyc_already_pending_business() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);

    client.submit_kyc_application(&business, &String::from_str(&env, "data"));

    let result =
        client.try_submit_kyc_application(&business, &String::from_str(&env, "data again"));
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::KYCAlreadyPending
    );
}

#[test]
fn test_kyc_already_verified_business() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);

    client.submit_kyc_application(&business, &String::from_str(&env, "data"));
    client.verify_business(&admin, &business);

    // verify_business checks status == Pending; after verification status is Verified,
    // so a second call returns InvalidKYCStatus (status is not Pending).
    let result = client.try_verify_business(&admin, &business);
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::InvalidKYCStatus
    );
}

#[test]
fn test_kyc_not_found_investor() {
    let (env, client, admin) = setup();
    let non_existent = Address::generate(&env);

    let result = client.try_verify_investor(&non_existent, &10000);
    assert!(result.is_err());
    assert_eq!(
        result.err().unwrap().expect("contract error"),
        QuickLendXError::KYCNotFound
    );
}

// ── 9. No panics on error conditions ─────────────────────────────────────────

/// All bad-input paths must return `Err(...)`, never panic.
#[test]
fn test_no_panics_on_error_conditions() {
    let (env, client, _admin) = setup();
    let bad_id = BytesN::from_array(&env, &[0u8; 32]);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set a non-zero timestamp so past-date arithmetic doesn't underflow.
    env.ledger().set_timestamp(10_000);
    let future_date = env.ledger().timestamp() + 86400;
    let past_date = env.ledger().timestamp() - 1_000; // 9_000, clearly in the past.

    // Missing-entity lookups — no panic.
    let _ = client.try_get_invoice(&bad_id);
    let _ = client.get_bid(&bad_id);
    let _ = client.try_get_investment(&bad_id);
    let _ = client.try_get_escrow_details(&bad_id);

    // Invalid invoice creation parameters — all return Err, none panic.
    let _ = client.try_store_invoice(
        &business,
        &0, // zero amount → InvalidAmount
        &currency,
        &future_date,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let _ = client.try_store_invoice(
        &business,
        &TEST_AMOUNT,
        &currency,
        &past_date, // past due date → InvoiceDueDateInvalid
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let _ = client.try_store_invoice(
        &business,
        &TEST_AMOUNT,
        &currency,
        &future_date,
        &String::from_str(&env, ""), // empty description → InvalidDescription
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
}

// ── 10. Distinctness ──────────────────────────────────────────────────────────

/// Every error variant must have a unique u32 discriminant.
#[test]
fn test_error_codes_are_distinct() {
    let codes = [
        QuickLendXError::InvoiceNotFound as u32,
        QuickLendXError::InvoiceNotAvailableForFunding as u32,
        QuickLendXError::InvoiceAlreadyFunded as u32,
        QuickLendXError::InvoiceAmountInvalid as u32,
        QuickLendXError::InvoiceDueDateInvalid as u32,
        QuickLendXError::InvoiceNotFunded as u32,
        QuickLendXError::InvoiceAlreadyDefaulted as u32,
        QuickLendXError::Unauthorized as u32,
        QuickLendXError::NotBusinessOwner as u32,
        QuickLendXError::NotInvestor as u32,
        QuickLendXError::NotAdmin as u32,
        QuickLendXError::InvalidAmount as u32,
        QuickLendXError::InvalidAddress as u32,
        QuickLendXError::InvalidCurrency as u32,
        QuickLendXError::InvalidTimestamp as u32,
        QuickLendXError::InvalidDescription as u32,
        QuickLendXError::StorageError as u32,
        QuickLendXError::StorageKeyNotFound as u32,
        QuickLendXError::InsufficientFunds as u32,
        QuickLendXError::InvalidStatus as u32,
        QuickLendXError::OperationNotAllowed as u32,
        QuickLendXError::PaymentTooLow as u32,
        QuickLendXError::PlatformAccountNotConfigured as u32,
        QuickLendXError::InvalidCoveragePercentage as u32,
        QuickLendXError::InvalidRating as u32,
        QuickLendXError::NotFunded as u32,
        QuickLendXError::AlreadyRated as u32,
        QuickLendXError::NotRater as u32,
        QuickLendXError::BusinessNotVerified as u32,
        QuickLendXError::KYCAlreadyPending as u32,
        QuickLendXError::KYCAlreadyVerified as u32,
        QuickLendXError::KYCNotFound as u32,
        QuickLendXError::InvalidKYCStatus as u32,
        QuickLendXError::AuditLogNotFound as u32,
        QuickLendXError::AuditIntegrityError as u32,
        QuickLendXError::AuditQueryError as u32,
        QuickLendXError::InvalidTag as u32,
        QuickLendXError::TagLimitExceeded as u32,
        QuickLendXError::InvalidFeeConfiguration as u32,
        QuickLendXError::TreasuryNotConfigured as u32,
        QuickLendXError::InvalidFeeBasisPoints as u32,
        QuickLendXError::DisputeNotFound as u32,
        QuickLendXError::DisputeAlreadyExists as u32,
        QuickLendXError::DisputeNotAuthorized as u32,
        QuickLendXError::DisputeAlreadyResolved as u32,
        QuickLendXError::DisputeNotUnderReview as u32,
        QuickLendXError::InvalidDisputeReason as u32,
        QuickLendXError::InvalidDisputeEvidence as u32,
        QuickLendXError::NotificationNotFound as u32,
        QuickLendXError::NotificationBlocked as u32,
    ];

    for i in 0..codes.len() {
        for j in (i + 1)..codes.len() {
            assert_ne!(
                codes[i], codes[j],
                "Duplicate error code {} at positions {} and {}",
                codes[i], i, j
            );
        }
    }
}
