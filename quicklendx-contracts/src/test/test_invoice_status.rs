// quicklendx-contracts/src/test/test_invoice_status.rs

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{Env, Address, BytesN, String, Vec};
    use crate::invoice::Invoice;
    use crate::types::{InvoiceStatus, InvoiceCategory};
    use crate::errors::QuickLendXError;

    fn dummy_env() -> Env {
        Env::default()
    }

    fn dummy_address() -> Address {
        // Zero address for simplicity
        Address::from_str(&Env::default(), "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF").unwrap()
    }

    fn create_invoice(env: &Env, status: InvoiceStatus) -> Invoice {
        // Minimal fields for new invoice
        let business = dummy_address();
        let amount: i128 = 1_000_000; // 1 unit
        let currency = dummy_address();
        let due_date = env.ledger().timestamp() + 86400; // 1 day later
        let description = String::from_str(env, "test invoice");
        let category = InvoiceCategory::Services;
        let tags = Vec::new(env);
        let mut inv = Invoice::new(
            env,
            business,
            amount,
            currency,
            due_date,
            description,
            category,
            tags,
        None).expect("invoice creation should succeed");
        // Manually set status to the desired variant for testing
        inv.status = status;
        inv
    }

    #[test]
    fn test_invoice_status_is_terminal() {
        assert!(!InvoiceStatus::Pending.is_terminal());
        assert!(!InvoiceStatus::Verified.is_terminal());
        assert!(!InvoiceStatus::Funded.is_terminal());
        assert!(InvoiceStatus::Paid.is_terminal());
        assert!(InvoiceStatus::Defaulted.is_terminal());
        assert!(InvoiceStatus::Cancelled.is_terminal());
        assert!(InvoiceStatus::Refunded.is_terminal());
    }

    #[test]
    fn test_is_available_for_funding_logic() {
        let env = dummy_env();
        // Pending should not be available for funding
        let pending = create_invoice(&env, InvoiceStatus::Pending);
        assert!(!pending.is_available_for_funding());

        // Verified and no funding yet should be available
        let verified = create_invoice(&env, InvoiceStatus::Verified);
        assert!(verified.is_available_for_funding());

        // Funded should not be available
        let mut funded = create_invoice(&env, InvoiceStatus::Funded);
        funded.funded_amount = 500_000;
        funded.funded_at = Some(env.ledger().timestamp());
        funded.investor = Some(dummy_address());
        assert!(!funded.is_available_for_funding());
    }

    #[test]
    fn test_mark_as_funded_transitions_status() {
        let env = dummy_env();
        let mut inv = create_invoice(&env, InvoiceStatus::Verified);
        let investor = dummy_address();
        let amount: i128 = 500_000;
        let timestamp = env.ledger().timestamp();
        inv.mark_as_funded(&env, investor.clone(), amount, timestamp);
        assert_eq!(inv.status, InvoiceStatus::Funded);
        assert_eq!(inv.funded_amount, amount);
        assert_eq!(inv.funded_at, Some(timestamp));
        assert_eq!(inv.investor, Some(investor));
    }

    #[test]
    fn test_mark_as_paid_transitions_status() {
        let env = dummy_env();
        let mut inv = create_invoice(&env, InvoiceStatus::Funded);
        let timestamp = env.ledger().timestamp();
        inv.mark_as_paid(&env, dummy_address(), timestamp);
        assert_eq!(inv.status, InvoiceStatus::Paid);
        assert_eq!(inv.total_paid, inv.amount);
        assert_eq!(inv.settled_at, Some(timestamp));
    }

    #[test]
    fn test_mark_as_defaulted_transitions_status() {
        let env = dummy_env();
        let mut inv = create_invoice(&env, InvoiceStatus::Funded);
        inv.mark_as_defaulted(&mut inv);
        assert_eq!(inv.status, InvoiceStatus::Defaulted);
    }

    #[test]
    fn test_mark_as_refunded_resets_funding() {
        let env = dummy_env();
        let mut inv = create_invoice(&env, InvoiceStatus::Funded);
        // Simulate funded state first
        inv.funded_amount = 500_000;
        inv.funded_at = Some(env.ledger().timestamp());
        inv.investor = Some(dummy_address());
        inv.total_paid = 300_000;
        // Now refund
        inv.mark_as_refunded(&env, dummy_address());
        assert_eq!(inv.status, InvoiceStatus::Refunded);
        assert_eq!(inv.funded_amount, 0);
        assert_eq!(inv.funded_at, None);
        assert_eq!(inv.investor, None);
        assert_eq!(inv.total_paid, 0);
        assert!(inv.payment_history.is_empty());
    }
}
