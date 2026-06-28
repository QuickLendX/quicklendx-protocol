use crate::errors::QuickLendXError;
use crate::investment::InvestmentStorage;
use crate::types::InvestmentStatus;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Vec};

/// Aggregate portfolio snapshot for a single investor, computed in one bounded
/// pass over `InvestmentStorage::get_investments_by_investor`.
///
/// ## Bucket semantics
/// | Field              | Status(es) counted     | Notes                              |
/// |--------------------|------------------------|------------------------------------|
/// | `active_principal` | `Active`               | Sum of `investment.amount`.        |
/// | `completed_count`  | `Completed`            | Number of completed positions.     |
/// | `completed_returns`| `Completed`            | Sum of `investment.amount`.        |
/// | `defaulted_count`  | `Defaulted`            | Number of defaulted positions.     |
/// | `refunded_count`   | `Refunded`             | Number of refunded positions.      |
/// | `total_positions`  | all statuses           | Total number of investment records.|
///
/// `Withdrawn` investments are included in `total_positions` but have no
/// dedicated counter because they represent positions the investor already
/// exited voluntarily and are fully accounted for off-chain.
///
/// ## Auth
/// No auth is required: every individual investment is already publicly
/// queryable, so an aggregate of public data does not expose new information.
///
/// ## Iteration bound
/// The pass iterates at most `MAX_QUERY_LIMIT` investment IDs. Any IDs beyond
/// that cap are silently skipped so that on-chain instruction costs stay within
/// the Soroban budget. Callers that need full coverage must paginate using
/// `get_investor_investments_paged`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestorPortfolioSummary {
    /// Sum of `amount` for all Active investments (unchecked i128 addition is
    /// safe because `overflow-checks = true` in the Cargo profile).
    pub active_principal: i128,
    /// Number of investments that reached `Completed` status.
    pub completed_count: u32,
    /// Sum of `amount` for all Completed investments.
    pub completed_returns: i128,
    /// Number of investments that reached `Defaulted` status.
    pub defaulted_count: u32,
    /// Number of investments that reached `Refunded` status.
    pub refunded_count: u32,
    /// Total number of investment records iterated (≤ `MAX_QUERY_LIMIT`).
    pub total_positions: u32,
}

/// Maximum number of records returned by paginated query endpoints.
/// This constant ensures memory usage stays within reasonable bounds.
pub const MAX_QUERY_LIMIT: u32 = crate::MAX_QUERY_LIMIT;

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
        crate::pagination::cap_query_limit(limit)
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
        crate::pagination::validate_pagination_params(offset, limit, total_count)
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
        crate::pagination::calculate_safe_bounds(offset, limit, collection_size)
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
        let collection_size = filtered.len();
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

    /// Aggregate an investor's portfolio in a single bounded pass.
    ///
    /// Iterates up to `MAX_QUERY_LIMIT` investment IDs from the investor index
    /// and buckets each resolved record by `InvestmentStatus`. Records whose
    /// storage entry is missing (orphaned IDs) are silently skipped.
    ///
    /// Arithmetic uses `checked_add` on every accumulation; an overflow returns
    /// `Err(QuickLendXError::ArithmeticOverflow)` rather than wrapping or
    /// panicking (though `overflow-checks = true` would also catch it).
    ///
    /// # Arguments
    /// * `env`      - Soroban environment.
    /// * `investor` - Address whose portfolio is summarised.
    ///
    /// # Returns
    /// * `Ok(InvestorPortfolioSummary)` — consistent snapshot.
    /// * `Err(ArithmeticOverflow)`      — impossible in practice; present as a
    ///   safety net.
    pub fn investor_portfolio_summary(
        env: &Env,
        investor: &Address,
    ) -> Result<InvestorPortfolioSummary, QuickLendXError> {
        let ids = InvestmentStorage::get_investments_by_investor(env, investor);

        let mut active_principal: i128 = 0;
        let mut completed_count: u32 = 0;
        let mut completed_returns: i128 = 0;
        let mut defaulted_count: u32 = 0;
        let mut refunded_count: u32 = 0;
        let mut total_positions: u32 = 0;

        let cap = Self::cap_query_limit(ids.len());
        let mut idx = 0u32;
        while idx < cap {
            if let Some(id) = ids.get(idx) {
                if let Some(inv) = InvestmentStorage::get_investment(env, &id) {
                    total_positions = total_positions
                        .checked_add(1)
                        .ok_or(QuickLendXError::ArithmeticOverflow)?;
                    match inv.status {
                        InvestmentStatus::Active => {
                            active_principal = active_principal
                                .checked_add(inv.amount)
                                .ok_or(QuickLendXError::ArithmeticOverflow)?;
                        }
                        InvestmentStatus::Completed => {
                            completed_count = completed_count
                                .checked_add(1)
                                .ok_or(QuickLendXError::ArithmeticOverflow)?;
                            completed_returns = completed_returns
                                .checked_add(inv.amount)
                                .ok_or(QuickLendXError::ArithmeticOverflow)?;
                        }
                        InvestmentStatus::Defaulted => {
                            defaulted_count = defaulted_count
                                .checked_add(1)
                                .ok_or(QuickLendXError::ArithmeticOverflow)?;
                        }
                        InvestmentStatus::Refunded => {
                            refunded_count = refunded_count
                                .checked_add(1)
                                .ok_or(QuickLendXError::ArithmeticOverflow)?;
                        }
                        InvestmentStatus::Withdrawn => {} // counted in total_positions only
                    }
                }
            }
            idx = idx.saturating_add(1);
        }

        Ok(InvestorPortfolioSummary {
            active_principal,
            completed_count,
            completed_returns,
            defaulted_count,
            refunded_count,
            total_positions,
        })
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
