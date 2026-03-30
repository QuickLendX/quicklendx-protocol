use crate::errors::QuickLendXError;
// Re-export from crate::types so other modules can continue to import from crate::investment.
pub use crate::types::{InsuranceCoverage, Investment, InvestmentStatus};
use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol, Vec};

// ─── Storage key for the global active-investment index ───────────────────────
const ACTIVE_INDEX_KEY: Symbol = symbol_short!("act_inv");

/// Premium rate applied to the covered amount expressed in basis points (1/10,000).
/// Represents 2% of the covered amount (200 / 10,000 = 0.02).
pub const DEFAULT_INSURANCE_PREMIUM_BPS: i128 = 200;

/// Minimum allowed coverage percentage (inclusive). Zero-percent coverage
/// carries no protection and is semantically meaningless.
pub const MIN_COVERAGE_PERCENTAGE: u32 = 1;

/// Maximum allowed coverage percentage (inclusive). Coverage above 100% would
/// produce a `coverage_amount` exceeding the investment principal, enabling an
/// over-coverage exploit where a claimant could receive more than was invested.
pub const MAX_COVERAGE_PERCENTAGE: u32 = 100;

/// Minimum acceptable premium in base currency units. A zero-premium policy
/// would represent free insurance — an unbounded liability for the provider
/// with no economic cost to the insured party.
pub const MIN_PREMIUM_AMOUNT: i128 = 1;

// Local type definitions removed — InsuranceCoverage, InvestmentStatus, and
// Investment are now imported from crate::types (the single source of truth).

impl InvestmentStatus {
    /// Validate that a status transition is legal.
    ///
    /// ### Allowed transitions
    /// | From      | To                              |
    /// |-----------|----------------------------------|
    /// | Active    | Completed, Defaulted, Refunded, Withdrawn |
    /// | Withdrawn | (terminal – no further moves)   |
    /// | Completed | (terminal)                      |
    /// | Defaulted | (terminal)                      |
    /// | Refunded  | (terminal)                      |
    ///
    /// ### Security
    /// Calling code **must** invoke this before persisting a status change so
    /// that no path (settlement, default, refund, or future code) can produce
    /// an orphan `Active` investment or an impossible backward transition.
    pub fn validate_transition(
        from: &InvestmentStatus,
        to: &InvestmentStatus,
    ) -> Result<(), QuickLendXError> {
        let allowed = match from {
            InvestmentStatus::Active => matches!(
                to,
                InvestmentStatus::Completed
                    | InvestmentStatus::Defaulted
                    | InvestmentStatus::Refunded
                    | InvestmentStatus::Withdrawn
            ),
            // All other states are terminal.
            _ => false,
        };
        if allowed {
            Ok(())
        } else {
            Err(QuickLendXError::InvalidStatus)
        }
    }
}

impl Investment {
    /// Compute the insurance premium for a given investment amount and coverage
    /// percentage.
    ///
    /// # Arguments
    /// * `amount`              – Positive investment principal in base currency units.
    /// * `coverage_percentage` – Integer percentage in
    ///                           [`MIN_COVERAGE_PERCENTAGE`]`..=`[`MAX_COVERAGE_PERCENTAGE`].
    ///
    /// # Returns
    /// * The premium in base currency units, always ≥ [`MIN_PREMIUM_AMOUNT`] when
    ///   `coverage_amount > 0`.
    /// * `0` for any out-of-bounds input — callers **must** treat `0` as a
    ///   rejection signal.
    ///
    /// # Math
    /// ```text
    /// coverage_amount = amount × coverage_percentage / 100
    /// premium         = coverage_amount × DEFAULT_INSURANCE_PREMIUM_BPS / 10_000
    /// ```
    /// Both multiplications use `saturating_mul`; division uses `checked_div`
    /// to prevent overflow and division-by-zero panics.
    ///
    /// # Security
    /// * Rejects `coverage_percentage > MAX_COVERAGE_PERCENTAGE` so that
    ///   `coverage_amount` can never exceed `amount` (over-coverage exploit).
    /// * Verifies the `coverage_amount ≤ amount` invariant after computation as
    ///   an explicit defense-in-depth guard against future arithmetic changes.
    /// * Applies the [`MIN_PREMIUM_AMOUNT`] floor so that zero-premium insurance
    ///   is impossible whenever coverage is non-zero.
    pub fn calculate_premium(amount: i128, coverage_percentage: u32) -> i128 {
        // Reject invalid inputs before any arithmetic.
        if amount <= 0
            || coverage_percentage < MIN_COVERAGE_PERCENTAGE
            || coverage_percentage > MAX_COVERAGE_PERCENTAGE
        {
            return 0;
        }

        let coverage_amount = amount
            .saturating_mul(coverage_percentage as i128)
            .checked_div(100)
            .unwrap_or(0);

        // Invariant: coverage can never exceed the principal.
        // Guaranteed by coverage_percentage ≤ 100, but checked explicitly to
        // defend against future arithmetic changes or unexpected saturation.
        if coverage_amount <= 0 || coverage_amount > amount {
            return 0;
        }

        let premium = coverage_amount
            .saturating_mul(DEFAULT_INSURANCE_PREMIUM_BPS)
            .checked_div(10_000)
            .unwrap_or(0);

        // Apply minimum premium floor: positive coverage must always cost
        // at least MIN_PREMIUM_AMOUNT to prevent zero-premium exploits.
        if premium < MIN_PREMIUM_AMOUNT {
            MIN_PREMIUM_AMOUNT
        } else {
            premium
        }
    }

    /// Attach an insurance coverage record to this investment.
    ///
    /// # Arguments
    /// * `provider`            – Address of the insurance provider.
    /// * `coverage_percentage` – Coverage in
    ///                           [`MIN_COVERAGE_PERCENTAGE`]`..=`[`MAX_COVERAGE_PERCENTAGE`].
    /// * `premium`             – Pre-computed premium ≥ [`MIN_PREMIUM_AMOUNT`], typically
    ///                           produced by [`Investment::calculate_premium`].
    ///
    /// # Returns
    /// * `Ok(coverage_amount)` – The absolute amount covered in base currency units.
    ///
    /// # Errors
    /// * [`InvalidCoveragePercentage`] – `coverage_percentage` out of valid range.
    /// * [`InvalidAmount`]             – Investment principal ≤ 0, premium below
    ///                                   minimum, `coverage_amount` is zero or
    ///                                   exceeds principal, or premium exceeds
    ///                                   coverage amount.
    /// * [`OperationNotAllowed`]       – An active coverage entry already exists.
    ///
    /// # Security
    /// All arithmetic bounds are re-checked inside this method so that it is
    /// safe to call directly (i.e., independently of `lib.rs`), providing
    /// defense-in-depth against caller omissions.
    pub fn add_insurance(
        &mut self,
        provider: Address,
        coverage_percentage: u32,
        premium: i128,
    ) -> Result<i128, QuickLendXError> {
        // Validate coverage percentage bounds.
        if coverage_percentage < MIN_COVERAGE_PERCENTAGE
            || coverage_percentage > MAX_COVERAGE_PERCENTAGE
        {
            return Err(QuickLendXError::InvalidCoveragePercentage);
        }

        // The investment principal must be positive before any derived amount
        // is computed. A zero or negative principal cannot be meaningfully
        // insured and would produce a nonsensical coverage_amount.
        if self.amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        // Reject zero or below-minimum premiums.  A free policy creates
        // unbounded liability for the provider and is an economic exploit.
        if premium < MIN_PREMIUM_AMOUNT {
            return Err(QuickLendXError::InvalidAmount);
        }

        // Only one active insurance policy is permitted per investment at a
        // time.  Multiple concurrent active policies would complicate claim
        // settlement and open double-coverage exploits.
        for coverage in self.insurance.iter() {
            if coverage.active {
                return Err(QuickLendXError::OperationNotAllowed);
            }
        }

        let coverage_amount = self
            .amount
            .saturating_mul(coverage_percentage as i128)
            .checked_div(100)
            .unwrap_or(0);

        // Invariant: coverage_amount must be strictly positive and must not
        // exceed the investment principal.  Guaranteed by the input bounds
        // above, but verified explicitly as a defense-in-depth safeguard.
        if coverage_amount <= 0 || coverage_amount > self.amount {
            return Err(QuickLendXError::InvalidAmount);
        }

        // Invariant: premium must not exceed the coverage it funds.  With the
        // standard 2 % BPS rate this always holds, but an explicit check
        // prevents economic inversions if the rate is ever changed.
        if premium > coverage_amount {
            return Err(QuickLendXError::InvalidAmount);
        }

        self.insurance.push_back(InsuranceCoverage {
            provider,
            coverage_amount,
            premium_amount: premium,
            coverage_percentage,
            active: true,
        });

        Ok(coverage_amount)
    }

    pub fn has_active_insurance(&self) -> bool {
        for coverage in self.insurance.iter() {
            if coverage.active {
                return true;
            }
        }
        false
    }

    pub fn process_insurance_claim(&mut self) -> Option<(Address, i128)> {
        let len = self.insurance.len();
        for idx in 0..len {
            if let Some(mut coverage) = self.insurance.get(idx) {
                if coverage.active {
                    coverage.active = false;
                    let provider = coverage.provider.clone();
                    let amount = coverage.coverage_amount;
                    self.insurance.set(idx, coverage);
                    return Some((provider, amount));
                }
            }
        }
        None
    }
}

pub struct InvestmentStorage;

impl InvestmentStorage {
    fn invoice_index_key(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (symbol_short!("inv_map"), invoice_id.clone())
    }

    /// Generate a unique investment ID using timestamp and counter
    pub fn generate_unique_investment_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let counter_key = symbol_short!("invst_cnt");
        let counter = env.storage().instance().get(&counter_key).unwrap_or(0u64);
        let next_counter = counter.saturating_add(1);
        env.storage().instance().set(&counter_key, &next_counter);

        let mut id_bytes = [0u8; 32];
        // Add investment prefix to distinguish from other entity types
        id_bytes[0] = 0x1A; // 'I' for Investment
        id_bytes[1] = 0x4E; // 'N' for iNvestment
                            // Embed timestamp in next 8 bytes
        id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        // Embed counter in next 8 bytes
        id_bytes[10..18].copy_from_slice(&next_counter.to_be_bytes());
        // Fill remaining bytes with a pattern to ensure uniqueness (overflow-safe)
        let mix = timestamp
            .saturating_add(next_counter)
            .saturating_add(0x1A4E);
        for i in 18..32 {
            id_bytes[i] = (mix % 256) as u8;
        }

        BytesN::from_array(env, &id_bytes)
    }

    pub fn store_investment(env: &Env, investment: &Investment) {
        env.storage()
            .instance()
            .set(&investment.investment_id, investment);

        env.storage().instance().set(
            &Self::invoice_index_key(&investment.invoice_id),
            &investment.investment_id,
        );

        // Add to investor index
        Self::add_to_investor_index(env, &investment.investor, &investment.investment_id);

        // Track in active index (new investments always start Active)
        if investment.status == InvestmentStatus::Active {
            Self::add_to_active_index(env, &investment.investment_id);
        }
    }

    pub fn get_investment(env: &Env, investment_id: &BytesN<32>) -> Option<Investment> {
        env.storage().instance().get(investment_id)
    }

    pub fn get_investment_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Investment> {
        let index_key = Self::invoice_index_key(invoice_id);
        let investment_id: Option<BytesN<32>> = env.storage().instance().get(&index_key);
        investment_id
            .and_then(|id| Self::get_investment(env, &id))
            .filter(|inv| inv.invoice_id == *invoice_id)
    }

    /// Update an investment, enforcing the transition guard and maintaining the
    /// active-investment index so no orphan `Active` records can accumulate.
    ///
    /// ### Panics
    /// Panics (contract error) if the transition `old_status → new_status` is
    /// not in the allowed set defined by `InvestmentStatus::validate_transition`.
    pub fn update_investment(env: &Env, investment: &Investment) {
        // Retrieve the previous status to validate the transition.
        let previous_status = env
            .storage()
            .instance()
            .get::<_, Investment>(&investment.investment_id)
            .map(|i| i.status)
            .unwrap_or(InvestmentStatus::Active); // safe default for new records

        // Only validate when the status actually changes.
        if previous_status != investment.status {
            InvestmentStatus::validate_transition(&previous_status, &investment.status)
                .expect("invalid investment status transition");

            // Remove from active index when leaving Active state.
            if previous_status == InvestmentStatus::Active {
                Self::remove_from_active_index(env, &investment.investment_id);
            }
        }

        env.storage()
            .instance()
            .set(&investment.investment_id, investment);

        env.storage().instance().set(
            &Self::invoice_index_key(&investment.invoice_id),
            &investment.investment_id,
        );
    }

    // ── Active-investment index ───────────────────────────────────────────────

    fn add_to_active_index(env: &Env, investment_id: &BytesN<32>) {
        let mut ids: Vec<BytesN<32>> = env
            .storage()
            .instance()
            .get(&ACTIVE_INDEX_KEY)
            .unwrap_or_else(|| Vec::new(env));
        // Deduplicate
        for existing in ids.iter() {
            if existing == *investment_id {
                return;
            }
        }
        ids.push_back(investment_id.clone());
        env.storage().instance().set(&ACTIVE_INDEX_KEY, &ids);
    }

    fn remove_from_active_index(env: &Env, investment_id: &BytesN<32>) {
        let ids: Vec<BytesN<32>> = env
            .storage()
            .instance()
            .get(&ACTIVE_INDEX_KEY)
            .unwrap_or_else(|| Vec::new(env));
        let mut updated = Vec::new(env);
        for id in ids.iter() {
            if id != *investment_id {
                updated.push_back(id);
            }
        }
        env.storage().instance().set(&ACTIVE_INDEX_KEY, &updated);
    }

    /// Return all investment IDs currently in `Active` status.
    ///
    /// Used by `validate_no_orphan_investments` and off-chain monitoring.
    pub fn get_active_investment_ids(env: &Env) -> Vec<BytesN<32>> {
        env.storage()
            .instance()
            .get(&ACTIVE_INDEX_KEY)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Scan the active index and verify every listed investment is still `Active`.
    ///
    /// Returns `true` when no orphans exist (all active-index entries have
    /// `status == Active`).  Returns `false` if any entry has a terminal status
    /// but was not removed from the index — indicating a bug in the transition
    /// path.
    ///
    /// ### Security note
    /// This is a read-only integrity check.  It does **not** mutate state.
    /// Call it after settlement or default to assert correctness in tests and
    /// off-chain monitoring.
    pub fn validate_no_orphan_investments(env: &Env) -> bool {
        let ids = Self::get_active_investment_ids(env);
        for id in ids.iter() {
            if let Some(inv) = Self::get_investment(env, &id) {
                if inv.status != InvestmentStatus::Active {
                    return false; // orphan detected
                }
            }
        }
        true
    }

    fn investor_index_key(investor: &Address) -> (Symbol, Address) {
        (symbol_short!("invst_inv"), investor.clone())
    }

    /// Get all investments for an investor
    pub fn get_investments_by_investor(env: &Env, investor: &Address) -> Vec<BytesN<32>> {
        let key = Self::investor_index_key(investor);
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Add investment to investor index
    pub fn add_to_investor_index(env: &Env, investor: &Address, investment_id: &BytesN<32>) {
        let key = Self::investor_index_key(investor);
        let mut investments = Self::get_investments_by_investor(env, investor);
        // Check if already exists
        let mut exists = false;
        for inv_id in investments.iter() {
            if inv_id == *investment_id {
                exists = true;
                break;
            }
        }
        if !exists {
            investments.push_back(investment_id.clone());
            env.storage().instance().set(&key, &investments);
        }
    }
}
