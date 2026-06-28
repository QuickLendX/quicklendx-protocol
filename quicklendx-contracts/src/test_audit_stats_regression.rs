#![cfg(test)]
use crate::audit::{log_operation, AuditOperation, AuditStorage};
use proptest::prelude::*;
use crate::QuickLendXContract;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

proptest! {
    #[test]
    fn test_audit_stats_counters_increment_exactly_once_per_event(
        count_a in 0u32..20,
        count_b in 0u32..20
    ) {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        let actor = Address::generate(&env);
        let invoice_id = BytesN::from_array(&env, &[1; 32]);

        let stats = env.as_contract(&contract_id, || {
            for _ in 0..count_a {
                log_operation(
                    &env,
                    invoice_id.clone(),
                    AuditOperation::InvoiceCreated,
                    actor.clone(),
                    None,
                    None,
                    None,
                    None,
                );
            }

            for _ in 0..count_b {
                log_operation(
                    &env,
                    invoice_id.clone(),
                    AuditOperation::InvoiceStatusChanged,
                    actor.clone(),
                    None,
                    None,
                    None,
                    None,
                );
            }

            AuditStorage::get_audit_stats(&env)
        });
        
        let mut found_a = 0;
        let mut found_b = 0;
        
        for (op, count) in stats.operations_count.into_iter() {
            if op == AuditOperation::InvoiceCreated {
                found_a = count;
            } else if op == AuditOperation::InvoiceStatusChanged {
                found_b = count;
            }
        }
        
        assert_eq!(found_a, count_a, "InvoiceCreated counter mismatch");
        assert_eq!(found_b, count_b, "InvoiceStatusChanged counter mismatch");
        assert_eq!(stats.total_entries, count_a + count_b, "Total entries mismatch");
    }
}
