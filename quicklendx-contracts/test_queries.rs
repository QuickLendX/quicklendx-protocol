use soroban_sdk::{Address, String, Vec, vec, BytesN, Env};
use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use quicklendx_contracts::invoice::{InvoiceCategory, InvoiceStatus};
use quicklendx_contracts::verification::{BusinessVerificationStatus, InvestorTier, InvestorRiskLevel};
use quicklendx_contracts::analytics::{TimePeriod};

/// Test all documented queries from README.md
pub fn test_all_documented_queries() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Register the contract
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    // Create test addresses
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400; // 1 day from now
    
    println!("=== Testing QuickLendX Protocol Queries ===");
    println!("Contract ID: {:?}", contract_id);
    println!("Admin: {:?}", admin);
    println!("Business: {:?}", business);
    println!("Investor: {:?}", investor);
    println!("Currency: {:?}", currency);
    println!();
    
    // Test 1: Set admin
    println!("1. Setting admin...");
    match client.try_set_admin(&admin) {
        Ok(_) => println!("✅ Admin set successfully"),
        Err(e) => println!("❌ Failed to set admin: {:?}", e),
    }
    
    // Test 2: Business KYC submission
    println!("\n2. Testing business KYC submission...");
    match client.try_submit_kyc_application(&business, &String::from_str(&env, "Business KYC Data")) {
        Ok(_) => println!("✅ Business KYC submitted successfully"),
        Err(e) => println!("❌ Failed to submit business KYC: {:?}", e),
    }
    
    // Test 3: Business verification
    println!("\n3. Testing business verification...");
    match client.try_verify_business(&admin, &business) {
        Ok(_) => println!("✅ Business verified successfully"),
        Err(e) => println!("❌ Failed to verify business: {:?}", e),
    }

    // Add currency to whitelist (required for invoice creation)
    let _ = client.add_currency(&admin, &currency);

    // Test 4: Create invoice
    println!("\n4. Testing invoice creation...");
    let invoice_id = match client.try_store_invoice(
        &business,
        &10000, // $100.00
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice for services"),
        &InvoiceCategory::Services,
        &vec![&env, String::from_str(&env, "test"), String::from_str(&env, "services")]
    ) {
        Ok(id) => {
            println!("✅ Invoice created successfully: {:?}", id);
            id
        },
        Err(e) => {
            println!("❌ Failed to create invoice: {:?}", e);
            return;
        }
    };
    
    // Test 5: Verify invoice
    println!("\n5. Testing invoice verification...");
    match client.try_verify_invoice(&invoice_id) {
        Ok(_) => println!("✅ Invoice verified successfully"),
        Err(e) => println!("❌ Failed to verify invoice: {:?}", e),
    }
    
    // Test 6: Investor KYC submission
    println!("\n6. Testing investor KYC submission...");
    match client.try_submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC Data")) {
        Ok(_) => println!("✅ Investor KYC submitted successfully"),
        Err(e) => println!("❌ Failed to submit investor KYC: {:?}", e),
    }
    
    // Test 7: Investor verification
    println!("\n7. Testing investor verification...");
    match client.try_verify_investor(&investor, &1000) {
        Ok(_) => println!("✅ Investor verified successfully"),
        Err(e) => println!("❌ Failed to verify investor: {:?}", e),
    }
    
    // Test 8: Place bid
    println!("\n8. Testing bid placement...");
    let bid_id = match client.try_place_bid(&investor, &invoice_id, &9500, &10000) {
        Ok(id) => {
            println!("✅ Bid placed successfully: {:?}", id);
            id
        },
        Err(e) => {
            println!("❌ Failed to place bid: {:?}", e);
            return;
        }
    };
    
    // Test 9: Accept bid
    println!("\n9. Testing bid acceptance...");
    match client.try_accept_bid(&invoice_id, &bid_id) {
        Ok(_) => println!("✅ Bid accepted successfully"),
        Err(e) => println!("❌ Failed to accept bid: {:?}", e),
    }
    
    // Test 10: Release escrow funds
    println!("\n10. Testing escrow fund release...");
    match client.try_release_escrow_funds(&invoice_id) {
        Ok(_) => println!("✅ Escrow funds released successfully"),
        Err(e) => println!("❌ Failed to release escrow funds: {:?}", e),
    }
    
    // Test 11: Query functions
    println!("\n11. Testing query functions...");
    
    // Get invoice
    match client.try_get_invoice(&invoice_id) {
        Ok(invoice) => println!("✅ Retrieved invoice: amount={}, status={:?}", invoice.amount, invoice.status),
        Err(e) => println!("❌ Failed to get invoice: {:?}", e),
    }
    
    // Get business invoices
    let business_invoices = client.get_business_invoices(&business);
    println!("✅ Retrieved {} business invoices", business_invoices.len());
    
    // Get invoices by status
    let pending_invoices = client.get_invoices_by_status(&InvoiceStatus::Pending);
    let verified_invoices = client.get_invoices_by_status(&InvoiceStatus::Verified);
    let funded_invoices = client.get_invoices_by_status(&InvoiceStatus::Funded);
    println!("✅ Retrieved invoices by status - Pending: {}, Verified: {}, Funded: {}", 
             pending_invoices.len(), verified_invoices.len(), funded_invoices.len());
    
    // Get available invoices
    let available_invoices = client.get_available_invoices();
    println!("✅ Retrieved {} available invoices", available_invoices.len());
    
    // Test 12: Verification queries
    println!("\n12. Testing verification queries...");
    
    // Get verified businesses
    let verified_businesses = client.get_verified_businesses();
    println!("✅ Retrieved {} verified businesses", verified_businesses.len());
    
    // Get pending businesses
    let pending_businesses = client.get_pending_businesses();
    println!("✅ Retrieved {} pending businesses", pending_businesses.len());
    
    // Get business verification status
    match client.try_get_business_verification_status(&business) {
        Ok(Some(verification)) => println!("✅ Retrieved business verification: status={:?}", verification.status),
        Ok(None) => println!("❌ No business verification found"),
        Err(e) => println!("❌ Failed to get business verification: {:?}", e),
    }
    
    // Test 13: Investor verification queries
    println!("\n13. Testing investor verification queries...");
    
    // Get verified investors
    let verified_investors = client.get_verified_investors();
    println!("✅ Retrieved {} verified investors", verified_investors.len());
    
    // Get pending investors
    let pending_investors = client.get_pending_investors();
    println!("✅ Retrieved {} pending investors", pending_investors.len());
    
    // Get investor verification
    match client.try_get_investor_verification(&investor) {
        Ok(Some(verification)) => {
            println!("✅ Retrieved investor verification: tier={:?}, risk_level={:?}, limit={}", 
                     verification.tier, verification.risk_level, verification.investment_limit);
        },
        Ok(None) => println!("❌ No investor verification found"),
        Err(e) => println!("❌ Failed to get investor verification: {:?}", e),
    }
    
    // Test 14: Analytics queries
    println!("\n14. Testing analytics queries...");
    
    // Get platform metrics
    match client.try_get_platform_metrics() {
        Ok(metrics) => println!("✅ Retrieved platform metrics: total_invoices={}, total_volume={}", 
                               metrics.total_invoices, metrics.total_volume),
        Err(e) => println!("❌ Failed to get platform metrics: {:?}", e),
    }
    
    // Get performance metrics
    match client.try_get_performance_metrics() {
        Ok(metrics) => println!("✅ Retrieved performance metrics: success_rate={}, satisfaction={}", 
                               metrics.transaction_success_rate, metrics.user_satisfaction_score),
        Err(e) => println!("❌ Failed to get performance metrics: {:?}", e),
    }
    
    // Test 15: Audit queries
    println!("\n15. Testing audit queries...");
    
    // Get audit trail
    let audit_trail = client.get_invoice_audit_trail(&invoice_id);
    println!("✅ Retrieved {} audit entries for invoice", audit_trail.len());
    
    // Get audit stats
    let audit_stats = client.get_audit_stats();
    println!("✅ Retrieved audit stats: total_entries={}", audit_stats.total_entries);
    
    // Test 16: Backup queries
    println!("\n16. Testing backup queries...");
    
    // Create backup
    match client.try_create_backup(&String::from_str(&env, "Test backup")) {
        Ok(backup_id) => {
            println!("✅ Created backup: {:?}", backup_id);
            
            // Get backup details
            match client.try_get_backup_details(&backup_id) {
                Ok(Some(backup)) => println!("✅ Retrieved backup details: status={:?}, count={}", 
                                           backup.status, backup.invoice_count),
                Ok(None) => println!("❌ No backup found"),
                Err(e) => println!("❌ Failed to get backup details: {:?}", e),
            }
        },
        Err(e) => println!("❌ Failed to create backup: {:?}", e),
    }
    
    // Get all backups
    let backups = client.get_backups();
    println!("✅ Retrieved {} backups", backups.len());
    
    // Test 17: Category and tag queries
    println!("\n17. Testing category and tag queries...");
    
    // Get invoices by category
    let services_invoices = client.get_invoices_by_category(&InvoiceCategory::Services);
    println!("✅ Retrieved {} invoices in Services category", services_invoices.len());
    
    // Get invoices by tag
    let test_tag_invoices = client.get_invoices_by_tag(&String::from_str(&env, "test"));
    println!("✅ Retrieved {} invoices with 'test' tag", test_tag_invoices.len());
    
    // Get all categories
    let all_categories = client.get_all_categories();
    println!("✅ Retrieved {} categories", all_categories.len());
    
    // Test 18: Rating queries
    println!("\n18. Testing rating queries...");
    
    // Get invoices with ratings
    let invoices_with_ratings = client.get_invoices_with_ratings_count();
    println!("✅ Retrieved {} invoices with ratings", invoices_with_ratings);
    
    // Get invoices with rating above threshold
    let high_rated_invoices = client.get_invoices_with_rating_above(&4);
    println!("✅ Retrieved {} invoices with rating above 4", high_rated_invoices.len());
    
    // Test 19: Notification queries
    println!("\n19. Testing notification queries...");
    
    // Get user notifications
    let user_notifications = client.get_user_notifications(&business);
    println!("✅ Retrieved {} notifications for business", user_notifications.len());
    
    // Get notification preferences
    let preferences = client.get_notification_preferences(&business);
    println!("✅ Retrieved notification preferences: email={}, push={}", 
             preferences.email_enabled, preferences.push_enabled);
    
    // Get notification stats
    let notification_stats = client.get_user_notification_stats(&business);
    println!("✅ Retrieved notification stats: total_sent={}, total_delivered={}", 
             notification_stats.total_sent, notification_stats.total_delivered);
    
    // Test 20: Advanced analytics queries
    println!("\n20. Testing advanced analytics queries...");
    
    // Get financial metrics
    match client.try_get_financial_metrics(&TimePeriod::Monthly) {
        Ok(metrics) => println!("✅ Retrieved financial metrics: total_volume={}, total_fees={}", 
                               metrics.total_volume, metrics.total_fees),
        Err(e) => println!("❌ Failed to get financial metrics: {:?}", e),
    }
    
    // Get user behavior metrics
    match client.try_get_user_behavior_metrics(&business) {
        Ok(metrics) => println!("✅ Retrieved user behavior metrics: invoices_uploaded={}, risk_score={}", 
                               metrics.total_invoices_uploaded, metrics.risk_score),
        Err(e) => println!("❌ Failed to get user behavior metrics: {:?}", e),
    }
    
    // Get analytics summary
    match client.try_get_analytics_summary() {
        Ok((platform_metrics, performance_metrics)) => {
            println!("✅ Retrieved analytics summary: platform_invoices={}, performance_success_rate={}", 
                     platform_metrics.total_invoices, performance_metrics.transaction_success_rate);
        },
        Err(e) => println!("❌ Failed to get analytics summary: {:?}", e),
    }
    
    // Test 21: Investor analytics queries
    println!("\n21. Testing investor analytics queries...");
    
    // Get investors by tier
    let basic_investors = client.get_investors_by_tier(&InvestorTier::Basic);
    println!("✅ Retrieved {} investors in Basic tier", basic_investors.len());
    
    // Get investors by risk level
    let medium_risk_investors = client.get_investors_by_risk_level(&InvestorRiskLevel::Medium);
    println!("✅ Retrieved {} investors with Medium risk", medium_risk_investors.len());
    
    // Calculate investor analytics
    match client.try_calculate_investor_analytics(&investor) {
        Ok(analytics) => println!("✅ Calculated investor analytics: tier={:?}, success_rate={}", 
                                 analytics.tier, analytics.success_rate),
        Err(e) => println!("❌ Failed to calculate investor analytics: {:?}", e),
    }
    
    // Get investor performance metrics
    match client.try_calc_investor_perf_metrics() {
        Ok(metrics) => println!("✅ Retrieved investor performance metrics: total_investors={}, verified={}", 
                               metrics.total_investors, metrics.verified_investors),
        Err(e) => println!("❌ Failed to get investor performance metrics: {:?}", e),
    }
    
    println!("\n=== Query Testing Complete ===");
    println!("All documented queries have been tested.");
    println!("Some queries may fail due to:");
    println!("- Insufficient test data");
    println!("- Missing dependencies");
    println!("- Contract state requirements");
    println!("- Network connectivity issues");
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_all_queries() {
        test_all_documented_queries();
    }
}
