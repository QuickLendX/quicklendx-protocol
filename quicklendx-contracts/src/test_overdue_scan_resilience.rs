use super::*;
use crate::defaults::scan_funded_invoice_expirations;
use crate::errors::QuickLendXError;
use crate::events::ScanItemFailed;
use crate::invoice::{Invoice, InvoiceCategory, InvoiceStatus};
use crate::payments::{Escrow, EscrowStatus, EscrowStorage};
use crate::storage::InvoiceStorage;
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    xdr, Address, BytesN, Env, String, Symbol, TryFromVal, Val, Vec,
};

fn setup() -> (Env, Address) {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    env.ledger().set_timestamp(1);
    (env, contract_id)
}

fn store_funded_invoice(env: &Env, due_date: u64) -> BytesN<32> {
    let mut invoice = Invoice::new(
        env,
        Address::generate(env),
        1_000,
        Address::generate(env),
        due_date,
        String::from_str(env, "Overdue scan test"),
        InvoiceCategory::Services,
        Vec::new(env),
    )
    .unwrap();
    invoice.status = InvoiceStatus::Funded;
    invoice.funded_amount = invoice.amount;
    InvoiceStorage::store(env, &invoice);
    invoice.id
}

fn store_released_escrow(env: &Env, invoice_id: &BytesN<32>) {
    EscrowStorage::store_escrow(
        env,
        &Escrow {
            escrow_id: BytesN::from_array(env, &[91; 32]),
            invoice_id: invoice_id.clone(),
            investor: Address::generate(env),
            business: Address::generate(env),
            amount: 1_000,
            currency: Address::generate(env),
            created_at: env.ledger().timestamp(),
            status: EscrowStatus::Released,
        },
    );
}

fn scan_failure_events(env: &Env) -> Vec<ScanItemFailed> {
    let topic = Symbol::new(env, "scan_item_failed");
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic).unwrap();
    let mut failures = Vec::new(env);

    for event in env.events().all().events().iter() {
        if let xdr::ContractEventBody::V0(body) = &event.body {
            if body.topics.first() == Some(&topic_xdr) {
                let data = Val::try_from_val(env, &body.data).unwrap();
                failures.push_back(ScanItemFailed::try_from_val(env, &data).unwrap());
            }
        }
    }
    failures
}

#[test]
fn scan_isolates_failures() {
    let (env, contract_id) = setup();
    let (poisoned_id, healthy_ids) = env.as_contract(&contract_id, || {
        let poisoned = store_funded_invoice(&env, 2);
        let healthy_b = store_funded_invoice(&env, 2);
        let healthy_c = store_funded_invoice(&env, 2);
        store_released_escrow(&env, &poisoned);
        (poisoned, [healthy_b, healthy_c])
    });
    env.ledger().set_timestamp(3);

    let result = env.as_contract(&contract_id, || {
        scan_funded_invoice_expirations(&env, 0, Some(10)).unwrap()
    });

    assert_eq!(result.failed_count, 1);
    assert_eq!(result.scanned_count, 3);
    env.as_contract(&contract_id, || {
        assert_eq!(
            InvoiceStorage::get_invoice(&env, &poisoned_id)
                .unwrap()
                .status,
            InvoiceStatus::Funded
        );
        for invoice_id in healthy_ids.iter() {
            assert_eq!(
                InvoiceStorage::get_invoice(&env, &invoice_id)
                    .unwrap()
                    .status,
                InvoiceStatus::Defaulted
            );
        }
    });
    let failures = scan_failure_events(&env);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures.get(0).unwrap().invoice_id, poisoned_id);
    assert_eq!(
        failures.get(0).unwrap().error_code,
        QuickLendXError::InvalidStatus as u32
    );
}

#[test]
fn scan_failure_does_not_pin_cursor() {
    let (env, contract_id) = setup();
    let (invoice_ids, poisoned_id) = env.as_contract(&contract_id, || {
        let ids = (0..5)
            .map(|_| store_funded_invoice(&env, 2))
            .collect::<alloc::vec::Vec<_>>();
        store_released_escrow(&env, &ids[0]);
        (ids, ids[0].clone())
    });
    env.ledger().set_timestamp(3);

    env.as_contract(&contract_id, || {
        for _ in 0..3 {
            scan_funded_invoice_expirations(&env, 0, Some(2)).unwrap();
        }
    });

    env.as_contract(&contract_id, || {
        assert_eq!(
            InvoiceStorage::get_invoice(&env, &poisoned_id)
                .unwrap()
                .status,
            InvoiceStatus::Funded
        );
        for invoice_id in invoice_ids.iter().skip(1) {
            assert_eq!(
                InvoiceStorage::get_invoice(&env, invoice_id)
                    .unwrap()
                    .status,
                InvoiceStatus::Defaulted
            );
        }
    });
}

#[test]
fn cursor_no_skip_after_defaults() {
    let (env, contract_id) = setup();
    let invoice_ids = env.as_contract(&contract_id, || {
        [
            store_funded_invoice(&env, 2),
            store_funded_invoice(&env, 2),
            store_funded_invoice(&env, 10),
            store_funded_invoice(&env, 10),
            store_funded_invoice(&env, 10),
        ]
    });
    env.ledger().set_timestamp(3);

    env.as_contract(&contract_id, || {
        let first = scan_funded_invoice_expirations(&env, 0, Some(2)).unwrap();
        assert_eq!(first.scanned_count, 2);
        assert_eq!(first.next_cursor, 0);
        assert_eq!(first.total_funded, 5);
    });

    env.ledger().set_timestamp(11);
    env.as_contract(&contract_id, || {
        for expected_id in invoice_ids.iter().skip(2) {
            let result = scan_funded_invoice_expirations(&env, 0, Some(1)).unwrap();
            assert_eq!(result.scanned_count, 1);
            assert_eq!(
                InvoiceStorage::get_invoice(&env, &expected_id)
                    .unwrap()
                    .status,
                InvoiceStatus::Defaulted
            );
        }
    });

    env.as_contract(&contract_id, || {
        for invoice_id in invoice_ids.iter() {
            assert_eq!(
                InvoiceStorage::get_invoice(&env, &invoice_id)
                    .unwrap()
                    .status,
                InvoiceStatus::Defaulted
            );
        }
    });
}

#[test]
fn scan_counts_deadline_overflow_as_item_failure() {
    let (env, contract_id) = setup();
    let invoice_id = env.as_contract(&contract_id, || {
        let id = store_funded_invoice(&env, 2);
        let mut invoice = InvoiceStorage::get_invoice(&env, &id).unwrap();
        invoice.due_date = u64::MAX;
        InvoiceStorage::update_invoice(&env, &invoice);
        id
    });
    env.ledger().set_timestamp(u64::MAX);

    let result = env.as_contract(&contract_id, || {
        scan_funded_invoice_expirations(&env, 1, Some(1)).unwrap()
    });

    assert_eq!(result.failed_count, 1);
    let failures = scan_failure_events(&env);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures.get(0).unwrap().invoice_id, invoice_id);
    assert_eq!(
        failures.get(0).unwrap().error_code,
        QuickLendXError::InvalidTimestamp as u32
    );
}

#[test]
fn scan_rejects_grace_period_above_maximum() {
    let (env, contract_id) = setup();
    let result = env.as_contract(&contract_id, || {
        scan_funded_invoice_expirations(&env, 30 * 24 * 60 * 60 + 1, Some(1))
    });

    assert_eq!(result, Err(QuickLendXError::InvalidTimestamp));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn overdue_scan_eventually_defaults_every_overdue_invoice(
        n in 1usize..=30,
        overdue_mask in prop::collection::vec(any::<bool>(), 30),
        limit in 1u32..=10,
    ) {
        let (env, contract_id) = setup();
        let invoice_ids = env.as_contract(&contract_id, || {
            let mut ids = Vec::new(&env);
            for index in 0..n {
                let due_date = if overdue_mask[index] { 2 } else { 100 };
                ids.push_back(store_funded_invoice(&env, due_date));
            }
            ids
        });
        env.ledger().set_timestamp(3);

        env.as_contract(&contract_id, || {
            let scans = (n + limit as usize - 1) / limit as usize + 1;
            for _ in 0..scans {
                scan_funded_invoice_expirations(&env, 0, Some(limit)).unwrap();
            }
            for (index, invoice_id) in invoice_ids.iter().enumerate() {
                let expected = if overdue_mask[index] {
                    InvoiceStatus::Defaulted
                } else {
                    InvoiceStatus::Funded
                };
                assert_eq!(
                    InvoiceStorage::get_invoice(&env, &invoice_id).unwrap().status,
                    expected
                );
            }
        });
    }
}
