use soroban_sdk::{BytesN, Env, String, Vec};

use crate::errors::QuickLendXError;
use crate::storage::InvoiceStorage;
use crate::types::{Invoice, InvoiceStatus, SearchRank, SearchResult};

/// Maximum number of search results to return
pub const MAX_SEARCH_RESULTS: u32 = 50;

/// Invoice search functionality with safe query semantics and relevance ranking
pub struct InvoiceSearch;

impl InvoiceSearch {
    /// Sanitize search query to prevent injection and ensure safe processing
    ///
    /// # Security Notes
    /// - Trims whitespace
    /// - Limits length to prevent buffer overflow
    /// - Rejects empty queries
    /// - Only allows printable ASCII characters
    pub fn sanitize_query(query: &String) -> Result<String, QuickLendXError> {
        let len = query.len() as usize;
        if len == 0 {
            return Err(QuickLendXError::InvalidDescription);
        }
        if len > 100 {
            return Err(QuickLendXError::InvalidDescription);
        }

        // Convert to lowercase for case-insensitive search
        let mut sanitized_bytes = alloc::vec::Vec::new();
        for byte in query.to_bytes().iter() {
            if byte >= b'A' && byte <= b'Z' {
                sanitized_bytes.push(byte + 32); // Convert to lowercase
            } else if byte >= b'a' && byte <= b'z' || byte >= b'0' && byte <= b'9' || byte == b' ' {
                sanitized_bytes.push(byte);
            }
            // Skip other characters (punctuation, etc.)
        }

        let trimmed_str = core::str::from_utf8(&sanitized_bytes).unwrap_or("").trim();
        if trimmed_str.len() == 0 {
            return Err(QuickLendXError::InvalidDescription);
        }

        Ok(String::from_str(query.env(), trimmed_str))
    }

    /// Search invoices with relevance ranking
    ///
    /// # Arguments
    /// * `env` - Soroban environment
    /// * `query` - Search query string (will be sanitized)
    ///
    /// # Returns
    /// * `Vec<SearchResult>` - Ranked search results, limited to MAX_SEARCH_RESULTS
    ///
    /// # Ranking Logic
    /// 1. Exact matches on invoice_id (highest priority)
    /// 2. Partial matches on description or customer_name
    /// 3. Sort by created_at timestamp (newest first) within same rank
    ///
    /// # Security Notes
    /// - Input sanitization prevents injection attacks
    /// - Memory-safe: bounded result set prevents DoS
    /// - Case-insensitive search
    pub fn search_invoices(env: &Env, query: String) -> Result<Vec<SearchResult>, QuickLendXError> {
        let sanitized_query = Self::sanitize_query(&query)?;

        // Get all invoices (in a real implementation, this might be paginated or indexed)
        // For now, we'll search through all invoices
        let all_invoices = Self::get_all_invoice_ids(env);

        let mut results = Vec::new(env);

        for invoice_id in all_invoices.iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(env, &invoice_id) {
                let rank = Self::calculate_rank(env, &invoice, &sanitized_query);
                if rank != SearchRank::Other {
                    results.push_back(SearchResult {
                        invoice_id,
                        rank,
                        created_at: invoice.created_at,
                    });
                }
            }
        }

        // Sort by rank (descending) then by created_at (descending)
        Self::sort_results(&mut results);

        // Limit results
        let mut limited_results = Vec::new(env);
        let max_results = MAX_SEARCH_RESULTS.min(results.len() as u32);
        for i in 0..max_results {
            if let Some(result) = results.get(i) {
                limited_results.push_back(result);
            }
        }

        Ok(limited_results)
    }

    /// Calculate search relevance rank for an invoice
    fn calculate_rank(env: &Env, invoice: &Invoice, query: &String) -> SearchRank {
        // Check for exact invoice ID match (convert to hex string)
        let invoice_id_hex = Self::bytes_to_hex_string(env, &invoice.id);
        if invoice_id_hex == *query {
            return SearchRank::ExactId;
        }

        // Check partial matches in description
        if Self::contains_substring(&invoice.description, query) {
            return SearchRank::PartialMatch;
        }

        // Check partial matches in customer name (if available)
        if let Some(customer_name) = &invoice.metadata_customer_name {
            if Self::contains_substring(customer_name, query) {
                return SearchRank::PartialMatch;
            }
        }

        SearchRank::Other
    }

    /// Check if text contains query as substring (case-insensitive)
    fn contains_substring(text: &String, query: &String) -> bool {
        let text_lower = Self::to_lowercase(text);
        let query_lower = Self::to_lowercase(query);

        // Simple substring search
        if text_lower.len() < query_lower.len() {
            return false;
        }

        let text_lower_bytes = text_lower.to_bytes();
        let query_lower_bytes = query_lower.to_bytes();

        for i in 0..=(text_lower.len() - query_lower.len()) {
            let mut matches = true;
            for j in 0..query_lower.len() {
                if text_lower_bytes.get(i + j).unwrap() != query_lower_bytes.get(j).unwrap() {
                    matches = false;
                    break;
                }
            }
            if matches {
                return true;
            }
        }

        false
    }

    /// Convert string to lowercase
    fn to_lowercase(s: &String) -> String {
        let mut result_bytes = alloc::vec::Vec::new();
        for byte in s.to_bytes().iter() {
            if byte >= b'A' && byte <= b'Z' {
                result_bytes.push(byte + 32);
            } else {
                result_bytes.push(byte);
            }
        }
        String::from_str(s.env(), core::str::from_utf8(&result_bytes).unwrap_or(""))
    }

    /// Convert BytesN<32> to hex string for comparison
    fn bytes_to_hex_string(env: &Env, bytes: &BytesN<32>) -> String {
        let mut hex_bytes = alloc::vec::Vec::new();
        for byte in bytes.to_array().iter() {
            let high = byte >> 4;
            let low = byte & 0x0F;
            hex_bytes.push(Self::nibble_to_hex(high));
            hex_bytes.push(Self::nibble_to_hex(low));
        }
        String::from_str(env, core::str::from_utf8(&hex_bytes).unwrap_or(""))
    }

    /// Convert nibble to hex character
    fn nibble_to_hex(nibble: u8) -> u8 {
        if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + (nibble - 10)
        }
    }

    /// Get all invoice IDs (helper - in production this might be optimized)
    fn get_all_invoice_ids(env: &Env) -> Vec<BytesN<32>> {
        // This is a simplified implementation. In a real system, you might have
        // a more efficient way to iterate through all invoices.
        // For now, we'll get invoices from different status lists.

        let mut all_ids = Vec::new(env);

        // Get from all status lists
        let statuses = [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
            InvoiceStatus::Defaulted,
            InvoiceStatus::Cancelled,
            InvoiceStatus::Refunded,
        ];

        for status in statuses {
            let status_invoices = InvoiceStorage::get_invoices_by_status(env, status);
            for invoice_id in status_invoices.iter() {
                // Avoid duplicates
                if !Self::contains_id(&all_ids, &invoice_id) {
                    all_ids.push_back(invoice_id);
                }
            }
        }

        all_ids
    }

    /// Check if vector contains invoice ID
    fn contains_id(vec: &Vec<BytesN<32>>, id: &BytesN<32>) -> bool {
        for existing_id in vec.iter() {
            if existing_id == *id {
                return true;
            }
        }
        false
    }

    /// Sort search results by rank (desc) then created_at (desc)
    fn sort_results(results: &mut Vec<SearchResult>) {
        // Simple bubble sort for small result sets (MAX_SEARCH_RESULTS = 50)
        let len = results.len();
        for i in 0..len {
            for j in 0..(len - i - 1) {
                if let (Some(a), Some(b)) = (results.get(j), results.get(j + 1)) {
                    let should_swap = match a.rank.cmp(&b.rank) {
                        core::cmp::Ordering::Less => true,
                        core::cmp::Ordering::Equal => a.created_at < b.created_at,
                        core::cmp::Ordering::Greater => false,
                    };

                    if should_swap {
                        // Swap elements
                        let temp = results.get(j).unwrap();
                        results.set(j, results.get(j + 1).unwrap());
                        results.set(j + 1, temp);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env, String, Vec};

    use crate::types::{DisputeStatus, Invoice, InvoiceCategory, InvoiceStatus, SearchRank};

    fn setup_test_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn create_test_invoice(
        env: &Env,
        business: &Address,
        description: &str,
        customer_name: Option<&str>,
    ) -> Invoice {
        let mut invoice = Invoice::new(
            env,
            business.clone(),
            1000,
            Address::generate(env),
            env.ledger().timestamp() + 86400,
            String::from_str(env, description),
            InvoiceCategory::Services,
            Vec::new(env),
        )
        .unwrap();
        invoice.metadata_customer_name = customer_name.map(|name| String::from_str(env, name));
        invoice
    }

    #[test]
    fn test_sanitize_query() {
        let env = setup_test_env();

        // Valid query
        let query = String::from_str(&env, "test query");
        let sanitized = InvoiceSearch::sanitize_query(&query).unwrap();
        assert_eq!(sanitized, String::from_str(&env, "test query"));

        // Mixed case
        let query = String::from_str(&env, "Test Query");
        let sanitized = InvoiceSearch::sanitize_query(&query).unwrap();
        assert_eq!(sanitized, String::from_str(&env, "test query"));

        // With punctuation (should be removed)
        let query = String::from_str(&env, "test-query!");
        let sanitized = InvoiceSearch::sanitize_query(&query).unwrap();
        assert_eq!(sanitized, String::from_str(&env, "testquery"));

        // Empty query
        let query = String::from_str(&env, "");
        assert!(InvoiceSearch::sanitize_query(&query).is_err());

        // Only spaces
        let query = String::from_str(&env, "   ");
        assert!(InvoiceSearch::sanitize_query(&query).is_err());

        // Too long
        let long_query = String::from_str(&env, &"a".repeat(101));
        assert!(InvoiceSearch::sanitize_query(&long_query).is_err());
    }

    #[test]
    #[ignore = "requires contract-scoped storage context for Invoice::new"]
    fn test_search_ranking() {
        let env = setup_test_env();
        let business = Address::generate(&env);

        // Create test invoices
        let invoice1 =
            create_test_invoice(&env, &business, "consulting services", Some("ABC Corp"));
        let invoice2 =
            create_test_invoice(&env, &business, "software development", Some("XYZ Ltd"));
        let invoice3 = create_test_invoice(&env, &business, "marketing campaign", None);

        // Mock storage (in real test, we'd store them)
        // For this test, we'll test the ranking logic directly

        // Test exact ID match (highest rank)
        let query = String::from_str(&env, "consulting");
        let rank1 = InvoiceSearch::calculate_rank(&env, &invoice1, &query);
        assert_eq!(rank1, SearchRank::PartialMatch);

        // Test partial match in customer name
        let query = String::from_str(&env, "abc");
        let rank2 = InvoiceSearch::calculate_rank(&env, &invoice1, &query);
        assert_eq!(rank2, SearchRank::PartialMatch);

        // Test no match
        let query = String::from_str(&env, "nonexistent");
        let rank3 = InvoiceSearch::calculate_rank(&env, &invoice1, &query);
        assert_eq!(rank3, SearchRank::Other);
    }

    #[test]
    fn test_contains_substring() {
        let env = setup_test_env();

        let text = String::from_str(&env, "hello world");
        let query = String::from_str(&env, "world");
        assert!(InvoiceSearch::contains_substring(&text, &query));

        let query = String::from_str(&env, "WORLD");
        assert!(InvoiceSearch::contains_substring(&text, &query)); // case insensitive

        let query = String::from_str(&env, "nonexistent");
        assert!(!InvoiceSearch::contains_substring(&text, &query));
    }

    #[test]
    fn test_bytes_to_hex_string() {
        let env = setup_test_env();
        let bytes = BytesN::<32>::from_array(
            &env,
            &[
                0x12, 0x34, 0xAB, 0xCD, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0,
            ],
        );

        let hex = InvoiceSearch::bytes_to_hex_string(&env, &bytes);
        assert_eq!(
            hex,
            String::from_str(
                &env,
                "1234abcd00000000000000000000000000000000000000000000000000000000"
            )
        );
    }
}
