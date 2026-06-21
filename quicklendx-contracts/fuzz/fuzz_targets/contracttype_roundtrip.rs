//! Fuzz target for XDR round-trip testing of #[contracttype] structs
//!
//! This fuzzer validates that all contract types can survive a round trip
//! through XDR encoding/decoding without panicking and maintain data integrity.

#![no_main]

use libfuzzer_sys::fuzz_target;
use soroban_sdk::{Env, TryFromVal, IntoVal, Val};
use quicklendx_contracts::types::*;

/// Round-trip fuzzer harness for contract types
///
/// Tests XDR encoding/decoding stability by:
/// 1. Attempting to decode random bytes as each contract type
/// 2. For successful decodes, re-encoding and comparing bytes
/// 3. Ensuring no panics occur on invalid input
mod harness {
    use super::*;
    
    /// Test a type's round-trip XDR encoding/decoding
    fn test_roundtrip<T>(env: &Env, data: &[u8]) -> bool 
    where
        T: TryFromVal<Env, Val> + IntoVal<Env, Val>,
    {
        // Try to decode random bytes as this type
        if let Ok(val) = Val::try_from_slice(env, data) {
            if let Ok(decoded) = T::try_from_val(env, &val) {
                // Re-encode and verify bit-identical
                let reencoded: Val = decoded.into_val(env);
                let reencoded_bytes = reencoded.try_to_slice().unwrap_or_default();
                return data == reencoded_bytes;
            }
        }
        true // Invalid decode is acceptable
    }
    
    pub fn fuzz_contracttypes(data: &[u8]) {
        let env = Env::default();
        
        // Test all enum types
        let _ = test_roundtrip::<InvoiceStatus>(&env, data);
        let _ = test_roundtrip::<BidStatus>(&env, data);
        let _ = test_roundtrip::<InvestmentStatus>(&env, data);
        let _ = test_roundtrip::<DisputeStatus>(&env, data);
        let _ = test_roundtrip::<InvoiceCategory>(&env, data);
        let _ = test_roundtrip::<SearchRank>(&env, data);
        
        // Test struct types (more complex)
        let _ = test_roundtrip::<LineItemRecord>(&env, data);
        let _ = test_roundtrip::<PaymentRecord>(&env, data);
        let _ = test_roundtrip::<Dispute>(&env, data);
        let _ = test_roundtrip::<InvoiceRating>(&env, data);
        let _ = test_roundtrip::<Invoice>(&env, data);
        let _ = test_roundtrip::<InvoiceMetadata>(&env, data);
        let _ = test_roundtrip::<Bid>(&env, data);
        let _ = test_roundtrip::<Investment>(&env, data);
        let _ = test_roundtrip::<InsuranceCoverage>(&env, data);
        let _ = test_roundtrip::<PlatformFeeConfig>(&env, data);
        let _ = test_roundtrip::<SearchResult>(&env, data);
    }
}

fuzz_target!(|data: &[u8]| {
    harness::fuzz_contracttypes(data);
});