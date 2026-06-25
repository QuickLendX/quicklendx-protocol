//! Protocol health status reporting.
//!
//! This module provides a comprehensive snapshot of the protocol's current state
//! through a single canonical `ProtocolHealth` struct. This is intended as the
//! heartbeat endpoint for off-chain dashboards, monitoring systems, and governance
//! tooling.
//!
//! # Security Model
//!
//! - **Read-only**: No authentication required; all getters are view-only
//! - **Non-invasive**: Purely advisory; returning health data does not mutate any state
//! - **Pause-exempt**: Health endpoint remains available even when the protocol is paused
//! - **PII-safe**: Contains only aggregate counts and system configuration; no user/business data leaks
//!
//! # Fields
//!
//! The `ProtocolHealth` struct aggregates:
//! - **version**: Protocol version (from initialization)
//! - **initialized**: Whether the contract has been set up
//! - **paused**: Current pause state
//! - **emergency_withdraw_pending**: Optional pending emergency withdrawal details
//! - **treasury**: Optional treasury address for fee collection
//! - **fee_bps**: Fee basis points (0-1000)
//! - **total_invoice_count**: Total number of invoices across all statuses
//! - **currency_count**: Number of whitelisted currencies
//!
//! # Example Usage (Pseudo-Rust)
//!
//! ```ignore
//! let health = get_protocol_health(&env);
//! println!("Protocol version: {}", health.version);
//! println!("Paused: {}", health.paused);
//! if let Some(pending) = &health.emergency_withdraw_pending {
//!     println!("Emergency withdraw pending: expires_at = {}", pending.expires_at);
//! }
//! println!("Treasury: {:?}", health.treasury);
//! println!("Invoices: {}", health.total_invoice_count);
//! println!("Currencies: {}", health.currency_count);
//! ```

use crate::emergency::EmergencyWithdraw;
use soroban_sdk::{contracttype, Address};

/// Canonical protocol health snapshot.
///
/// All fields are read-only aggregates of current protocol state. This struct
/// is updated on every `get_protocol_health()` call; the snapshot is fresh.
///
/// # Fields with special note
///
/// - **emergency_withdraw_pending**: If Some, indicates a timelock is in progress.
///   Consult `emg_time_until_unlock()` and `emg_time_until_expire()` for timing details.
/// - **treasury**: May be None if not configured; fee collection is no-op in that case.
/// - **total_invoice_count**: Sum of all invoices across all statuses (Pending, Verified,
///   Funded, Paid, Defaulted, Cancelled, Refunded).
/// - **currency_count**: Number of addresses in the whitelist. Operations require
///   at least one whitelisted currency.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub struct ProtocolHealth {
    /// Protocol version number written at initialization.
    /// Reflects the PROTOCOL_VERSION constant from init.rs at the time
    /// the contract was first deployed.
    pub version: u32,

    /// Whether the protocol has completed initialization.
    /// `true` = initialized and operational; `false` = awaiting initialize().
    pub initialized: bool,

    /// Current pause state of the protocol.
    /// `true` = paused (business operations frozen); `false` = normal operation.
    /// Admin-only read-only entrypoints and emergency recovery bypass this flag.
    pub paused: bool,

    /// Whether an emergency withdrawal timelock is currently pending.
    pub emergency_withdraw_pending: bool,

    /// Treasury address for fee collection (may be None).
    /// Fee calculations are performed even if treasury is not set (fees accrue
    /// but do not get transferred until treasury is configured).
    pub treasury: Option<Address>,

    /// Fee basis points applied to profit settlements (0-1000).
    /// Example: 200 means 2% fee. Controlled by admin via set_fee_config().
    pub fee_bps: u32,

    /// Total number of invoices across all statuses.
    /// Includes: Pending, Verified, Funded, Paid, Defaulted, Cancelled, Refunded.
    pub total_invoice_count: u32,

    /// Total number of whitelisted currencies.
    /// At least one currency must be whitelisted for the protocol to accept invoices.
    pub currency_count: u32,
}

impl ProtocolHealth {
    /// Construct a ProtocolHealth snapshot from current contract state.
    ///
    /// This is a read-only snapshot operation. All data is pulled directly from
    /// contract storage with fresh reads; no caching is performed.
    ///
    /// # Arguments
    /// * `env` - The contract environment (provides access to storage and ledger)
    ///
    /// # Returns
    /// A `ProtocolHealth` struct containing the current state.
    ///
    /// # Security
    /// - No authentication required
    /// - No state mutations
    /// - Safe to call from any context (read-only getter)
    pub fn new(env: &soroban_sdk::Env) -> Self {
        use crate::admin::AdminStorage;
        use crate::currency::CurrencyWhitelist;
        use crate::emergency::EmergencyWithdraw;
        use crate::init::ProtocolInitializer;
        use crate::pause::PauseControl;

        ProtocolHealth {
            version: ProtocolInitializer::get_version(env),
            initialized: ProtocolInitializer::is_initialized(env),
            paused: PauseControl::is_paused(env),
            emergency_withdraw_pending: EmergencyWithdraw::get_pending(env).is_some(),
            treasury: ProtocolInitializer::get_treasury(env),
            fee_bps: ProtocolInitializer::get_fee_bps(env),
            total_invoice_count: crate::storage::InvoiceStorage::get_total_count(env) as u32,
            currency_count: CurrencyWhitelist::currency_count(env),
        }
    }
}

#[cfg(all(test, feature = "legacy-tests"))]
mod tests {
    use super::*;
    use crate::admin::AdminStorage;
    use crate::currency::CurrencyWhitelist;
    use crate::init::ProtocolInitializer;
    use crate::pause::PauseControl;
    use soroban_sdk::{Address, Env};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::QuickLendXContract, ());
        (env, contract_id)
    }

    fn setup_initialized() -> (Env, Address, Address) {
        let (env, contract_id) = setup();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let currency = Address::generate(&env);

        // Initialize with basic configuration
        let params = crate::init::InitializationParams {
            admin: admin.clone(),
            treasury: treasury.clone(),
            fee_bps: 200,
            min_invoice_amount: 1000,
            max_due_date_days: 365,
            grace_period_seconds: 604800,
            initial_currencies: {
                let mut v = soroban_sdk::Vec::new(&env);
                v.push_back(currency);
                v
            },
        };

        ProtocolInitializer::initialize(&env, &params).expect("init failed");
        (env, contract_id, admin)
    }

    #[test]
    fn test_health_uninitialized() {
        let (env, _) = setup();
        let health = ProtocolHealth::new(&env);

        assert_eq!(health.version, 1);
        assert!(!health.initialized);
        assert!(!health.paused);
        assert_eq!(health.fee_bps, 0); // Default when uninitialized
        assert!(health.treasury.is_none());
        assert_eq!(health.total_invoice_count, 0);
        assert_eq!(health.currency_count, 0);
        assert!(!health.emergency_withdraw_pending);
    }

    #[test]
    fn test_health_initialized() {
        let (env, _, _) = setup_initialized();
        let health = ProtocolHealth::new(&env);

        assert_eq!(health.version, 1);
        assert!(health.initialized);
        assert!(!health.paused);
        assert_eq!(health.fee_bps, 200);
        assert!(health.treasury.is_some());
        assert_eq!(health.total_invoice_count, 0);
        assert_eq!(health.currency_count, 1);
        assert!(!health.emergency_withdraw_pending);
    }

    #[test]
    fn test_health_paused() {
        let (env, _, admin) = setup_initialized();
        let health_before = ProtocolHealth::new(&env);
        assert!(!health_before.paused);

        // Pause the protocol
        PauseControl::set_paused(&env, &admin, true).expect("pause failed");

        let health_after = ProtocolHealth::new(&env);
        assert!(health_after.paused);
    }

    #[test]
    fn test_health_fee_update() {
        let (env, _, admin) = setup_initialized();

        let health_before = ProtocolHealth::new(&env);
        assert_eq!(health_before.fee_bps, 200);

        // Update fee config
        ProtocolInitializer::set_fee_config(&env, &admin, 300)
            .expect("set_fee_config failed");

        let health_after = ProtocolHealth::new(&env);
        assert_eq!(health_after.fee_bps, 300);
    }

    #[test]
    fn test_health_currency_count() {
        let (env, _, admin) = setup_initialized();

        let health_initial = ProtocolHealth::new(&env);
        assert_eq!(health_initial.currency_count, 1);

        // Add another currency
        let new_currency = Address::generate(&env);
        CurrencyWhitelist::add_currency(&env, &admin, new_currency)
            .expect("add_currency failed");

        let health_after = ProtocolHealth::new(&env);
        assert_eq!(health_after.currency_count, 2);
    }

    #[test]
    fn test_health_is_read_only() {
        // Calling ProtocolHealth::new multiple times should yield
        // identical results (modulo fresh timestamp reads if applicable)
        let (env, _, _) = setup_initialized();

        let health1 = ProtocolHealth::new(&env);
        let health2 = ProtocolHealth::new(&env);

        // Core fields should be identical
        assert_eq!(health1.version, health2.version);
        assert_eq!(health1.initialized, health2.initialized);
        assert_eq!(health1.paused, health2.paused);
        assert_eq!(health1.fee_bps, health2.fee_bps);
        assert_eq!(health1.total_invoice_count, health2.total_invoice_count);
        assert_eq!(health1.currency_count, health2.currency_count);
    }

    #[test]
    fn test_health_all_fields_populated() {
        // Verify every field of ProtocolHealth is populated and accessible
        let (env, _, _) = setup_initialized();
        let health = ProtocolHealth::new(&env);

        // Access each field to ensure no panics and proper typing
        let _v = health.version;
        let _i = health.initialized;
        let _p = health.paused;
        let _e = health.emergency_withdraw_pending;
        let _t = health.treasury;
        let _f = health.fee_bps;
        let _ic = health.total_invoice_count;
        let _cc = health.currency_count;
    }

    #[test]
    fn test_health_emergency_withdraw_pending() {
        // This test verifies the emergency_withdraw_pending field is false
        // when no emergency withdrawal is pending, and true when one is.
        // Full emergency withdraw testing is in test_emergency.rs.
        let (env, _, _) = setup_initialized();

        let health = ProtocolHealth::new(&env);
        assert!(!health.emergency_withdraw_pending);

        // Note: Full emergency withdrawal state testing requires creating
        // an actual pending emergency withdrawal, which is tested in test_emergency.rs
    }
}
