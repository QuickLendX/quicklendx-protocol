#![cfg(test)]

/// Test isolation and environment management for QuickLendX Protocol
/// 
/// This module provides utilities for ensuring test isolation, managing
/// test environments, and preventing test interference.

use soroban_sdk::{Address, Env, BytesN, String, Vec};
use crate::QuickLendXContractClient;

/// Simple test environment counter (simplified without std dependencies)
static mut ENVIRONMENT_COUNTER: u32 = 0;

/// Get next environment ID (simplified)
fn get_next_env_id(test_name: &str) -> String {
    unsafe {
        ENVIRONMENT_COUNTER += 1;
        String::from_str(&Env::default(), test_name)
    }
}

/// Isolated test environment
#[derive(Clone)]
pub struct TestEnvironment<'a> {
    pub env_id: String,
    pub env: Env,
    pub contract_client: QuickLendXContractClient<'a>,
    pub admin: Address,
    pub test_data: TestData,
}

/// Test data container for managing test state
#[derive(Clone)]
pub struct TestData {
    pub businesses: Vec<Address>,
    pub investors: Vec<Address>,
    pub currencies: Vec<Address>,
    pub invoices: Vec<BytesN<32>>,
    pub bids: Vec<BytesN<32>>,
}

impl TestData {
    fn new() -> Self {
        Self {
            businesses: Vec::new(),
            investors: Vec::new(),
            currencies: Vec::new(),
            invoices: Vec::new(),
            bids: Vec::new(),
        }
    }
    
    fn clear(&mut self) {
        self.businesses.clear();
        self.investors.clear();
        self.currencies.clear();
        self.invoices.clear();
        self.bids.clear();
    }
}

impl<'a> TestEnvironment<'a> {
    fn new(env_id: String) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        
        // Register contract with unique identifier to prevent conflicts
        let contract_id = env.register_contract(None, crate::QuickLendXContract);
        let contract_client = QuickLendXContractClient::new(&env, &contract_id);
        
        // Set up admin
        let admin = Address::generate(&env);
        contract_client.set_admin(&admin);
        
        Self {
            env_id,
            env,
            contract_client,
            admin,
            test_data: TestData::new(),
        }
    }
    
    /// Create a verified business for testing
    pub fn create_verified_business(&mut self) -> Address {
        let business = Address::generate(&self.env);
        let kyc_data = soroban_sdk::String::from_str(
            &self.env,
            "KYC data for test business"
        );
        
        self.contract_client.submit_kyc_application(&business, &kyc_data).unwrap();
        self.contract_client.verify_business(&self.admin, &business).unwrap();
        
        self.test_data.businesses.push(business.clone());
        business
    }
    
    /// Create multiple verified businesses
    pub fn create_verified_businesses(&mut self, count: usize) -> Vec<Address> {
        (0..count).map(|_| self.create_verified_business()).collect()
    }
    
    /// Create test investors
    pub fn create_investors(&mut self, count: usize) -> Vec<Address> {
        let investors: Vec<Address> = (0..count)
            .map(|_| Address::generate(&self.env))
            .collect();
        
        self.test_data.investors.extend(investors.clone());
        investors
    }
    
    /// Create test currencies
    pub fn create_currencies(&mut self, count: usize) -> Vec<Address> {
        let currencies: Vec<Address> = (0..count)
            .map(|_| Address::generate(&self.env))
            .collect();
        
        self.test_data.currencies.extend(currencies.clone());
        currencies
    }
    
    /// Create a test invoice
    pub fn create_test_invoice(
        &mut self,
        business: &Address,
        amount: i128,
        currency: &Address,
    ) -> Result<BytesN<32>, crate::errors::QuickLendXError> {
        let due_date = self.env.ledger().timestamp() + 86400;
        let description = soroban_sdk::String::from_str(
            &self.env,
            "Test invoice for environment"
        );
        let category = crate::invoice::InvoiceCategory::Services;
        let tags = soroban_sdk::vec![
            &self.env,
            soroban_sdk::String::from_str(&self.env, "test")
        ];
        
        let invoice_id = self.contract_client.upload_invoice(
            business,
            &amount,
            currency,
            &due_date,
            &description,
            &category,
            &tags,
        )?;
        
        self.test_data.invoices.push(invoice_id.clone());
        Ok(invoice_id)
    }
    
    /// Create and verify a test invoice
    pub fn create_verified_invoice(
        &mut self,
        business: &Address,
        amount: i128,
        currency: &Address,
    ) -> Result<BytesN<32>, crate::errors::QuickLendXError> {
        let invoice_id = self.create_test_invoice(business, amount, currency)?;
        self.contract_client.verify_invoice(&invoice_id)?;
        Ok(invoice_id)
    }
    
    /// Place a test bid
    pub fn place_test_bid(
        &mut self,
        investor: &Address,
        invoice_id: &BytesN<32>,
        bid_amount: i128,
        expected_return: i128,
    ) -> BytesN<32> {
        let bid_id = self.contract_client.place_bid(
            investor,
            invoice_id,
            &bid_amount,
            &expected_return,
        );
        
        self.test_data.bids.push(bid_id.clone());
        bid_id
    }
    
    /// Get test statistics
    pub fn get_test_stats(&self) -> TestStats {
        TestStats {
            businesses_created: self.test_data.businesses.len(),
            investors_created: self.test_data.investors.len(),
            currencies_created: self.test_data.currencies.len(),
            invoices_created: self.test_data.invoices.len(),
            bids_created: self.test_data.bids.len(),
        }
    }
    
    /// Clean up test environment
    fn cleanup(&self) {
        // Perform any necessary cleanup
        // In Soroban, the environment is automatically cleaned up
        // but we could add custom cleanup logic here if needed
    }
}

/// Test statistics
#[derive(Debug, Clone)]
pub struct TestStats {
    pub businesses_created: usize,
    pub investors_created: usize,
    pub currencies_created: usize,
    pub invoices_created: usize,
    pub bids_created: usize,
}

/// Test isolation wrapper - ensures each test runs in isolation
pub struct IsolatedTest<'a> {
    env: TestEnvironment<'a>,
    test_name: String,
}

impl<'a> IsolatedTest<'a> {
    /// Create a new isolated test environment
    pub fn new(test_name: &str) -> Self {
        let env_id = get_next_env_id(test_name);
        let env = TestEnvironment::new(env_id);

        Self {
            env,
            test_name: String::from_str(&Env::default(), test_name),
        }
    }

    /// Get the test environment
    pub fn env(&mut self) -> &mut TestEnvironment<'a> {
        &mut self.env
    }

    /// Run a test with automatic cleanup
    pub fn run<F, R>(test_name: &str, test_fn: F) -> R
    where
        F: FnOnce(&mut TestEnvironment) -> R,
    {
        let mut isolated_test = Self::new(test_name);
        let result = test_fn(isolated_test.env());

        // Cleanup is handled automatically
        result
    }
}

/// Macro for creating isolated tests
#[macro_export]
macro_rules! isolated_test {
    ($test_name:ident, $test_fn:expr) => {
        #[test]
        fn $test_name() {
            $crate::test_isolation::IsolatedTest::run(stringify!($test_name), $test_fn);
        }
    };
}

/// Simplified test execution (no parallel support in no_std)
pub struct TestManager;

impl TestManager {
    pub fn new() -> Self {
        Self
    }

    /// Run tests sequentially with isolation
    pub fn run_tests<F>(&self, tests: Vec<(&str, F)>) -> Vec<Result<(), String>>
    where
        F: Fn(&mut TestEnvironment) -> Result<(), String>,
    {
        let mut results = Vec::new();

        for (test_name, test_fn) in tests {
            let result = IsolatedTest::run(test_name, |env| test_fn(env));
            results.push(result);
        }

        results
    }
}

/// Test environment factory for different test scenarios
pub struct TestEnvironmentFactory;

impl TestEnvironmentFactory {
    /// Create environment for unit tests
    pub fn unit_test_env(test_name: &str) -> TestEnvironment {
        let manager = get_test_manager();
        let mut manager_lock = manager.lock().unwrap();
        manager_lock.create_environment(test_name)
    }
    
    /// Create environment for integration tests
    pub fn integration_test_env(test_name: &str) -> TestEnvironment {
        let mut env = Self::unit_test_env(test_name);
        
        // Pre-populate with common test data
        env.create_verified_businesses(3);
        env.create_investors(5);
        env.create_currencies(2);
        
        env
    }
    
    /// Create environment for stress tests
    pub fn stress_test_env(test_name: &str) -> TestEnvironment {
        let mut env = Self::unit_test_env(test_name);
        
        // Pre-populate with larger amounts of test data
        env.create_verified_businesses(10);
        env.create_investors(50);
        env.create_currencies(5);
        
        env
    }
    
    /// Create environment for security tests
    pub fn security_test_env(test_name: &str) -> TestEnvironment {
        let mut env = Self::unit_test_env(test_name);
        
        // Create both legitimate and attacker addresses
        env.create_verified_businesses(2);
        env.create_investors(3);
        env.create_currencies(1);
        
        env
    }
}
