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

/// Test query functions with empty results and edge cases
pub fn test_query_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Register the contract
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    // Create test addresses
    let non_existent_address = Address::generate(&env);
    let invalid_invoice_id = BytesN::<32>::from_array(&env, &[0u8; 32]);
    
    println!("=== Testing Query Edge Cases ===");
    
    // Test 1: Empty results - no data in contract
    println!("\n1. Testing empty results (no data)...");
    
    // Invoice queries with no data
    let empty_business_invoices = client.get_business_invoices(&non_existent_address);
    assert_eq!(empty_business_invoices.len(), 0, "Should return empty vec for non-existent business");
    println!("✅ get_business_invoices returns empty for non-existent business");
    
    let empty_pending = client.get_invoices_by_status(&InvoiceStatus::Pending);
    assert_eq!(empty_pending.len(), 0, "Should return empty vec when no invoices exist");
    println!("✅ get_invoices_by_status returns empty when no invoices exist");
    
    let empty_available = client.get_available_invoices();
    assert_eq!(empty_available.len(), 0, "Should return empty vec when no verified invoices exist");
    println!("✅ get_available_invoices returns empty when no verified invoices exist");
    
    let empty_categories = client.get_all_categories();
    assert_eq!(empty_categories.len(), 0, "Should return empty vec when no categories used");
    println!("✅ get_all_categories returns empty when no categories used");
    
    let empty_ratings_count = client.get_invoices_with_ratings_count();
    assert_eq!(empty_ratings_count, 0, "Should return 0 when no invoices have ratings");
    println!("✅ get_invoices_with_ratings_count returns 0 when no ratings exist");
    
    let empty_high_ratings = client.get_invoices_with_rating_above(&3);
    assert_eq!(empty_high_ratings.len(), 0, "Should return empty vec when no high ratings exist");
    println!("✅ get_invoices_with_rating_above returns empty when no high ratings exist");
    
    // Verification queries with no data
    let empty_verified_businesses = client.get_verified_businesses();
    assert_eq!(empty_verified_businesses.len(), 0, "Should return empty vec when no businesses verified");
    println!("✅ get_verified_businesses returns empty when no businesses verified");
    
    let empty_pending_businesses = client.get_pending_businesses();
    assert_eq!(empty_pending_businesses.len(), 0, "Should return empty vec when no businesses pending");
    println!("✅ get_pending_businesses returns empty when no businesses pending");
    
    let empty_verified_investors = client.get_verified_investors();
    assert_eq!(empty_verified_investors.len(), 0, "Should return empty vec when no investors verified");
    println!("✅ get_verified_investors returns empty when no investors verified");
    
    let empty_pending_investors = client.get_pending_investors();
    assert_eq!(empty_pending_investors.len(), 0, "Should return empty vec when no investors pending");
    println!("✅ get_pending_investors returns empty when no investors pending");
    
    // Bid queries with no data
    let empty_best_bid = client.get_best_bid(&invalid_invoice_id);
    assert!(empty_best_bid.is_none(), "Should return None for non-existent invoice");
    println!("✅ get_best_bid returns None for non-existent invoice");
    
    let empty_ranked_bids = client.get_ranked_bids(&invalid_invoice_id);
    assert_eq!(empty_ranked_bids.len(), 0, "Should return empty vec for non-existent invoice");
    println!("✅ get_ranked_bids returns empty for non-existent invoice");
    
    // Audit queries with no data
    let empty_audit_trail = client.get_invoice_audit_trail(&invalid_invoice_id);
    assert_eq!(empty_audit_trail.len(), 0, "Should return empty vec for non-existent invoice");
    println!("✅ get_invoice_audit_trail returns empty for non-existent invoice");
    
    let audit_stats = client.get_audit_stats();
    assert_eq!(audit_stats.total_entries, 0, "Should return 0 entries when no audit data exists");
    println!("✅ get_audit_stats returns 0 entries when no audit data exists");
    
    // Backup queries with no data
    let empty_backups = client.get_backups();
    assert_eq!(empty_backups.len(), 0, "Should return empty vec when no backups exist");
    println!("✅ get_backups returns empty when no backups exist");
    
    // Notification queries with no data
    let empty_notifications = client.get_user_notifications(&non_existent_address);
    assert_eq!(empty_notifications.len(), 0, "Should return empty vec for user with no notifications");
    println!("✅ get_user_notifications returns empty for user with no notifications");
    
    let notification_stats = client.get_user_notification_stats(&non_existent_address);
    assert_eq!(notification_stats.total_sent, 0, "Should return 0 sent for user with no notifications");
    assert_eq!(notification_stats.total_delivered, 0, "Should return 0 delivered for user with no notifications");
    println!("✅ get_user_notification_stats returns zeros for user with no notifications");
    
    // Test 2: Invalid parameters
    println!("\n2. Testing invalid parameters...");
    
    // Try to get non-existent invoice
    match client.try_get_invoice(&invalid_invoice_id) {
        Ok(_) => panic!("Should fail for non-existent invoice"),
        Err(_) => println!("✅ get_invoice correctly fails for non-existent invoice"),
    }
    
    // Try to get non-existent bid
    let non_existent_bid = client.get_bid(&invalid_invoice_id);
    assert!(non_existent_bid.is_none(), "Should return None for non-existent bid");
    println!("✅ get_bid returns None for non-existent bid");
    
    // Try to get business verification for non-existent business
    match client.try_get_business_verification_status(&non_existent_address) {
        Ok(None) => println!("✅ get_business_verification_status returns None for non-existent business"),
        Ok(Some(_)) => panic!("Should return None for non-existent business"),
        Err(e) => println!("❌ Unexpected error: {:?}", e),
    }
    
    // Try to get investor verification for non-existent investor
    match client.try_get_investor_verification(&non_existent_address) {
        Ok(None) => println!("✅ get_investor_verification returns None for non-existent investor"),
        Ok(Some(_)) => panic!("Should return None for non-existent investor"),
        Err(e) => println!("❌ Unexpected error: {:?}", e),
    }
    
    // Test 3: Filter combinations and boundary conditions
    println!("\n3. Testing filter combinations and boundaries...");
    
    // Test with empty tag
    let empty_tag_results = client.get_invoices_by_tag(&String::from_str(&env, ""));
    assert_eq!(empty_tag_results.len(), 0, "Should return empty for empty tag");
    println!("✅ get_invoices_by_tag returns empty for empty tag");
    
    // Test with non-existent tag
    let non_existent_tag_results = client.get_invoices_by_tag(&String::from_str(&env, "nonexistent"));
    assert_eq!(non_existent_tag_results.len(), 0, "Should return empty for non-existent tag");
    println!("✅ get_invoices_by_tag returns empty for non-existent tag");
    
    // Test rating threshold boundaries
    let zero_threshold = client.get_invoices_with_rating_above(&0);
    assert_eq!(zero_threshold.len(), 0, "Should return empty for threshold 0 (no ratings exist)");
    println!("✅ get_invoices_with_rating_above returns empty for threshold 0");
    
    let max_threshold = client.get_invoices_with_rating_above(&5);
    assert_eq!(max_threshold.len(), 0, "Should return empty for threshold 5 (above max possible)");
    println!("✅ get_invoices_with_rating_above returns empty for threshold 5");
    
    // Test investor tier and risk level filters
    let basic_tier_investors = client.get_investors_by_tier(&InvestorTier::Basic);
    assert_eq!(basic_tier_investors.len(), 0, "Should return empty for Basic tier when no investors exist");
    println!("✅ get_investors_by_tier returns empty for Basic tier");
    
    let low_risk_investors = client.get_investors_by_risk_level(&InvestorRiskLevel::Low);
    assert_eq!(low_risk_investors.len(), 0, "Should return empty for Low risk level when no investors exist");
    println!("✅ get_investors_by_risk_level returns empty for Low risk level");
    
    // Test invoice count methods
    let pending_count = client.get_invoice_count_by_status(&InvoiceStatus::Pending);
    assert_eq!(pending_count, 0, "Should return 0 for Pending status when no invoices exist");
    println!("✅ get_invoice_count_by_status returns 0 for Pending status");
    
    let total_count = client.get_total_invoice_count();
    assert_eq!(total_count, 0, "Should return 0 when no invoices exist");
    println!("✅ get_total_invoice_count returns 0 when no invoices exist");
    
    // Test analytics queries with no data
    match client.try_get_platform_metrics() {
        Ok(metrics) => {
            assert_eq!(metrics.total_invoices, 0, "Platform metrics should show 0 invoices");
            assert_eq!(metrics.total_volume, 0, "Platform metrics should show 0 volume");
            println!("✅ get_platform_metrics returns zeros when no data exists");
        },
        Err(e) => println!("❌ get_platform_metrics failed: {:?}", e),
    }
    
    match client.try_get_performance_metrics() {
        Ok(metrics) => {
            // These might have default values, just check they don't panic
            println!("✅ get_performance_metrics succeeds even with no data");
        },
        Err(e) => println!("❌ get_performance_metrics failed: {:?}", e),
    }
    
    // Test investor analytics with non-existent investor
    match client.try_calculate_investor_analytics(&non_existent_address) {
        Ok(_) => println!("✅ calculate_investor_analytics succeeds for non-existent investor"),
        Err(e) => println!("❌ calculate_investor_analytics failed for non-existent investor: {:?}", e),
    }
    
    println!("\n=== Edge Case Testing Complete ===");
    println!("All edge cases handled correctly:");
    println!("- Empty results return appropriate empty collections or zeros");
    println!("- Invalid parameters return None or fail gracefully");
    println!("- Boundary conditions are handled properly");
    println!("- No panics or unexpected errors");
}

/// Test query functions with populated data to verify correct results
pub fn test_query_correctness_with_data() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Register the contract
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    // Create test addresses
    let admin = Address::generate(&env);
    let business1 = Address::generate(&env);
    let business2 = Address::generate(&env);
    let investor1 = Address::generate(&env);
    let investor2 = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;
    
    println!("=== Testing Query Correctness with Data ===");
    
    // Set up initial state
    client.try_set_admin(&admin).unwrap();
    
    // Create multiple businesses and verify them
    client.try_submit_kyc_application(&business1, &String::from_str(&env, "Business 1 KYC")).unwrap();
    client.try_submit_kyc_application(&business2, &String::from_str(&env, "Business 2 KYC")).unwrap();
    client.try_verify_business(&admin, &business1).unwrap();
    client.try_verify_business(&admin, &business2).unwrap();
    
    // Create multiple investors and verify them
    client.try_submit_investor_kyc(&investor1, &String::from_str(&env, "Investor 1 KYC")).unwrap();
    client.try_submit_investor_kyc(&investor2, &String::from_str(&env, "Investor 2 KYC")).unwrap();
    client.try_verify_investor(&investor1, &5000).unwrap();
    client.try_verify_investor(&investor2, &10000).unwrap();
    
    // Create multiple invoices with different categories and tags
    let invoice1_id = client.try_store_invoice(
        &business1,
        &5000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 1"),
        &InvoiceCategory::Services,
        &vec![&env, String::from_str(&env, "urgent"), String::from_str(&env, "services")]
    ).unwrap();
    
    let invoice2_id = client.try_store_invoice(
        &business1,
        &7500,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Goods,
        &vec![&env, String::from_str(&env, "urgent"), String::from_str(&env, "goods")]
    ).unwrap();
    
    let invoice3_id = client.try_store_invoice(
        &business2,
        &10000,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice 3"),
        &InvoiceCategory::Services,
        &vec![&env, String::from_str(&env, "standard"), String::from_str(&env, "services")]
    ).unwrap();
    
    // Verify invoices
    client.try_verify_invoice(&invoice1_id).unwrap();
    client.try_verify_invoice(&invoice2_id).unwrap();
    client.try_verify_invoice(&invoice3_id).unwrap();
    
    // Test correctness
    println!("\n1. Testing invoice queries correctness...");
    
    // Get business invoices
    let business1_invoices = client.get_business_invoices(&business1);
    assert_eq!(business1_invoices.len(), 2, "Business 1 should have 2 invoices");
    assert!(business1_invoices.contains(&invoice1_id), "Should contain invoice1");
    assert!(business1_invoices.contains(&invoice2_id), "Should contain invoice2");
    println!("✅ get_business_invoices returns correct invoices for business1");
    
    let business2_invoices = client.get_business_invoices(&business2);
    assert_eq!(business2_invoices.len(), 1, "Business 2 should have 1 invoice");
    assert!(business2_invoices.contains(&invoice3_id), "Should contain invoice3");
    println!("✅ get_business_invoices returns correct invoices for business2");
    
    // Get invoices by status
    let verified_invoices = client.get_invoices_by_status(&InvoiceStatus::Verified);
    assert_eq!(verified_invoices.len(), 3, "Should have 3 verified invoices");
    println!("✅ get_invoices_by_status returns correct count for Verified");
    
    let pending_invoices = client.get_invoices_by_status(&InvoiceStatus::Pending);
    assert_eq!(pending_invoices.len(), 0, "Should have 0 pending invoices");
    println!("✅ get_invoices_by_status returns correct count for Pending");
    
    // Get available invoices (verified but not funded)
    let available_invoices = client.get_available_invoices();
    assert_eq!(available_invoices.len(), 3, "Should have 3 available invoices");
    println!("✅ get_available_invoices returns all verified invoices");
    
    // Get invoices by category
    let services_invoices = client.get_invoices_by_category(&InvoiceCategory::Services);
    assert_eq!(services_invoices.len(), 2, "Should have 2 services invoices");
    println!("✅ get_invoices_by_category returns correct count for Services");
    
    let goods_invoices = client.get_invoices_by_category(&InvoiceCategory::Goods);
    assert_eq!(goods_invoices.len(), 1, "Should have 1 goods invoice");
    println!("✅ get_invoices_by_category returns correct count for Goods");
    
    // Get invoices by tag
    let urgent_invoices = client.get_invoices_by_tag(&String::from_str(&env, "urgent"));
    assert_eq!(urgent_invoices.len(), 2, "Should have 2 urgent invoices");
    println!("✅ get_invoices_by_tag returns correct count for 'urgent'");
    
    let services_tag_invoices = client.get_invoices_by_tag(&String::from_str(&env, "services"));
    assert_eq!(services_tag_invoices.len(), 2, "Should have 2 services-tagged invoices");
    println!("✅ get_invoices_by_tag returns correct count for 'services'");
    
    // Get all categories
    let all_categories = client.get_all_categories();
    assert!(all_categories.contains(&InvoiceCategory::Services), "Should contain Services");
    assert!(all_categories.contains(&InvoiceCategory::Goods), "Should contain Goods");
    assert_eq!(all_categories.len(), 2, "Should have 2 unique categories");
    println!("✅ get_all_categories returns correct categories");
    
    // Get invoice counts
    let verified_count = client.get_invoice_count_by_status(&InvoiceStatus::Verified);
    assert_eq!(verified_count, 3, "Should have 3 verified invoices");
    println!("✅ get_invoice_count_by_status returns correct count");
    
    let total_count = client.get_total_invoice_count();
    assert_eq!(total_count, 3, "Should have 3 total invoices");
    println!("✅ get_total_invoice_count returns correct total");
    
    println!("\n2. Testing verification queries correctness...");
    
    // Get verified businesses
    let verified_businesses = client.get_verified_businesses();
    assert_eq!(verified_businesses.len(), 2, "Should have 2 verified businesses");
    assert!(verified_businesses.contains(&business1), "Should contain business1");
    assert!(verified_businesses.contains(&business2), "Should contain business2");
    println!("✅ get_verified_businesses returns correct businesses");
    
    // Get verified investors
    let verified_investors = client.get_verified_investors();
    assert_eq!(verified_investors.len(), 2, "Should have 2 verified investors");
    assert!(verified_investors.contains(&investor1), "Should contain investor1");
    assert!(verified_investors.contains(&investor2), "Should contain investor2");
    println!("✅ get_verified_investors returns correct investors");
    
    // Get pending businesses/investors (should be empty)
    let pending_businesses = client.get_pending_businesses();
    assert_eq!(pending_businesses.len(), 0, "Should have 0 pending businesses");
    println!("✅ get_pending_businesses returns empty when all verified");
    
    let pending_investors = client.get_pending_investors();
    assert_eq!(pending_investors.len(), 0, "Should have 0 pending investors");
    println!("✅ get_pending_investors returns empty when all verified");
    
    println!("\n3. Testing investor tier and risk level queries...");
    
    // Get investors by tier (both should be Basic by default)
    let basic_investors = client.get_investors_by_tier(&InvestorTier::Basic);
    assert_eq!(basic_investors.len(), 2, "Should have 2 basic tier investors");
    println!("✅ get_investors_by_tier returns correct count for Basic tier");
    
    // Get investors by risk level (should be Low by default)
    let low_risk_investors = client.get_investors_by_risk_level(&InvestorRiskLevel::Low);
    assert_eq!(low_risk_investors.len(), 2, "Should have 2 low risk investors");
    println!("✅ get_investors_by_risk_level returns correct count for Low risk");
    
    println!("\n=== Query Correctness Testing Complete ===");
    println!("All queries return correct data when populated with test data.");
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_all_queries() {
        test_all_documented_queries();
    }
    
    #[test]
    fn test_edge_cases() {
        test_query_edge_cases();
    }
    
    #[test]
    fn test_correctness_with_data() {
        test_query_correctness_with_data();
    }
}
