use crate::errors::QuickLendXError;
use crate::invoice::{DisputeStatus, InvoiceCategory};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

struct UnderReviewDispute {
    env: Env,
    client: QuickLendXContractClient<'static>,
    admin: Address,
    invoice_id: BytesN<32>,
}

fn setup_under_review_dispute() -> UnderReviewDispute {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 30 * 24 * 60 * 60;

    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));
    client.verify_business(&admin, &business);

    let invoice_id = client.store_invoice(
        &business,
        &100_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Dispute pause maintenance regression"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "Service delivery dispute"),
        &String::from_str(&env, "Evidence packet"),
    );
    client.put_dispute_under_review(&invoice_id, &admin);

    UnderReviewDispute {
        env,
        client,
        admin,
        invoice_id,
    }
}

fn assert_resolve_rejects_with(fx: &UnderReviewDispute, expected: QuickLendXError) {
    let result = fx.client.try_resolve_dispute(
        &fx.invoice_id,
        &fx.admin,
        &String::from_str(&fx.env, "Administrative dispute resolution"),
    );
    let err = result.unwrap_err().expect("expected contract error");
    assert_eq!(err, expected);

    let invoice = fx.client.get_invoice(&fx.invoice_id);
    assert_eq!(invoice.dispute_status, DisputeStatus::UnderReview);
}

fn assert_read_path_available(fx: &UnderReviewDispute) {
    let dispute = fx
        .client
        .get_dispute_details(&fx.invoice_id)
        .expect("read-only dispute details must remain available");
    assert_eq!(dispute.resolved_at, 0);
}

fn resolve_after_gates_clear(fx: &UnderReviewDispute) {
    fx.client.resolve_dispute(
        &fx.invoice_id,
        &fx.admin,
        &String::from_str(&fx.env, "Administrative dispute resolution"),
    );

    let invoice = fx.client.get_invoice(&fx.invoice_id);
    assert_eq!(invoice.dispute_status, DisputeStatus::Resolved);
}

#[test]
fn resolve_dispute_rejects_while_paused_but_reads_remain_available() {
    let fx = setup_under_review_dispute();

    fx.client.pause(&fx.admin);

    assert_resolve_rejects_with(&fx, QuickLendXError::ContractPaused);
    assert_read_path_available(&fx);

    fx.client.unpause(&fx.admin);
    resolve_after_gates_clear(&fx);
}

#[test]
fn resolve_dispute_rejects_during_maintenance_but_reads_remain_available() {
    let fx = setup_under_review_dispute();

    fx.client.set_maintenance_mode(
        &fx.admin,
        &true,
        &String::from_str(&fx.env, "Dispute resolution maintenance"),
    );

    assert_resolve_rejects_with(&fx, QuickLendXError::MaintenanceModeActive);
    assert_read_path_available(&fx);

    fx.client
        .set_maintenance_mode(&fx.admin, &false, &String::from_str(&fx.env, ""));
    resolve_after_gates_clear(&fx);
}

#[test]
fn resolve_dispute_rejects_when_pause_and_maintenance_are_both_active() {
    let fx = setup_under_review_dispute();

    fx.client.set_maintenance_mode(
        &fx.admin,
        &true,
        &String::from_str(&fx.env, "Incident maintenance"),
    );
    fx.client.pause(&fx.admin);

    assert_resolve_rejects_with(&fx, QuickLendXError::ContractPaused);
    assert_read_path_available(&fx);

    fx.client.unpause(&fx.admin);
    fx.client
        .set_maintenance_mode(&fx.admin, &false, &String::from_str(&fx.env, ""));
    resolve_after_gates_clear(&fx);
}
