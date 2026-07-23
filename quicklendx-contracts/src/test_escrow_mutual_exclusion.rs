//! Regression proving refund_escrow_funds and release_escrow are mutually exclusive
//! under interleaved calls from different callers.
//!
//! Each scenario funds one invoice, then attempts both terminal operations in both
//! orders. The second call must always fail with InvalidStatus — no double-spend,
//! no silent no-op.

use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::EscrowStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

const AMOUNT: i128 = 100_000;
const TS: u64 = 2_000_000;

macro_rules! assert_invalid_status {
    ($result:expr) => {{
        let r = $result;
        assert!(
            matches!(&r, Err(Ok(QuickLendXError::InvalidStatus))),
            "expected InvalidStatus; got: {r:?}"
        );
    }};
}

struct Ctx {
    env: Env,
    client: QuickLendXContractClient<'static>,
    admin: Address,
    investor: Address,
    business: Address,
    currency: Address,
}

fn setup() -> (Ctx, BytesN<32>) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(TS);

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac = token::StellarAssetClient::new(&env, &currency);
    sac.mint(&investor, &(AMOUNT * 2));
    sac.mint(&contract_id, &AMOUNT);

    let token_client = token::Client::new(&env, &currency);
    token_client.approve(&investor, &contract_id, &AMOUNT, &(TS as u32 + 1000));

    client.initialize(&crate::init::InitializationParams {
        admin: admin.clone(),
        treasury: admin.clone(),
        fee_bps: 0,
        min_invoice_amount: 1,
        max_due_date_days: 365,
        max_bid_duration_days: 30,
        insurance_fee_bps: 0,
        protocol_version: 1,
    });

    let inv_id = client.create_invoice(&business, &String::from_str(&env, "INV-ME-1"), &AMOUNT, &currency, &(TS + 86400), &InvoiceCategory::Trade, &String::from_str(&env, "{}")).unwrap();
    client.verify_invoice(&admin, &inv_id).unwrap();

    let bid_id = client.submit_bid(&investor, &inv_id, &AMOUNT, &(TS + 3600)).unwrap();
    client.accept_bid_and_fund(&business, &inv_id, &bid_id).unwrap();

    let ctx = Ctx { env, client, admin, investor, business, currency };
    (ctx, inv_id)
}

/// release first, then refund must be rejected.
#[test]
fn release_then_refund_is_rejected() {
    let (ctx, inv_id) = setup();

    ctx.client.release_escrow_funds(&ctx.invoice_id_for_inv(&inv_id)).unwrap();

    let escrow = ctx.client.get_escrow_status(&inv_id).unwrap();
    assert_eq!(escrow, EscrowStatus::Released);

    assert_invalid_status!(ctx.client.refund_escrow_funds(&ctx.admin, &inv_id));
}

/// refund first, then release must be rejected.
#[test]
fn refund_then_release_is_rejected() {
    let (ctx, inv_id) = setup();

    ctx.client.refund_escrow_funds(&ctx.admin, &inv_id).unwrap();

    let escrow = ctx.client.get_escrow_status(&inv_id).unwrap();
    assert_eq!(escrow, EscrowStatus::Refunded);

    assert_invalid_status!(ctx.client.release_escrow_funds(&inv_id));
}

impl Ctx {
    fn invoice_id_for_inv(&self, inv_id: &BytesN<32>) -> BytesN<32> {
        inv_id.clone()
    }
}
