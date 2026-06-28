/// Negative tests for confused-deputy / self-call prevention (issue #1575).
///
/// Threat model: on Soroban a cross-contract call can pass the callee's own
/// contract address as a user-controlled `Address` argument.  When the callee
/// subsequently calls `addr.require_auth()`, the host grants auth because the
/// contract is authorizing itself.  Without an explicit check, an attacker
/// could register the contract as a business or investor, bypassing KYC and
/// all access controls that gate financial operations.
///
/// The `require_not_self` guard added by this PR rejects any entrypoint call
/// where the supplied `Address` equals `env.current_contract_address()`, and
/// returns the typed `SelfCallNotAllowed` (1104) error — not a generic 500.
///
/// Each test below **would have passed before the fix** (no error returned)
/// and **now fails as expected** (SelfCallNotAllowed returned).
use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

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

/// Passing the contract's own address as `business` to `store_invoice` must
/// be rejected with `SelfCallNotAllowed`.
///
/// Before fix: contract would proceed (confused-deputy).
/// After fix:  returns Err(SelfCallNotAllowed).
#[test]
fn store_invoice_rejects_contract_as_business() {
    let (env, client, _admin) = setup();
    let self_addr = client.address.clone();
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    let err = client
        .try_store_invoice(
            &self_addr,
            &1_000i128,
            &currency,
            &due_date,
            &String::from_str(&env, "inv"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .unwrap_err()
        .expect("expected contract error");

    assert_eq!(err, QuickLendXError::SelfCallNotAllowed);
}

/// Passing the contract's own address as `business` to `submit_kyc_application`
/// must be rejected with `SelfCallNotAllowed`.
#[test]
fn submit_kyc_application_rejects_contract_as_business() {
    let (env, client, _admin) = setup();
    let self_addr = client.address.clone();

    let err = client
        .try_submit_kyc_application(&self_addr, &String::from_str(&env, "kyc"))
        .unwrap_err()
        .expect("expected contract error");

    assert_eq!(err, QuickLendXError::SelfCallNotAllowed);
}

/// Passing the contract's own address as `investor` to `submit_investor_kyc`
/// must be rejected with `SelfCallNotAllowed`.
#[test]
fn submit_investor_kyc_rejects_contract_as_investor() {
    let (env, client, _admin) = setup();
    let self_addr = client.address.clone();

    let err = client
        .try_submit_investor_kyc(&self_addr, &String::from_str(&env, "kyc"))
        .unwrap_err()
        .expect("expected contract error");

    assert_eq!(err, QuickLendXError::SelfCallNotAllowed);
}

/// Legitimate (non-self) callers are unaffected by the guard.
#[test]
fn legitimate_caller_passes_self_call_guard() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);

    // Should not return SelfCallNotAllowed — any other outcome is acceptable.
    let result = client.try_submit_kyc_application(&business, &String::from_str(&env, "kyc"));
    if let Err(e) = result {
        let err = e.expect("expected contract error");
        assert_ne!(
            err,
            QuickLendXError::SelfCallNotAllowed,
            "legitimate caller must not be blocked by self-call guard"
        );
    }
    let _ = admin; // suppress unused warning
}
