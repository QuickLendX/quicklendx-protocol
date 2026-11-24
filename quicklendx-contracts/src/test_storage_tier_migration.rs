//! Storage-tier regression tests against the production modules (issue #3065).
//!
//! These tests intentionally call the real FeeManager, NotificationSystem,
//! dispute index, and StorageMigration APIs. The former file contained a
//! parallel test-only implementation of those systems; its tests could pass
//! while production code regressed, so that duplicate harness was removed.

#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, Map, String,
};

use crate::analytics::{AnalyticsStorage, BusinessReport, TimePeriod};
use crate::dispute::{add_to_dispute_index, get_dispute_index, get_invoices_with_disputes_paged};
use crate::fees::{FeeManager, FeeType, VolumeTier};
use crate::types::InvoiceCategory;
use crate::notifications::{
    DataKey as NotificationDataKey, NotificationPriority, NotificationSystem, NotificationType,
};
use crate::storage::StorageMigration;
use crate::QuickLendXContract;

fn setup() -> (Env, Address) {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    env.mock_all_auths();
    (env, contract_id)
}

#[test]
fn user_volume_is_written_to_persistent_storage() {
    let (env, contract_id) = setup();

    for _ in 0..100 {
        let user = Address::generate(&env);
        env.as_contract(&contract_id, || {
            let updated = FeeManager::update_user_volume(&env, &user, 1_000).unwrap();
            assert_eq!(updated.total_volume, 1_000);
            assert_eq!(updated.transaction_count, 1);
            assert_eq!(updated.current_tier, VolumeTier::Standard);

            let key = (symbol_short!("volume"), user.clone());
            assert!(env.storage().persistent().has(&key));
            assert!(!env.storage().instance().has(&key));
        });
    }
}

#[test]
fn notification_idempotency_marker_uses_temporary_storage_and_expires() {
    let (env, contract_id) = setup();
    let recipient = Address::generate(&env);
    env.ledger().set_sequence_number(100);
    env.ledger().set_timestamp(1_000);

    let notification_id = env.as_contract(&contract_id, || {
        NotificationSystem::create_notification(
            &env,
            recipient.clone(),
            NotificationType::PaymentReceived,
            NotificationPriority::High,
            String::from_str(&env, "Payment received"),
            String::from_str(&env, "Your payment was received"),
            None,
        )
        .expect("production notification creation should succeed")
    });

    env.as_contract(&contract_id, || {
        let notification = NotificationSystem::get_notification(&env, &notification_id)
            .expect("notification body must be persisted");
        let marker = NotificationDataKey::IdempotencyKey(notification.idempotency_key);
        assert!(env.storage().temporary().has(&marker));
        assert!(!env.storage().instance().has(&marker));
        assert!(
            env.storage()
                .persistent()
                .has(&NotificationDataKey::Notification(notification_id.clone()))
        );
    });

    // The production constant is a 1,000-ledger temporary-storage TTL.
    env.ledger().set_sequence_number(1_110);
    env.as_contract(&contract_id, || {
        let notification = NotificationSystem::get_notification(&env, &notification_id)
            .expect("persistent notification body remains available");
        let marker = NotificationDataKey::IdempotencyKey(notification.idempotency_key);
        assert!(
            !env.storage().temporary().has(&marker),
            "idempotency marker should expire after its temporary TTL"
        );
    });
}

#[test]
fn production_notification_pagination_returns_all_records_once() {
    let (env, contract_id) = setup();
    let recipient = Address::generate(&env);

    for i in 0..120u32 {
        env.ledger().set_sequence_number(1_000 + i);
        env.ledger().set_timestamp(10_000 + u64::from(i));
        env.as_contract(&contract_id, || {
            NotificationSystem::create_notification(
                &env,
                recipient.clone(),
                NotificationType::PaymentReceived,
                NotificationPriority::High,
                String::from_str(&env, "Payment received"),
                String::from_str(&env, "Payment details"),
                None,
            )
            .expect("each distinct ledger event should create a notification");
        });
    }

    env.as_contract(&contract_id, || {
        let page1 = NotificationSystem::get_user_notifications_paged(&env, &recipient, 0, 50);
        let page2 = NotificationSystem::get_user_notifications_paged(&env, &recipient, 50, 50);
        let page3 = NotificationSystem::get_user_notifications_paged(&env, &recipient, 100, 50);

        assert_eq!(page1.len(), 50);
        assert_eq!(page2.len(), 50);
        assert_eq!(page3.len(), 20);

        for id in page1.iter() {
            for other in page2.iter() {
                assert_ne!(id, other);
            }
            for other in page3.iter() {
                assert_ne!(id, other);
            }
        }
        for id in page2.iter() {
            for other in page3.iter() {
                assert_ne!(id, other);
            }
        }
        assert_eq!(
            NotificationSystem::get_user_notifications(&env, &recipient).len(),
            120
        );
    });
}

#[test]
fn dispute_index_is_persistent_deduplicated_and_paged() {
    let (env, contract_id) = setup();
    env.as_contract(&contract_id, || {
        for i in 0..120u32 {
            let mut bytes = [0u8; 32];
            bytes[28..32].copy_from_slice(&i.to_be_bytes());
            let id = BytesN::from_array(&env, &bytes);
            add_to_dispute_index(&env, &id);
        }

        let mut duplicate = [0u8; 32];
        duplicate[28..32].copy_from_slice(&0u32.to_be_bytes());
        add_to_dispute_index(&env, &BytesN::from_array(&env, &duplicate));

        let key = symbol_short!("dispute");
        assert!(env.storage().persistent().has(&key));
        assert!(!env.storage().instance().has(&key));
        assert_eq!(get_dispute_index(&env).len(), 120);

        let page1 = get_invoices_with_disputes_paged(&env, 0, 50);
        let page2 = get_invoices_with_disputes_paged(&env, 50, 50);
        let page3 = get_invoices_with_disputes_paged(&env, 100, 50);
        assert_eq!(page1.len(), 50);
        assert_eq!(page2.len(), 50);
        assert_eq!(page3.len(), 20);
    });
}

#[test]
fn migration_progress_uses_declared_storage_keys_and_clears_on_commit() {
    let (env, contract_id) = setup();
    let admin = Address::generate(&env);

    env.as_contract(&contract_id, || {
        StorageMigration::begin_migration(&env, &admin, 0, 1)
            .expect("fresh schema should begin migration");

        assert_eq!(
            StorageMigration::get_pending_migration_version(&env),
            Some(1)
        );
        assert!(env.storage().instance().has(&symbol_short!("mig_ver")));
        assert!(env.storage().persistent().has(&symbol_short!("mig_off")));
        assert!(env.storage().persistent().has(&symbol_short!("mig_rec")));

        StorageMigration::advance_migration_page(&env, 25, 25);
        assert_eq!(StorageMigration::get_migration_offset(&env), 25);
        assert_eq!(StorageMigration::get_migration_records_migrated(&env), 25);

        StorageMigration::commit_migration(&env, &admin).unwrap();
        assert_eq!(StorageMigration::get_schema_version(&env), 1);
        assert!(!StorageMigration::is_migration_in_progress(&env));
        assert!(!env.storage().instance().has(&symbol_short!("mig_ver")));
        assert!(!env.storage().persistent().has(&symbol_short!("mig_off")));
        assert!(!env.storage().persistent().has(&symbol_short!("mig_rec")));
    });
}

#[test]
fn production_fee_revenue_and_business_reports_are_persistent() {
    let (env, contract_id) = setup();
    let user = Address::generate(&env);
    env.ledger().set_timestamp(2_592_000);
    let period = 1u64;

    env.as_contract(&contract_id, || {
        let mut fees = Map::new(&env);
        fees.set(FeeType::Platform, 250);
        FeeManager::collect_fees(&env, &user, fees, 250).unwrap();

        let analytics = FeeManager::get_analytics(&env, period).unwrap();
        assert_eq!(analytics.total_fees, 250);
        assert_eq!(analytics.total_transactions, 1);
        assert_eq!(FeeManager::get_user_volume(&env, &user).total_volume, 250);

        let mut report_ids = soroban_sdk::Vec::new(&env);
        for i in 0..10u32 {
            let mut bytes = [0u8; 32];
            bytes[28..32].copy_from_slice(&i.to_be_bytes());
            let report_id = BytesN::from_array(&env, &bytes);
            let report = BusinessReport {
                report_id: report_id.clone(),
                business_address: Address::generate(&env),
                period: TimePeriod::Monthly,
                start_date: 1,
                end_date: 2,
                invoices_uploaded: 5,
                invoices_funded: 3,
                total_volume: 50_000,
                average_funding_time: 60,
                success_rate: 80,
                default_rate: 5,
                category_breakdown: soroban_sdk::Vec::from_array(
                    &env,
                    [(InvoiceCategory::Services, 3)],
                ),
                rating_average: None,
                total_ratings: 0,
                generated_at: env.ledger().timestamp(),
            };
            AnalyticsStorage::store_business_report(&env, &report);
            report_ids.push_back(report_id);
        }

        for report_id in report_ids.iter() {
            let key = (symbol_short!("biz_rpt"), report_id.clone());
            assert!(env.storage().persistent().has(&key));
            assert!(!env.storage().instance().has(&key));
            assert_eq!(
                AnalyticsStorage::get_business_report(&env, &report_id)
                    .unwrap()
                    .invoices_funded,
                3
            );
        }
    });
}
