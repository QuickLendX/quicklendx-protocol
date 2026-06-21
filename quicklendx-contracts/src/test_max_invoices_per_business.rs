#[cfg(test)]
mod test_max_invoices_per_business {
    use crate::protocol_limits::is_active_status;
    use crate::types::InvoiceStatus;
    use crate::errors::QuickLendXError;

    // Core logic test extracted from check_invoice_limit architecture
    fn enforce_limit_logic(active_count: u32, limit: u32) -> Result<(), QuickLendXError> {
        if limit > 0 && active_count >= limit {
            return Err(QuickLendXError::MaxInvoicesPerBusinessExceeded);
        }
        Ok(())
    }

    #[test]
    fn test_business_at_cap_exact_boundary() {
        let limit = 5;
        
        // Below limit (N-1): allowed
        assert_eq!(enforce_limit_logic(4, limit), Ok(()));
        
        // At limit (N): trying to create the next one is rejected
        assert_eq!(enforce_limit_logic(5, limit), Err(QuickLendXError::MaxInvoicesPerBusinessExceeded));
        
        // Above limit (N+1): rejected safely
        assert_eq!(enforce_limit_logic(6, limit), Err(QuickLendXError::MaxInvoicesPerBusinessExceeded));
    }

    #[test]
    fn test_zero_limit_is_unlimited() {
        let limit = 0; // Protocol defined: 0 = unlimited
        
        // Very large volumes should be permissible
        assert_eq!(enforce_limit_logic(100, limit), Ok(()));
        assert_eq!(enforce_limit_logic(1000, limit), Ok(()));
    }

    #[test]
    fn test_is_active_status_boundaries() {
        // Ensures our capacity algorithm isn't falsely inflated by settled state
        assert_eq!(is_active_status(&InvoiceStatus::Pending), true);
        assert_eq!(is_active_status(&InvoiceStatus::Verified), true);
        assert_eq!(is_active_status(&InvoiceStatus::Funded), true);
        
        assert_eq!(is_active_status(&InvoiceStatus::Paid), false);
        assert_eq!(is_active_status(&InvoiceStatus::Defaulted), false);
        assert_eq!(is_active_status(&InvoiceStatus::Cancelled), false);
        assert_eq!(is_active_status(&InvoiceStatus::Refunded), false);
    }
}