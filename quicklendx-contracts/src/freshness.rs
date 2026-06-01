use soroban_sdk::{contract, contractimpl, contracttype, contracterror, Env, Symbol, Address, panic_with_error};

#[cfg(all(test, feature = "legacy-tests"))]
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    MaxFreshnessDriftSecs,
}

#[cfg(all(test, feature = "legacy-tests"))]
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FreshnessError {
    NotAuthorized = 1,
    StaleDataRejected = 2,
    InvalidConfigValue = 3,
}

#[cfg(all(test, feature = "legacy-tests"))]
#[contract]
pub struct FreshnessContract;

#[cfg(all(test, feature = "legacy-tests"))]
#[contractimpl]
impl FreshnessContract {
    
    /// Sets the maximum allowable freshness oracle drift in seconds.
    pub fn set_max_freshness_drift_secs(env: Env, drift_secs: u64) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, FreshnessError::NotAuthorized));
        admin.require_auth();
        
        if drift_secs == 0 {
            panic_with_error!(&env, FreshnessError::InvalidConfigValue);
        }
        
        env.storage().instance().set(&DataKey::MaxFreshnessDriftSecs, &drift_secs);
        
        env.events().publish(
            (Symbol::new(&env, "freshness_config_updated"),),
            drift_secs
        );
    }

    /// Returns the configured drift parameter or a highly conservative default (60 seconds).
    pub fn get_max_freshness_drift_secs(env: Env) -> u64 {
        env.storage().instance().get(&DataKey::MaxFreshnessDriftSecs).unwrap_or(60)
    }

    /// Consumed by callers needing data-freshness checks. Enforces upper bounds on data age.
    pub fn get_freshness(env: Env, oracle_timestamp: u64) -> u64 {
        let current_time = env.ledger().timestamp();
        
        if current_time < oracle_timestamp {
            panic_with_error!(&env, FreshnessError::InvalidConfigValue);
        }
        
        let age = current_time - oracle_timestamp;
        let max_drift = Self::get_max_freshness_drift_secs(env.clone());
        
        if age > max_drift {
            env.events().publish(
                (Symbol::new(&env, "freshness_rejected"),),
                (oracle_timestamp, current_time, max_drift)
            );
            panic_with_error!(&env, FreshnessError::StaleDataRejected);
        }
        
        age
    }
}


#[derive(Clone, Debug)]
pub struct FreshnessMetadata {
    pub last_indexed_ledger: u32,
    pub index_lag_seconds: i64,
    pub last_updated_at: soroban_sdk::String,
    pub cursor: soroban_sdk::String,
}

impl FreshnessMetadata {
    pub fn from_env(
        env: &Env,
        indexed_ledger_seq: u32,
        indexed_ledger_timestamp: u64,
        offset: u32,
    ) -> Self {
        let current_time = env.ledger().timestamp();
        let lag = (current_time as i64).saturating_sub(indexed_ledger_timestamp as i64);
        
        Self {
            last_indexed_ledger: indexed_ledger_seq,
            index_lag_seconds: lag,
            last_updated_at: crate::i64_to_string_lib(env, indexed_ledger_timestamp as i64),
            cursor: crate::u32_to_string_lib(env, indexed_ledger_seq.saturating_add(offset)),
        }
    }
}

/* ==========================================
   YOUR TEST SUITE (APPENDED BELOW LOGIC)
   ========================================== */
#[cfg(all(test, feature = "legacy-tests"))]
mod test_freshness {
    use super::*;
    use soroban_sdk::{testutils::{Ledger, Events, MockAuth, MockAuthInvoke}, Env, Address, Symbol, IntoVal};

    fn setup_test_env() -> (Env, Address, FreshnessContractClient<'static>) {
        let env = Env::default();
        let admin = Address::generate(&env);
        
        let contract_id = env.register_contract(None, FreshnessContract);
        let client = FreshnessContractClient::new(&env, &contract_id);
        
        env.storage().instance().set(&DataKey::Admin, &admin);
        
        (env, admin, client)
    }

    #[test]
    fn test_default_conservative_drift() {
        let (env, _, client) = setup_test_env();
        assert_eq!(client.get_max_freshness_drift_secs(), 60);
    }

    #[test]
    fn test_set_drift_admin_gated() {
        let (env, admin, client) = setup_test_env();
        
        env.mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                function: "set_max_freshness_drift_secs",
                args: (120u64,).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        
        client.set_max_freshness_drift_secs(&120);
        assert_eq!(client.get_max_freshness_drift_secs(), 120);
    }

    #[test]
    fn test_get_freshness_below_drift() {
        let (env, admin, client) = setup_test_env();
        
        env.mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                function: "set_max_freshness_drift_secs",
                args: (300u64,).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.set_max_freshness_drift_secs(&300);
        
        env.ledger().set_timestamp(1000);
        let age = client.get_freshness(&701);
        assert_eq!(age, 299);
    }

    #[test]
    fn test_get_freshness_exact_drift() {
        let (env, admin, client) = setup_test_env();
        
        env.mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                function: "set_max_freshness_drift_secs",
                args: (300u64,).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.set_max_freshness_drift_secs(&300);
        
        env.ledger().set_timestamp(1000);
        let age = client.get_freshness(&700);
        assert_eq!(age, 300);
    }

    #[test]
    fn test_get_freshness_above_drift_rejection() {
        let (env, admin, client) = setup_test_env();
        
        env.mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                function: "set_max_freshness_drift_secs",
                args: (300u64,).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.set_max_freshness_drift_secs(&300);
        
        env.ledger().set_timestamp(1000);
        
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.get_freshness(&699);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_rejection_event_emitted() {
        let (env, admin, client) = setup_test_env();
        
        env.mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                function: "set_max_freshness_drift_secs",
                args: (300u64,).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.set_max_freshness_drift_secs(&300);
        
        env.ledger().set_timestamp(1000);
        
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.get_freshness(&699);
        }));
        
        let events = env.events().all();
        let rejection_event = events.iter().find(|e| {
            e.topics.contains(&Symbol::new(&env, "freshness_rejected").into_val(&env))
        });
        
        assert!(rejection_event.is_some(), "Rejection event was not published");
    }
}