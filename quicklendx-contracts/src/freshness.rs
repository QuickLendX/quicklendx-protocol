use soroban_sdk::{contract, contractimpl, contracttype, Env, Symbol, Address, panic_with_error};
use crate::errors::QuickLendXError;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    MaxFreshnessDriftSecs,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FreshnessMetadata {
    pub last_indexed_ledger: u32,
    pub last_indexed_timestamp: u64,
    pub offset: u32,
}

impl FreshnessMetadata {
    pub fn from_env(
        env: &Env,
        indexed_ledger_seq: u32,
        indexed_ledger_timestamp: u64,
        offset: u32,
    ) -> Self {
        Self {
            last_indexed_ledger: indexed_ledger_seq,
            last_indexed_timestamp: indexed_ledger_timestamp,
            offset,
        }
    }
}

#[contract]
pub struct FreshnessContract;

#[contractimpl]
impl FreshnessContract {
    
    /// Sets the maximum allowable freshness oracle drift in seconds.
    pub fn set_max_freshness_drift_secs(env: Env, drift_secs: u64) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, QuickLendXError::NotAdmin));
        admin.require_auth();
        
        if drift_secs == 0 {
            panic_with_error!(&env, QuickLendXError::InvalidTimestamp);
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
            panic_with_error!(&env, QuickLendXError::InvalidTimestamp);
        }
        
        let age = current_time - oracle_timestamp;
        let max_drift = Self::get_max_freshness_drift_secs(env.clone());
        
        if age > max_drift {
            env.events().publish(
                (Symbol::new(&env, "freshness_rejected"),),
                (oracle_timestamp, current_time, max_drift)
            );
            panic_with_error!(&env, QuickLendXError::InvalidTimestamp);
        }
        
        age
    }
}

/* ==========================================
   YOUR TEST SUITE (APPENDED BELOW LOGIC)
   ========================================== */
#[cfg(test)]
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