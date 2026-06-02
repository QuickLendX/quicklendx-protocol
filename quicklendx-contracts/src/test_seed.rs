//! Unified seed management for deterministic testing across all fuzz harnesses.

use std::env;

/// Environment variable name for controlling test seeding
pub const SEED_ENV_VAR: &str = "QUICKLENDX_SEED";

/// Get a deterministic seed from environment or fallback to OS random
///
/// Returns `Some([u8; 32])` if QUICKLENDX_SEED is set, `None` for OS random.
/// Panics if QUICKLENDX_SEED is set but invalid.
pub fn seed() -> Option<[u8; 32]> {
    match env::var(SEED_ENV_VAR) {
        Ok(seed_str) => {
            let seed_value = seed_str.parse::<u64>()
                .unwrap_or_else(|_| panic!(
                    "Invalid {}={}: must be a valid u64", 
                    SEED_ENV_VAR, seed_str
                ));
            
            let mut seed_array = [0u8; 32];
            let seed_bytes = seed_value.to_le_bytes();
            for chunk in seed_array.chunks_mut(8) {
                chunk.copy_from_slice(&seed_bytes);
            }
            Some(seed_array)
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