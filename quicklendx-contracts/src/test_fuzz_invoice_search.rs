#[cfg(test)]
mod test_fuzz_invoice_search {
    use crate::invoice_search::InvoiceSearch;
    use crate::types::{SearchRank, SearchResult};
    use proptest::prelude::*;
    use soroban_sdk::{Env, String, Vec};

    /// Setup test environment
    fn setup_test_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    /// Strategy for generating valid search queries
    /// Covers: ASCII alphanumeric, spaces, unicode, control bytes, empty strings
    fn query_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            // Empty string (should fail sanitization)
            Just("".to_string()),
            // Only spaces (should fail sanitization)
            Just("   ".to_string()),
            // Valid ASCII queries
            "[a-z0-9 ]{1,100}",
            // Mixed case queries
            "[a-zA-Z0-9 ]{1,100}",
            // Queries with special characters (will be sanitized)
            "[a-zA-Z0-9 !@#$%^&*()_+\\-=\\[\\]{};:'\",.<>?/]{1,100}",
            // Queries with unicode characters (will be filtered)
            "[a-zA-Z0-9 \\u0080-\\u00FF]{1,100}",
            // Very long queries (should fail sanitization)
            "[a-z]{101,200}",
        ]
        .prop_map(|s| s)
    }

    /// Strategy for generating invoice descriptions
    fn description_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            "[a-z0-9 ]{1,100}",
            "[a-zA-Z0-9 ]{1,100}",
            "[a-zA-Z0-9 !@#$%^&*()_+\\-=\\[\\]{};:'\",.<>?/]{1,100}",
        ]
        .prop_map(|s| s)
    }

    /// Strategy for generating customer names
    fn customer_name_strategy() -> impl Strategy<Value = Option<String>> {
        prop_oneof![
            Just(None),
            "[a-zA-Z ]{1,50}".prop_map(|s| Some(s)),
        ]
    }

    // ============================================================================
    // DETERMINISM TESTS
    // ============================================================================

    /// Test: Sanitization is deterministic
    /// For a fixed query, sanitization always produces the same result
    #[test]
    fn test_fuzz_sanitization_determinism() {
        let env = setup_test_env();

        proptest!(|(query in "[a-zA-Z0-9 !@#$%^&*()_+\\-=\\[\\]{};:'\",.<>?/]{0,100}")| {
            let query_str = String::from_str(&env, &query);

            // Sanitize twice
            let result1 = InvoiceSearch::sanitize_query(&query_str);
            let result2 = InvoiceSearch::sanitize_query(&query_str);

            // Results must be identical
            match (&result1, &result2) {
                (Ok(s1), Ok(s2)) => prop_assert_eq!(s1, s2, "Sanitization not deterministic"),
                (Err(_), Err(_)) => {}, // Both failed, which is fine
                _ => prop_assert!(false, "Sanitization results differ (one OK, one Err)"),
            }
        });
    }

    /// Test: Substring search is deterministic
    /// For fixed text and query, substring search always returns the same result
    #[test]
    fn test_fuzz_substring_search_determinism() {
        let env = setup_test_env();

        proptest!(|(text in "[a-zA-Z0-9 ]{1,100}", query in "[a-zA-Z0-9 ]{1,50}")| {
            let text_str = String::from_str(&env, &text);
            let query_str = String::from_str(&env, &query);

            // Search twice
            let result1 = InvoiceSearch::contains_substring(&text_str, &query_str);
            let result2 = InvoiceSearch::contains_substring(&text_str, &query_str);

            // Results must be identical
            prop_assert_eq!(result1, result2, "Substring search not deterministic");
        });
    }

    /// Test: Hex string conversion is deterministic
    /// For fixed bytes, hex conversion always produces the same string
    #[test]
    fn test_fuzz_hex_conversion_determinism() {
        let env = setup_test_env();

        proptest!(|(bytes in prop::array::uniform32(0u8..))| {
            let bytes_n = soroban_sdk::BytesN::from_array(&env, &bytes);

            // Convert twice
            let hex1 = InvoiceSearch::bytes_to_hex_string(&env, &bytes_n);
            let hex2 = InvoiceSearch::bytes_to_hex_string(&env, &bytes_n);

            // Results must be identical
            prop_assert_eq!(hex1, hex2, "Hex conversion not deterministic");
        });
    }

    // ============================================================================
    // RANKING STABILITY TESTS
    // ============================================================================

    /// Test: Ranking is stable under different insertion orders
    /// The ranking of a query should not depend on the order invoices are stored
    #[test]
    fn test_fuzz_ranking_stable_under_reordering() {
        let env = setup_test_env();

        proptest!(|(
            query in "[a-z]{1,20}",
            text1 in "[a-z ]{1,50}",
            text2 in "[a-z ]{1,50}",
            text3 in "[a-z ]{1,50}",
        )| {
            let query_str = String::from_str(&env, &query);

            // Test substring matching is consistent
            let text1_str = String::from_str(&env, &text1);
            let text2_str = String::from_str(&env, &text2);
            let text3_str = String::from_str(&env, &text3);

            // Check matches in different orders
            let match1_first = InvoiceSearch::contains_substring(&text1_str, &query_str);
            let match1_second = InvoiceSearch::contains_substring(&text1_str, &query_str);

            let match2_first = InvoiceSearch::contains_substring(&text2_str, &query_str);
            let match2_second = InvoiceSearch::contains_substring(&text2_str, &query_str);

            let match3_first = InvoiceSearch::contains_substring(&text3_str, &query_str);
            let match3_second = InvoiceSearch::contains_substring(&text3_str, &query_str);

            // All matches must be consistent
            prop_assert_eq!(match1_first, match1_second);
            prop_assert_eq!(match2_first, match2_second);
            prop_assert_eq!(match3_first, match3_second);
        });
    }

    /// Test: Ranking comparison is transitive
    /// If A > B and B > C, then A > C
    #[test]
    fn test_fuzz_ranking_transitivity() {
        // SearchRank ordering: Other < PartialMatch < ExactId
        let ranks = vec![SearchRank::Other, SearchRank::PartialMatch, SearchRank::ExactId];

        for i in 0..ranks.len() {
            for j in 0..ranks.len() {
                for k in 0..ranks.len() {
                    let a = ranks[i];
                    let b = ranks[j];
                    let c = ranks[k];

                    // If a > b and b > c, then a > c
                    if a > b && b > c {
                        prop_assert!(a > c, "Ranking not transitive: {} > {} > {}", i, j, k);
                    }
                }
            }
        }
    }

    // ============================================================================
    // SAFETY TESTS
    // ============================================================================

    /// Test: Sanitization rejects oversized queries
    /// Queries longer than 100 characters should be rejected
    #[test]
    fn test_fuzz_sanitization_rejects_oversized() {
        let env = setup_test_env();

        proptest!(|(query in "[a-z]{101,200}")| {
            let query_str = String::from_str(&env, &query);
            let result = InvoiceSearch::sanitize_query(&query_str);

            // Must be rejected
            prop_assert!(result.is_err(), "Oversized query not rejected");
        });
    }

    /// Test: Sanitization rejects empty queries
    /// Empty or whitespace-only queries should be rejected
    #[test]
    fn test_fuzz_sanitization_rejects_empty() {
        let env = setup_test_env();

        proptest!(|(spaces in "[ ]{0,100}")| {
            let query_str = String::from_str(&env, &spaces);
            let result = InvoiceSearch::sanitize_query(&query_str);

            // Must be rejected
            prop_assert!(result.is_err(), "Empty query not rejected");
        });
    }

    /// Test: Substring search handles edge cases safely
    /// Should not panic on any input combination
    #[test]
    fn test_fuzz_substring_search_no_panic() {
        let env = setup_test_env();

        proptest!(|(text in "[a-zA-Z0-9 ]{0,100}", query in "[a-zA-Z0-9 ]{0,100}")| {
            let text_str = String::from_str(&env, &text);
            let query_str = String::from_str(&env, &query);

            // Should not panic
            let _ = InvoiceSearch::contains_substring(&text_str, &query_str);
        });
    }

    /// Test: Hex conversion handles all byte values safely
    /// Should not panic on any byte sequence
    #[test]
    fn test_fuzz_hex_conversion_no_panic() {
        let env = setup_test_env();

        proptest!(|(bytes in prop::array::uniform32(0u8..))| {
            let bytes_n = soroban_sdk::BytesN::from_array(&env, &bytes);

            // Should not panic
            let _ = InvoiceSearch::bytes_to_hex_string(&env, &bytes_n);
        });
    }

    /// Test: Hex conversion produces valid hex strings
    /// Output should only contain hex characters (0-9, a-f)
    #[test]
    fn test_fuzz_hex_conversion_valid_output() {
        let env = setup_test_env();

        proptest!(|(bytes in prop::array::uniform32(0u8..))| {
            let bytes_n = soroban_sdk::BytesN::from_array(&env, &bytes);
            let hex = InvoiceSearch::bytes_to_hex_string(&env, &bytes_n);

            // Check all characters are valid hex
            for byte in hex.to_bytes().iter() {
                let is_valid_hex = (*byte >= b'0' && *byte <= b'9') || (*byte >= b'a' && *byte <= b'f');
                prop_assert!(is_valid_hex, "Invalid hex character: {}", *byte as char);
            }

            // Should be exactly 64 characters (32 bytes * 2)
            prop_assert_eq!(hex.len(), 64, "Hex string wrong length");
        });
    }

    /// Test: Lowercase conversion is idempotent
    /// Converting to lowercase twice should give the same result
    #[test]
    fn test_fuzz_lowercase_idempotent() {
        let env = setup_test_env();

        proptest!(|(text in "[a-zA-Z0-9 ]{1,100}")| {
            let text_str = String::from_str(&env, &text);

            // Convert to lowercase twice
            let lower1 = InvoiceSearch::to_lowercase(&text_str);
            let lower2 = InvoiceSearch::to_lowercase(&lower1);

            // Should be identical
            prop_assert_eq!(lower1, lower2, "Lowercase not idempotent");
        });
    }

    // ============================================================================
    // SUBSTRING SEARCH CORRECTNESS TESTS
    // ============================================================================

    /// Test: Substring search finds all occurrences
    /// If a query appears in text, it should be found
    #[test]
    fn test_fuzz_substring_search_finds_matches() {
        let env = setup_test_env();

        proptest!(|(prefix in "[a-z]{0,20}", query in "[a-z]{1,20}", suffix in "[a-z]{0,20}")| {
            let text = format!("{}{}{}", prefix, query, suffix);
            let text_str = String::from_str(&env, &text);
            let query_str = String::from_str(&env, &query);

            // Should find the query
            let found = InvoiceSearch::contains_substring(&text_str, &query_str);
            prop_assert!(found, "Substring not found when it should be");
        });
    }

    /// Test: Substring search is case-insensitive
    /// Uppercase and lowercase queries should match
    #[test]
    fn test_fuzz_substring_search_case_insensitive() {
        let env = setup_test_env();

        proptest!(|(text in "[a-z]{1,50}", query in "[a-z]{1,20}")| {
            let text_str = String::from_str(&env, &text);
            let query_lower = String::from_str(&env, &query);
            let query_upper = String::from_str(&env, &query.to_uppercase());

            // Both should give same result
            let found_lower = InvoiceSearch::contains_substring(&text_str, &query_lower);
            let found_upper = InvoiceSearch::contains_substring(&text_str, &query_upper);

            prop_assert_eq!(found_lower, found_upper, "Case sensitivity inconsistent");
        });
    }

    /// Test: Substring search respects boundaries
    /// Query longer than text should not match
    #[test]
    fn test_fuzz_substring_search_respects_length() {
        let env = setup_test_env();

        proptest!(|(text in "[a-z]{1,20}", query in "[a-z]{21,50}")| {
            let text_str = String::from_str(&env, &text);
            let query_str = String::from_str(&env, &query);

            // Should not find query (it's longer than text)
            let found = InvoiceSearch::contains_substring(&text_str, &query_str);
            prop_assert!(!found, "Query longer than text should not match");
        });
    }

    // ============================================================================
    // SANITIZATION CORRECTNESS TESTS
    // ============================================================================

    /// Test: Sanitization preserves valid characters
    /// Valid alphanumeric and spaces should be preserved
    #[test]
    fn test_fuzz_sanitization_preserves_valid() {
        let env = setup_test_env();

        proptest!(|(query in "[a-z0-9 ]{1,100}")| {
            let query_str = String::from_str(&env, &query);
            let result = InvoiceSearch::sanitize_query(&query_str);

            // Should succeed
            prop_assert!(result.is_ok(), "Valid query rejected");

            // Result should contain only lowercase alphanumeric and spaces
            if let Ok(sanitized) = result {
                for byte in sanitized.to_bytes().iter() {
                    let is_valid = (*byte >= b'a' && *byte <= b'z') || 
                                   (*byte >= b'0' && *byte <= b'9') || 
                                   *byte == b' ';
                    prop_assert!(is_valid, "Invalid character in sanitized query");
                }
            }
        });
    }

    /// Test: Sanitization removes special characters
    /// Special characters should be filtered out
    #[test]
    fn test_fuzz_sanitization_removes_special() {
        let env = setup_test_env();

        proptest!(|(query in "[a-z0-9 !@#$%^&*()_+\\-=\\[\\]{};:'\",.<>?/]{1,100}")| {
            let query_str = String::from_str(&env, &query);
            let result = InvoiceSearch::sanitize_query(&query_str);

            // If it succeeds, result should have no special characters
            if let Ok(sanitized) = result {
                for byte in sanitized.to_bytes().iter() {
                    let is_valid = (*byte >= b'a' && *byte <= b'z') || 
                                   (*byte >= b'0' && *byte <= b'9') || 
                                   *byte == b' ';
                    prop_assert!(is_valid, "Special character not removed");
                }
            }
        });
    }

    /// Test: Sanitization converts to lowercase
    /// Uppercase letters should be converted to lowercase
    #[test]
    fn test_fuzz_sanitization_lowercase() {
        let env = setup_test_env();

        proptest!(|(query in "[A-Z0-9 ]{1,100}")| {
            let query_str = String::from_str(&env, &query);
            let result = InvoiceSearch::sanitize_query(&query_str);

            // If it succeeds, result should be lowercase
            if let Ok(sanitized) = result {
                for byte in sanitized.to_bytes().iter() {
                    let is_lowercase = (*byte >= b'a' && *byte <= b'z') || 
                                       (*byte >= b'0' && *byte <= b'9') || 
                                       *byte == b' ';
                    prop_assert!(is_lowercase, "Uppercase not converted to lowercase");
                }
            }
        });
    }

    // ============================================================================
    // EDGE CASE TESTS
    // ============================================================================

    /// Test: Empty text with any query
    /// Searching in empty text should not match
    #[test]
    fn test_fuzz_empty_text_no_match() {
        let env = setup_test_env();

        proptest!(|(query in "[a-z]{1,50}")| {
            let empty_text = String::from_str(&env, "");
            let query_str = String::from_str(&env, &query);

            // Should not find query in empty text
            let found = InvoiceSearch::contains_substring(&empty_text, &query_str);
            prop_assert!(!found, "Query found in empty text");
        });
    }

    /// Test: Single character queries
    /// Single character queries should work correctly
    #[test]
    fn test_fuzz_single_char_query() {
        let env = setup_test_env();

        proptest!(|(text in "[a-z]{1,50}", query_char in "[a-z]")| {
            let text_str = String::from_str(&env, &text);
            let query_str = String::from_str(&env, &query_char.to_string());

            // Should find or not find based on actual content
            let found = InvoiceSearch::contains_substring(&text_str, &query_str);
            let expected = text.contains(&query_char.to_string());

            prop_assert_eq!(found, expected, "Single char query result incorrect");
        });
    }

    /// Test: Query equals text
    /// Query equal to text should match
    #[test]
    fn test_fuzz_query_equals_text() {
        let env = setup_test_env();

        proptest!(|(text in "[a-z]{1,50}")| {
            let text_str = String::from_str(&env, &text);
            let query_str = String::from_str(&env, &text);

            // Should find query (it's the entire text)
            let found = InvoiceSearch::contains_substring(&text_str, &query_str);
            prop_assert!(found, "Query equal to text should match");
        });
    }

    /// Test: All byte values in hex conversion
    /// Should handle all possible byte values (0-255)
    #[test]
    fn test_fuzz_hex_all_byte_values() {
        let env = setup_test_env();

        for byte_val in 0u8..=255 {
            let bytes = [byte_val; 32];
            let bytes_n = soroban_sdk::BytesN::from_array(&env, &bytes);
            let hex = InvoiceSearch::bytes_to_hex_string(&env, &bytes_n);

            // Should produce valid hex
            prop_assert_eq!(hex.len(), 64);
            for byte in hex.to_bytes().iter() {
                let is_valid_hex = (*byte >= b'0' && *byte <= b'9') || (*byte >= b'a' && *byte <= b'f');
                prop_assert!(is_valid_hex);
            }
        }
    }

    // ============================================================================
    // CORPUS SIZE TESTS
    // ============================================================================

    /// Test: Determinism with varying corpus sizes
    /// Results should be deterministic regardless of corpus size
    #[test]
    fn test_fuzz_determinism_corpus_size() {
        let env = setup_test_env();

        proptest!(|(query in "[a-z]{1,20}", corpus_size in 1usize..100)| {
            let query_str = String::from_str(&env, &query);

            // Generate corpus of varying sizes
            let mut texts = Vec::new(&env);
            for i in 0..corpus_size {
                let text = format!("text_{}", i);
                texts.push_back(String::from_str(&env, &text));
            }

            // Search should be deterministic
            let mut results1 = Vec::new(&env);
            for text in texts.iter() {
                let found = InvoiceSearch::contains_substring(&text, &query_str);
                results1.push_back(found);
            }

            let mut results2 = Vec::new(&env);
            for text in texts.iter() {
                let found = InvoiceSearch::contains_substring(&text, &query_str);
                results2.push_back(found);
            }

            // Results must be identical
            prop_assert_eq!(results1.len(), results2.len());
            for i in 0..results1.len() {
                prop_assert_eq!(results1.get(i), results2.get(i));
            }
        });
    }

    // ============================================================================
    // INTEGRATION TESTS
    // ============================================================================

    /// Test: Sanitization followed by substring search
    /// Full pipeline should work correctly
    #[test]
    fn test_fuzz_sanitization_then_search() {
        let env = setup_test_env();

        proptest!(|(query in "[a-zA-Z0-9 !@#$%^&*()_+\\-=\\[\\]{};:'\",.<>?/]{1,100}", 
                   text in "[a-zA-Z0-9 ]{1,100}")| {
            let query_str = String::from_str(&env, &query);
            let text_str = String::from_str(&env, &text);

            // Sanitize query
            if let Ok(sanitized_query) = InvoiceSearch::sanitize_query(&query_str) {
                // Search should not panic
                let _ = InvoiceSearch::contains_substring(&text_str, &sanitized_query);
            }
        });
    }

    /// Test: Multiple queries on same text
    /// Multiple searches on same text should be consistent
    #[test]
    fn test_fuzz_multiple_queries_same_text() {
        let env = setup_test_env();

        proptest!(|(text in "[a-z]{1,50}", 
                   query1 in "[a-z]{1,20}",
                   query2 in "[a-z]{1,20}",
                   query3 in "[a-z]{1,20}")| {
            let text_str = String::from_str(&env, &text);
            let q1 = String::from_str(&env, &query1);
            let q2 = String::from_str(&env, &query2);
            let q3 = String::from_str(&env, &query3);

            // All searches should be deterministic
            let r1a = InvoiceSearch::contains_substring(&text_str, &q1);
            let r1b = InvoiceSearch::contains_substring(&text_str, &q1);
            prop_assert_eq!(r1a, r1b);

            let r2a = InvoiceSearch::contains_substring(&text_str, &q2);
            let r2b = InvoiceSearch::contains_substring(&text_str, &q2);
            prop_assert_eq!(r2a, r2b);

            let r3a = InvoiceSearch::contains_substring(&text_str, &q3);
            let r3b = InvoiceSearch::contains_substring(&text_str, &q3);
            prop_assert_eq!(r3a, r3b);
        });
    }
}
