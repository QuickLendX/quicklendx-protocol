//! Test validation for contracttype round-trip fuzzer
//! 
//! This validates that the fuzzing logic works correctly without requiring libfuzzer-sys

use soroban_sdk::{Env, TryFromVal, IntoVal, Val};
use quicklendx_contracts::types::*;

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

fn fuzz_contracttypes(data: &[u8]) -> bool {
    let env = Env::default();
    
    // Test all enum types
    test_roundtrip::<InvoiceStatus>(&env, data) &&
    test_roundtrip::<BidStatus>(&env, data) &&
    test_roundtrip::<InvestmentStatus>(&env, data) &&
    test_roundtrip::<DisputeStatus>(&env, data) &&
    test_roundtrip::<InvoiceCategory>(&env, data) &&
    test_roundtrip::<SearchRank>(&env, data) &&
    
    // Test struct types
    test_roundtrip::<LineItemRecord>(&env, data) &&
    test_roundtrip::<PaymentRecord>(&env, data) &&
    test_roundtrip::<Dispute>(&env, data) &&
    test_roundtrip::<InvoiceRating>(&env, data) &&
    test_roundtrip::<Invoice>(&env, data) &&
    test_roundtrip::<InvoiceMetadata>(&env, data) &&
    test_roundtrip::<Bid>(&env, data) &&
    test_roundtrip::<Investment>(&env, data) &&
    test_roundtrip::<InsuranceCoverage>(&env, data) &&
    test_roundtrip::<PlatformFeeConfig>(&env, data) &&
    test_roundtrip::<SearchResult>(&env, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        assert!(fuzz_contracttypes(&[]));
    }

    #[test]
    fn test_random_input() {
        let test_cases = vec![
            vec![0x00],
            vec![0xFF],
            vec![0x00, 0x01, 0x02, 0x03],
            vec![0xFF; 100],
            (0..255).collect::<Vec<u8>>(),
        ];
        
        for case in test_cases {
            assert!(fuzz_contracttypes(&case), "Failed on input: {:?}", case);
        }
    }

    #[test]
    fn test_large_input() {
        let large_input = vec![0xAA; 10000];
        assert!(fuzz_contracttypes(&large_input));
    }
}

fn main() {
    println!("Running fuzzer logic validation...");
    
    // Test with various inputs
    let test_inputs = vec![
        vec![],
        vec![0],
        vec![0x00, 0x01, 0x02, 0x03],
        vec![0xFF; 32],
        (0u8..=255).cycle().take(1000).collect(),
    ];
    
    let mut all_passed = true;
    for (i, input) in test_inputs.iter().enumerate() {
        match std::panic::catch_unwind(|| fuzz_contracttypes(input)) {
            Ok(result) => {
                println!("Test {}: {} bytes - {}", i + 1, input.len(), 
                         if result { "PASS" } else { "FAIL" });
                if !result {
                    all_passed = false;
                }
            }
            Err(_) => {
                println!("Test {}: {} bytes - PANIC", i + 1, input.len());
                all_passed = false;
            }
        }
    }
    
    if all_passed {
        println!("All fuzzer validation tests passed!");
    } else {
        println!("Some tests failed - fuzzer needs debugging");
        std::process::exit(1);
    }
}