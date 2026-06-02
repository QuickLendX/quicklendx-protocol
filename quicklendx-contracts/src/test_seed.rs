//! Unified seed management for deterministic testing across all fuzz harnesses.

#[cfg(feature = "fuzz-tests")]
use std::env;

#[cfg(feature = "fuzz-tests")]
pub const SEED_ENV_VAR: &str = "QUICKLENDX_SEED";

/// Get a deterministic seed from environment or fallback to OS random
#[cfg(feature = "fuzz-tests")]
pub fn seed() -> Option<u64> {
    match env::var(SEED_ENV_VAR) {
        Ok(seed_str) => {
            seed_str.parse::<u64>()
                .map(Some)
                .unwrap_or_else(|_| panic!(
                    "Invalid {}={}: must be a valid u64", 
                    SEED_ENV_VAR, seed_str
                ))
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_seed_deterministic() {
        env::set_var(SEED_ENV_VAR, "42");
        let result = seed();
        env::remove_var(SEED_ENV_VAR);
        assert!(result.is_some());
    }

    #[test]
    fn test_seed_fallback() {
        env::remove_var(SEED_ENV_VAR);
        assert_eq!(seed(), None);
    }
}