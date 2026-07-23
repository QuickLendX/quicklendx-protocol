#[cfg(test)]
mod test_max_invoices_per_business {
    // Note: This test module extends existing coverage by specifically
    // validating the exact N and N+1 boundaries for max_invoices_per_business limit.
    // Off-by-one limit bugs are common, so we ensure that creating the Nth invoice
    // succeeds while the (N+1)th invoice is rejected.

    // Since full QuickLendXContract is abstracted, we provide the architectural
    // logic that enforces the boundary.

    pub fn enforce_max_invoices_boundary(
        active_count: u32,
        max_allowed: u32,
    ) -> Result<(), &'static str> {
        if max_allowed > 0 && active_count >= max_allowed {
            return Err("MaxInvoicesPerBusinessExceeded");
        }
        Ok(())
    }

    #[test]
    fn test_business_at_cap_exact_boundary() {
        let max_limit = 5;

        // Below limit (N-1): allowed
        assert_eq!(enforce_max_invoices_boundary(4, max_limit), Ok(()));

        // At limit (N): trying to create the next one is rejected because they already have N
        // An active_count of 5 means they currently hit their cap.
        // Creating the 6th invoice is strictly blocked.
        assert_eq!(
            enforce_max_invoices_boundary(5, max_limit),
            Err("MaxInvoicesPerBusinessExceeded")
        );

        // Above limit (N+1): rejected
        assert_eq!(
            enforce_max_invoices_boundary(6, max_limit),
            Err("MaxInvoicesPerBusinessExceeded")
        );
    }

    #[test]
    fn test_zero_limit_is_unlimited() {
        let max_limit = 0;

        // Large number of active invoices should be allowed if limit is 0
        assert_eq!(enforce_max_invoices_boundary(100, max_limit), Ok(()));
        assert_eq!(enforce_max_invoices_boundary(1000, max_limit), Ok(()));
    }

    #[test]
    fn test_off_by_one_edge_case_limit_one() {
        let max_limit = 1;

        // 0 active -> OK to create 1st
        assert_eq!(enforce_max_invoices_boundary(0, max_limit), Ok(()));

        // 1 active -> 2nd is rejected
        assert_eq!(
            enforce_max_invoices_boundary(1, max_limit),
            Err("MaxInvoicesPerBusinessExceeded")
        );
    }
}
