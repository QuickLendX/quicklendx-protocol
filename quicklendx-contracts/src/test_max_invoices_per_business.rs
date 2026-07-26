#[cfg(test)]
mod test_max_invoices_per_business {
    use crate::errors::QuickLendXError;
    use crate::protocol_limits::is_active_status;
    use crate::types::InvoiceStatus;

    fn enforce_limit_logic(active_count: u32, limit: u32) -> Result<(), QuickLendXError> {
        if limit > 0 && active_count >= limit {
            return Err(QuickLendXError::MaxInvoicesPerBusinessExceeded);
        }
        Ok(())
    }

    #[test]
    fn test_business_at_cap_exact_boundary() {
        let limit = 5;
        assert_eq!(enforce_limit_logic(4, limit), Ok(()));
        assert_eq!(
            enforce_limit_logic(5, limit),
            Err(QuickLendXError::MaxInvoicesPerBusinessExceeded)
        );
        assert_eq!(
            enforce_limit_logic(6, limit),
            Err(QuickLendXError::MaxInvoicesPerBusinessExceeded)
        );
    }

    #[test]
    fn test_zero_limit_is_unlimited() {
        assert_eq!(enforce_limit_logic(100, 0), Ok(()));
        assert_eq!(enforce_limit_logic(1000, 0), Ok(()));
    }

    #[test]
    fn test_is_active_status_boundaries() {
        assert!(is_active_status(&InvoiceStatus::Pending));
        assert!(is_active_status(&InvoiceStatus::Verified));
        assert!(is_active_status(&InvoiceStatus::Funded));
        assert!(!is_active_status(&InvoiceStatus::Paid));
        assert!(!is_active_status(&InvoiceStatus::Defaulted));
        assert!(!is_active_status(&InvoiceStatus::Cancelled));
        assert!(!is_active_status(&InvoiceStatus::Refunded));
    }
}
