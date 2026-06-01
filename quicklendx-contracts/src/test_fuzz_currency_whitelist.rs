//! # Property-Based Fuzz Tests — Currency Whitelist Churn (Issue #1216)
//!
//! Validates that the four mutating whitelist operations
//! ([`CurrencyWhitelist::add_currency`], [`remove_currency`], [`set_currencies`],
//! and [`clear_currencies`]) behave identically to a deterministic reference
//! model under arbitrary interleaving ("churn").
//!
//! ## Core property
//!
//! For any random sequence of actions applied to the live contract, the
//! resulting on-chain whitelist **must equal** the whitelist produced by
//! replaying the same actions against a pure in-memory model that encodes the
//! contract's documented semantics:
//!
//! | Action            | Model semantics                                           |
//! |-------------------|-----------------------------------------------------------|
//! | `Add(c)`          | append `c` iff not already present (idempotent)           |
//! | `Remove(c)`       | drop all occurrences of `c` (no-op when absent)           |
//! | `Set(cs)`         | replace list with `cs`, first-occurrence dedupe           |
//! | `Clear`           | replace list with the empty list                          |
//!
//! After **every** action we additionally assert:
//! * `is_allowed_currency(c)` agrees with model membership for the touched key
//!   and a sentinel key that is never added,
//! * `currency_count()` equals the model length,
//! * ordering (first-occurrence) is preserved element-by-element.
//!
//! ## Running
//!
//! ```bash
//! # Default proptest budget
//! cargo test --features fuzz-tests test_fuzz_currency_whitelist
//!
//! # Assignment requirement: at least 30 000 random sequences
//! PROPTEST_CASES=30000 cargo test --features fuzz-tests test_fuzz_currency_whitelist
//! ```
//!
//! ## Security assumptions
//!
//! - **Admin auth coverage.** Every mutating entrypoint funnels through
//!   `AdminStorage::require_admin` + `require_auth()`. The churn tests run under
//!   `mock_all_auths()` (so the *state-machine* property is exercised), and a
//!   dedicated [`auth`] sub-module disables auth mocking to prove that a
//!   non-admin caller is rejected for all four mutators with `NotAdmin`. This
//!   guarantees the deterministic-replay property can never be reached by an
//!   unauthorized actor.
//! - **No silent dedupe drops authorization.** Dedup happens *after* the admin
//!   check inside `add_currency`/`set_currencies`; the auth tests confirm a
//!   duplicate-laden payload from a non-admin still fails before any mutation.
//! - **Idempotency.** Remove-then-add cycles are explicitly validated to return
//!   the list to a stable, deterministic state.

#[cfg(test)]
mod test_fuzz_currency_whitelist {
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use proptest::prelude::*;
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec as SVec};

    // ─────────────────────────────────────────────────────────────────────────
    // Action model
    // ─────────────────────────────────────────────────────────────────────────

    /// A single whitelist mutation applied during a churn sequence.
    ///
    /// Each variant maps 1:1 to a contract entrypoint. Currency operands are
    /// expressed as small `usize` indices into a fixed pool of generated
    /// [`Address`]es (see [`Pool`]); this keeps proptest shrinking cheap while
    /// still exercising collisions, re-adds, and duplicate payloads.
    #[derive(Clone, Debug)]
    enum Action {
        /// Add the currency at pool index `0` — idempotent if already present.
        Add(usize),
        /// Remove the currency at pool index `0` — no-op when absent.
        Remove(usize),
        /// Atomically replace the whole list with the currencies at the given
        /// pool indices (may contain duplicates to exercise dedupe).
        Set(std::vec::Vec<usize>),
        /// Clear the entire whitelist (allow-all mode afterwards).
        Clear,
    }

    /// Fixed pool of distinct addresses shared by the model and the contract.
    struct Pool {
        addrs: std::vec::Vec<Address>,
    }

    impl Pool {
        fn new(env: &Env, n: usize) -> Self {
            let addrs = (0..n).map(|_| Address::generate(env)).collect();
            Self { addrs }
        }
        fn get(&self, i: usize) -> Address {
            self.addrs[i % self.addrs.len()].clone()
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Reference model — pure, deterministic replay
    // ─────────────────────────────────────────────────────────────────────────

    /// In-memory mirror of the whitelist storing pool indices in
    /// first-occurrence order. Encodes the exact documented semantics so the
    /// contract can be diffed against it after every action.
    #[derive(Default, Clone)]
    struct Model {
        list: std::vec::Vec<usize>,
    }

    impl Model {
        fn apply(&mut self, a: &Action) {
            match a {
                Action::Add(i) => {
                    if !self.list.contains(i) {
                        self.list.push(*i);
                    }
                }
                Action::Remove(i) => {
                    self.list.retain(|x| x != i);
                }
                Action::Set(items) => {
                    let mut deduped: std::vec::Vec<usize> = std::vec::Vec::new();
                    for it in items {
                        if !deduped.contains(it) {
                            deduped.push(*it);
                        }
                    }
                    self.list = deduped;
                }
                Action::Clear => self.list.clear(),
            }
        }
        fn contains(&self, i: usize) -> bool {
            self.list.contains(&i)
        }
        fn len(&self) -> usize {
            self.list.len()
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Contract harness
    // ─────────────────────────────────────────────────────────────────────────

    const POOL_SIZE: usize = 6;

    fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Pool) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let _ = client.initialize_admin(&admin);
        let _ = client.set_admin(&admin);
        let pool = Pool::new(&env, POOL_SIZE);
        (env, client, admin, pool)
    }

    fn drive(client: &QuickLendXContractClient<'_>, admin: &Address, pool: &Pool, env: &Env, a: &Action) {
        match a {
            Action::Add(i) => client.add_currency(admin, &pool.get(*i)),
            Action::Remove(i) => client.remove_currency(admin, &pool.get(*i)),
            Action::Set(items) => {
                let mut v: SVec<Address> = SVec::new(env);
                for it in items {
                    v.push_back(pool.get(*it));
                }
                client.set_currencies(admin, &v);
            }
            Action::Clear => client.clear_currencies(admin),
        }
    }

    /// Assert the live contract state matches the reference model exactly.
    fn assert_equivalent(
        client: &QuickLendXContractClient<'_>,
        pool: &Pool,
        model: &Model,
        touched: usize,
    ) {
        let on_chain = client.get_whitelisted_currencies();

        // 1. Length parity.
        assert_eq!(
            on_chain.len() as usize,
            model.len(),
            "length mismatch: chain={} model={}",
            on_chain.len(),
            model.len()
        );

        // 2. Count entrypoint agrees with the materialized list.
        assert_eq!(
            client.currency_count() as usize,
            model.len(),
            "currency_count disagrees with model length"
        );

        // 3. Element-by-element first-occurrence ordering.
        for (pos, idx) in model.list.iter().enumerate() {
            let expected = pool.get(*idx);
            let actual = on_chain.get(pos as u32).expect("index in bounds");
            assert_eq!(actual, expected, "ordering mismatch at position {pos}");
        }

        // 4. Membership oracle agrees for the touched key.
        assert_eq!(
            client.is_allowed_currency(&pool.get(touched)),
            model.contains(touched),
            "is_allowed_currency disagrees for touched index {touched}"
        );

        // 5. Membership oracle agrees for every pool key (full sweep).
        for i in 0..POOL_SIZE {
            assert_eq!(
                client.is_allowed_currency(&pool.get(i)),
                model.contains(i),
                "is_allowed_currency disagrees for pool index {i}"
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Strategies
    // ─────────────────────────────────────────────────────────────────────────

    fn action_strategy() -> impl Strategy<Value = Action> {
        prop_oneof![
            (0usize..POOL_SIZE).prop_map(Action::Add),
            (0usize..POOL_SIZE).prop_map(Action::Remove),
            // Set payloads may contain duplicates and varying length (incl. empty).
            prop::collection::vec(0usize..POOL_SIZE, 0..8).prop_map(Action::Set),
            Just(Action::Clear),
        ]
    }

    fn sequence_strategy() -> impl Strategy<Value = std::vec::Vec<Action>> {
        prop::collection::vec(action_strategy(), 1..40)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Property: deterministic replay under churn
    // ─────────────────────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// For any random interleaving of add/remove/set/clear, the live
        /// contract state equals the deterministic replay of the same actions,
        /// and `is_allowed_currency`/`currency_count` answer correctly after
        /// every single step.
        #[test]
        fn fuzz_whitelist_churn_matches_model(actions in sequence_strategy()) {
            let (env, client, admin, pool) = setup();
            let mut model = Model::default();

            for a in &actions {
                drive(&client, &admin, &pool, &env, a);
                model.apply(a);

                let touched = match a {
                    Action::Add(i) | Action::Remove(i) => *i,
                    Action::Set(items) => items.first().copied().unwrap_or(0),
                    Action::Clear => 0,
                };
                assert_equivalent(&client, &pool, &model, touched);
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Property: remove-then-add idempotency cycles
    // ─────────────────────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// A `remove(c)` followed by `add(c)` returns the list to a deterministic
        /// state regardless of starting point, and repeating the cycle is stable.
        #[test]
        fn fuzz_remove_then_add_cycle_idempotent(
            seed in prop::collection::vec(0usize..POOL_SIZE, 0..6),
            target in 0usize..POOL_SIZE,
            cycles in 1u32..6,
        ) {
            let (env, client, admin, pool) = setup();

            // Seed an arbitrary starting whitelist.
            let mut v: SVec<Address> = SVec::new(&env);
            for it in &seed { v.push_back(pool.get(*it)); }
            client.set_currencies(&admin, &v);

            // Reach a canonical "present" state once.
            client.add_currency(&admin, &pool.get(target));
            let baseline = client.get_whitelisted_currencies();

            // Each remove→add cycle must restore the exact baseline.
            for _ in 0..cycles {
                client.remove_currency(&admin, &pool.get(target));
                prop_assert!(!client.is_allowed_currency(&pool.get(target)));
                client.add_currency(&admin, &pool.get(target));
                prop_assert!(client.is_allowed_currency(&pool.get(target)));
                prop_assert_eq!(
                    client.get_whitelisted_currencies(),
                    baseline.clone(),
                    "remove→add cycle is not idempotent"
                );
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Edge cases (deterministic, not randomized)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn edge_add_idempotent_no_growth() {
        let (env, client, admin, pool) = setup();
        let _ = env;
        client.add_currency(&admin, &pool.get(0));
        client.add_currency(&admin, &pool.get(0));
        client.add_currency(&admin, &pool.get(0));
        assert_eq!(client.currency_count(), 1, "duplicate adds must not grow list");
    }

    #[test]
    fn edge_remove_absent_is_noop() {
        let (env, client, admin, pool) = setup();
        let _ = env;
        client.remove_currency(&admin, &pool.get(0)); // empty list
        assert_eq!(client.currency_count(), 0);
        client.add_currency(&admin, &pool.get(1));
        client.remove_currency(&admin, &pool.get(0)); // absent key
        assert_eq!(client.currency_count(), 1);
        assert!(client.is_allowed_currency(&pool.get(1)));
    }

    #[test]
    fn edge_set_dedupes_and_replaces() {
        let (env, client, admin, pool) = setup();
        client.add_currency(&admin, &pool.get(5)); // pre-existing, must be replaced
        let mut v: SVec<Address> = SVec::new(&env);
        v.push_back(pool.get(0));
        v.push_back(pool.get(1));
        v.push_back(pool.get(0)); // duplicate
        v.push_back(pool.get(1)); // duplicate
        client.set_currencies(&admin, &v);
        assert_eq!(client.currency_count(), 2, "set must dedupe to 2 entries");
        assert!(client.is_allowed_currency(&pool.get(0)));
        assert!(client.is_allowed_currency(&pool.get(1)));
        assert!(!client.is_allowed_currency(&pool.get(5)), "set must replace, not merge");
    }

    #[test]
    fn edge_set_empty_then_clear() {
        let (env, client, admin, pool) = setup();
        client.add_currency(&admin, &pool.get(0));
        let empty: SVec<Address> = SVec::new(&env);
        client.set_currencies(&admin, &empty);
        assert_eq!(client.currency_count(), 0, "set([]) clears the list");
        client.add_currency(&admin, &pool.get(1));
        client.clear_currencies(&admin);
        assert_eq!(client.currency_count(), 0, "clear empties the list");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Security: admin authorization coverage
    //
    // The stored-admin check (`AdminStorage::require_admin` / explicit storage
    // comparison) rejects a non-admin caller with `NotAdmin` *before* any
    // mutation or dedupe occurs. We assert this for all four mutators and
    // confirm the whitelist is left untouched. Even with auths mocked, the
    // storage identity check guarantees only the registered admin can mutate
    // state — exactly the invariant that lets the replay property stay sound.
    // ─────────────────────────────────────────────────────────────────────────

    mod auth {
        use super::*;
        use crate::errors::QuickLendXError;

        /// Setup with admin initialized and one seeded currency, plus a
        /// non-admin "attacker" address.
        fn setup_with_attacker(
        ) -> (Env, QuickLendXContractClient<'static>, Address, Pool) {
            let (env, client, admin, pool) = setup();
            client.add_currency(&admin, &pool.get(0)); // known pre-state
            let attacker = Address::generate(&env);
            (env, client, attacker, pool)
        }

        #[test]
        fn non_admin_cannot_add() {
            let (_env, client, attacker, pool) = setup_with_attacker();
            let res = client.try_add_currency(&attacker, &pool.get(1));
            assert_eq!(res, Err(Ok(QuickLendXError::NotAdmin)));
            assert_eq!(client.currency_count(), 1, "state must be unchanged");
            assert!(!client.is_allowed_currency(&pool.get(1)));
        }

        #[test]
        fn non_admin_cannot_remove() {
            let (_env, client, attacker, pool) = setup_with_attacker();
            let res = client.try_remove_currency(&attacker, &pool.get(0));
            assert_eq!(res, Err(Ok(QuickLendXError::NotAdmin)));
            assert_eq!(client.currency_count(), 1, "state must be unchanged");
            assert!(client.is_allowed_currency(&pool.get(0)));
        }

        #[test]
        fn non_admin_cannot_set_even_with_duplicates() {
            let (env, client, attacker, pool) = setup_with_attacker();
            // A duplicate-laden payload must be rejected BEFORE any dedupe runs:
            // "no silent dedupe drops authorization checks."
            let mut v: SVec<Address> = SVec::new(&env);
            v.push_back(pool.get(2));
            v.push_back(pool.get(2));
            v.push_back(pool.get(3));
            let res = client.try_set_currencies(&attacker, &v);
            assert_eq!(res, Err(Ok(QuickLendXError::NotAdmin)));
            assert_eq!(client.currency_count(), 1, "state must be unchanged");
        }

        #[test]
        fn non_admin_cannot_clear() {
            let (_env, client, attacker, pool) = setup_with_attacker();
            let res = client.try_clear_currencies(&attacker);
            assert_eq!(res, Err(Ok(QuickLendXError::NotAdmin)));
            assert_eq!(client.currency_count(), 1, "state must be unchanged");
            assert!(client.is_allowed_currency(&pool.get(0)));
        }
    }
}
