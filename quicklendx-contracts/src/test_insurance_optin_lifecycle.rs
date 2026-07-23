//! Regression: investment insurance opt-in lifecycle (#1547).
//!
//! Locks in the full lifecycle of an insurance policy on an investment and the
//! events emitted at each transition:
//!
//!   opt-in  ->  active  ->  claim  ->  closed
//!
//! - **opt-in / active**: the investor opts in via `add_investment_insurance`;
//!   the policy becomes active and `insurance_added` + `premium_collected`
//!   events are emitted.
//! - **claim**: the active policy is claimed exactly once, returning the
//!   provider and the coverage amount.
//! - **closed**: after the claim the policy is deactivated and the investment
//!   reports no active coverage; a second claim is a no-op.
//!
//! These cases are deterministic (no `Date.now()`/randomness) and run on the
//! default test build so they execute on every CI matrix entry.
#[cfg(test)]
mod test_insurance_optin_lifecycle {
    use crate::investment::{
        Investment, InvestmentStatus, InvestmentStorage, MAX_COVERAGE_PERCENTAGE,
        MIN_COVERAGE_PERCENTAGE,
    };
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Events as _},
        Address, BytesN, Env, Vec,
    };

    fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
        let env = Env::default();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        (env, client, contract_id)
    }

    /// Store a bare Active investment directly through storage so the test does
    /// not depend on the (separate) funding path.
    fn store_active_investment(
        env: &Env,
        contract_id: &Address,
        investor: &Address,
        amount: i128,
        seed: u8,
    ) -> BytesN<32> {
        env.as_contract(contract_id, || {
            let investment_id = InvestmentStorage::generate_unique_investment_id(env);
            let mut invoice_bytes = [seed; 32];
            invoice_bytes[0] = 0xCD;
            let investment = Investment {
                investment_id: investment_id.clone(),
                invoice_id: BytesN::from_array(env, &invoice_bytes),
                investor: investor.clone(),
                amount,
                funded_at: env.ledger().timestamp(),
                status: InvestmentStatus::Active,
                insurance: Vec::new(env),
            };
            InvestmentStorage::store_investment(env, &investment);
            investment_id
        })
    }

    /// Full happy-path lifecycle: opt-in -> active -> claim -> closed, asserting
    /// the events emitted on opt-in.
    #[test]
    fn insurance_optin_lifecycle_active_then_claim_then_closed() {
        let (env, client, contract_id) = setup();
        env.mock_all_auths();

        let investor = Address::generate(&env);
        let provider = Address::generate(&env);
        let amount = 10_000i128;
        let coverage_pct = 80u32;

        // --- opt-in / active -------------------------------------------------
        let investment_id = store_active_investment(&env, &contract_id, &investor, amount, 1);
        client.add_investment_insurance(&investment_id, &provider, &coverage_pct);

        // Opt-in must emit at least the insurance_added + premium_collected
        // events for this transition.
        let events = env.events().all();
        assert!(
            events.events().len() >= 2,
            "opt-in must emit insurance_added and premium_collected events"
        );

        // Policy is now active and queryable.
        let records = client.query_investment_insurance(&investment_id);
        assert_eq!(records.len(), 1, "exactly one policy after opt-in");
        let coverage = records.get(0).unwrap();
        assert!(coverage.active, "policy must be active after opt-in");
        assert_eq!(
            coverage.coverage_amount, 8_000,
            "80% of 10_000 principal must be covered"
        );

        // --- claim -----------------------------------------------------------
        let (claim_provider, claim_amount) = env.as_contract(&contract_id, || {
            let mut investment = InvestmentStorage::get_investment(&env, &investment_id)
                .expect("investment must exist");
            assert!(investment.has_active_insurance());
            let claim = investment
                .process_insurance_claim()
                .expect("active policy must yield exactly one claim");
            InvestmentStorage::update_investment(&env, &investment);
            claim
        });
        assert_eq!(claim_provider, provider, "claim pays the policy provider");
        assert_eq!(claim_amount, 8_000, "claim pays the coverage amount");

        // --- closed ----------------------------------------------------------
        let records_after = client.query_investment_insurance(&investment_id);
        assert_eq!(records_after.len(), 1, "the policy record is retained");
        assert!(
            !records_after.get(0).unwrap().active,
            "policy must be inactive (closed) after the claim"
        );

        // A second claim attempt is a no-op: the policy is closed.
        let second_claim = env.as_contract(&contract_id, || {
            let mut investment = InvestmentStorage::get_investment(&env, &investment_id)
                .expect("investment must exist");
            assert!(!investment.has_active_insurance());
            investment.process_insurance_claim()
        });
        assert!(
            second_claim.is_none(),
            "a closed policy must not produce a second claim"
        );
    }

    /// Sad path: opting in is only allowed on Active investments. A terminal
    /// (Withdrawn) investment must reject the opt-in so closed positions cannot
    /// acquire new coverage.
    #[test]
    fn insurance_optin_rejected_on_terminal_investment() {
        let (env, client, contract_id) = setup();
        env.mock_all_auths();

        let investor = Address::generate(&env);
        let provider = Address::generate(&env);

        // Store an investment and force it into a terminal Withdrawn status.
        let investment_id = store_active_investment(&env, &contract_id, &investor, 10_000, 2);
        env.as_contract(&contract_id, || {
            let mut investment = InvestmentStorage::get_investment(&env, &investment_id).unwrap();
            investment.status = InvestmentStatus::Withdrawn;
            InvestmentStorage::update_investment(&env, &investment);
        });

        let result = client.try_add_investment_insurance(&investment_id, &provider, &50u32);
        assert!(
            result.is_err(),
            "opt-in on a terminal investment must be rejected"
        );
    }

    /// Boundary: opt-in accepts the documented coverage-percentage bounds, and
    /// each produces an active policy.
    #[test]
    fn insurance_optin_accepts_coverage_bounds() {
        let (env, client, contract_id) = setup();
        env.mock_all_auths();

        let investor = Address::generate(&env);

        for (seed, pct) in [(3u8, MIN_COVERAGE_PERCENTAGE), (4u8, MAX_COVERAGE_PERCENTAGE)] {
            let provider = Address::generate(&env);
            let investment_id =
                store_active_investment(&env, &contract_id, &investor, 100_000, seed);
            client.add_investment_insurance(&investment_id, &provider, &pct);

            let records = client.query_investment_insurance(&investment_id);
            assert_eq!(records.len(), 1);
            assert!(
                records.get(0).unwrap().active,
                "boundary coverage {pct} must produce an active policy"
            );
        }
    }
}
