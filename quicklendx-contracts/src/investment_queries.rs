use crate::investment::InvestmentStorage;
use crate::types::InvestmentStatus;
use soroban_sdk::{symbol_short, Address, BytesN, Env, Vec};

/// Maximum number of records returned by paginated query endpoints.
/// This constant ensures memory usage stays within reasonable bounds.
pub const MAX_QUERY_LIMIT: u32 = 100;

/// Read-only investment query helpers with pagination support
pub struct InvestmentQueries;

impl InvestmentQueries {
    /// Returns investment IDs indexed by investor address
    pub fn by_investor(env: &Env, investor: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&(symbol_short!("inv_invr"), investor))
            .unwrap_or(Vec::new(env))
    }

    /// Returns investment IDs for a specific invoice
    pub fn by_invoice(env: &Env, invoice_id: u64) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&(symbol_short!("inv_invc"), invoice_id))
            .unwrap_or(Vec::new(env))
    }

    /// Returns investment IDs filtered by status
    pub fn by_status(env: &Env, status: InvestmentStatus) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&(symbol_short!("inv_stat"), status))
            .unwrap_or(Vec::new(env))
    }

    /// Caps query limit to prevent excessive memory usage and ensure consistent performance.
    ///
    /// # Arguments
    /// * `limit` - Requested limit (will be capped to MAX_QUERY_LIMIT)
    ///
    /// # Returns
    /// * Capped limit value, guaranteed to be <= MAX_QUERY_LIMIT
    ///
    /// # Security Notes
    /// - Uses saturating arithmetic to prevent overflow
    /// - Enforces maximum limit to prevent DoS attacks via large queries
    #[inline]
    pub fn cap_query_limit(limit: u32) -> u32 {
        limit.min(MAX_QUERY_LIMIT)
    }

    /// Validates pagination parameters for safety and correctness.
    ///
    /// # Arguments
    /// * `offset` - Starting position in the result set
    /// * `limit` - Maximum number of records to return
    /// * `total_count` - Total number of available records
    ///
    /// # Returns
    /// * Tuple of (validated_offset, validated_limit, has_more)
    ///
    /// # Security Notes
    /// - Uses saturating arithmetic to prevent overflow
    /// - Ensures offset doesn't exceed available data
    /// - Caps limit to prevent excessive memory usage
    pub fn validate_pagination_params(
        offset: u32,
        limit: u32,
        total_count: u32,
    ) -> (u32, u32, bool) {
        let capped_limit = Self::cap_query_limit(limit);
        let safe_offset = offset.min(total_count);
        let remaining = total_count.saturating_sub(safe_offset);
        let actual_limit = capped_limit.min(remaining);
        let has_more = safe_offset.saturating_add(actual_limit) < total_count;

        (safe_offset, actual_limit, has_more)
    }

    /// Safely calculates pagination bounds with overflow protection.
    ///
    /// # Arguments
    /// * `offset` - Starting position
    /// * `limit` - Number of records requested
    /// * `collection_size` - Size of the collection being paginated
    ///
    /// # Returns
    /// * Tuple of (start_index, end_index) both guaranteed to be within bounds
    ///
    /// # Security Notes
    /// - All arithmetic operations use saturating variants
    /// - Bounds are guaranteed to be within [0, collection_size]
    /// - Handles edge cases like offset >= collection_size gracefully
    pub fn calculate_safe_bounds(offset: u32, limit: u32, collection_size: u32) -> (u32, u32) {
        let capped_limit = Self::cap_query_limit(limit);
        let start = offset.min(collection_size);
        let end = start.saturating_add(capped_limit).min(collection_size);
        (start, end)
    }

    /// Retrieves paginated investments for a specific investor with overflow-safe arithmetic.
    ///
    /// # Arguments
    /// * `env` - Soroban environment
    /// * `investor` - Investor address to query
    /// * `status_filter` - Optional status filter
    /// * `offset` - Starting position (0-based)
    /// * `limit` - Maximum records to return (capped to MAX_QUERY_LIMIT)
    ///
    /// # Returns
    /// * Vector of investment IDs matching the criteria
    ///
    /// # Security Notes
    /// - Uses saturating arithmetic throughout to prevent overflow
    /// - Validates all bounds before array access
    /// - Caps limit to prevent DoS attacks
    /// - Handles empty collections gracefully
    pub fn get_investor_investments_paginated(
        env: &Env,
        investor: &Address,
        status_filter: Option<InvestmentStatus>,
        offset: u32,
        limit: u32,
    ) -> Vec<BytesN<32>> {
        let all_investment_ids = InvestmentStorage::get_investments_by_investor(env, investor);
        let mut filtered = Vec::new(env);

        // Apply status filter if provided
        for investment_id in all_investment_ids.iter() {
            if let Some(investment) = InvestmentStorage::get_investment(env, &investment_id) {
                let matches_filter = match &status_filter {
                    Some(status) => investment.status == *status,
                    None => true,
                };

                if matches_filter {
                    filtered.push_back(investment_id);
                }
            }
        }

        // Apply pagination with overflow-safe arithmetic
        let collection_size = filtered.len() as u32;
        let (start, end) = Self::calculate_safe_bounds(offset, limit, collection_size);

        let mut result = Vec::new(env);
        let mut idx = start;

        while idx < end {
            if let Some(investment_id) = filtered.get(idx) {
                result.push_back(investment_id);
            }
            idx = idx.saturating_add(1);
        }

        result
    }

    /// Counts total investments for an investor with optional status filter.
    ///
    /// # Arguments
    /// * `env` - Soroban environment
    /// * `investor` - Investor address
    /// * `status_filter` - Optional status filter
    ///
    /// # Returns
    /// * Total count of matching investments
    ///
    /// # Security Notes
    /// - Uses saturating arithmetic for count operations
    /// - Handles storage access failures gracefully
    pub fn count_investor_investments(
        env: &Env,
        investor: &Address,
        status_filter: Option<InvestmentStatus>,
    ) -> u32 {
        let all_investment_ids = InvestmentStorage::get_investments_by_investor(env, investor);
        let mut count = 0u32;

        for investment_id in all_investment_ids.iter() {
            if let Some(investment) = InvestmentStorage::get_investment(env, &investment_id) {
                let matches_filter = match &status_filter {
                    Some(status) => investment.status == *status,
                    None => true,
                };

                if matches_filter {
                    count = count.saturating_add(1);
                }
            }
        }

        count
    }
}
