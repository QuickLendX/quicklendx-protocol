#[cfg(test)]
mod test_dispute_timeline_props {
    use alloc::vec::Vec as RustVec;
    use crate::dispute_timeline::DisputeTimeline;
    use crate::errors::QuickLendXError;
    use crate::invoice::{DisputeStatus, Invoice, InvoiceCategory};
    use crate::storage::InvoiceStorage;
    use crate::QuickLendXContract;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, BytesN, Env, String, Vec,
    };

    const RANDOMIZED_SEQUENCE_CASES: usize = 20_000;
    const MAX_ACTIONS_PER_SEQUENCE: usize = 8;
    const REDACTED_ADDRESS: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
    const DOC_STATE_MACHINE_TABLE: &str = r#"| Action | Allowed current `dispute_status` | Next `dispute_status` | Timeline effect | Terminal |
|---|---|---|---|---|
| `create` | `None` | `Disputed` | Append `Opened` | No |
| `evidence` | `Disputed` | `Disputed` | No new timeline entry | No |
| `under_review` | `Disputed` | `UnderReview` | Append `UnderReview` | No |
| `resolve` | `UnderReview` | `Resolved` | Append `Resolved` | Yes |"#;
    const DOC_GRAMMAR_LINE: &str =
        "`create -> evidence* -> (under_review -> resolve?)?`";
    const DOC_AUDIT_NOTE: &str =
        "The dispute timeline is a user-facing summary and does not replace the append-only invoice audit trail.";

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Action {
        Create,
        Evidence,
        UnderReview,
        Resolve,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ModelState {
        None,
        Disputed,
        UnderReview,
        Resolved,
    }

    #[derive(Clone, Debug)]
    struct ModelOutcome {
        final_state: ModelState,
        accepted_steps: usize,
        first_error: Option<QuickLendXError>,
        created_at: Option<u64>,
        review_at: Option<u64>,
        resolved_at: Option<u64>,
    }

    struct XorShift64 {
        state: u64,
    }

    impl XorShift64 {
        fn new(seed: u64) -> Self {
            Self {
                state: if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed },
            }
        }

        fn next_u64(&mut self) -> u64 {
            let mut x = self.state;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.state = x;
            x
        }
    }

    fn setup() -> (Env, Address, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);
        let contract_id = env.register(QuickLendXContract, ());
        let admin = Address::generate(&env);
        let business = Address::generate(&env);
        let currency = Address::generate(&env);

        env.as_contract(&contract_id, || {
            QuickLendXContract::set_admin(env.clone(), admin.clone())
                .expect("admin should initialize");
        });

        (env, contract_id, admin, business, currency)
    }

    fn create_invoice(
        env: &Env,
        contract_id: &Address,
        business: &Address,
        currency: &Address,
    ) -> BytesN<32> {
        env.as_contract(contract_id, || {
            let invoice = Invoice::new(
                env,
                business.clone(),
                100_000i128,
                currency.clone(),
                env.ledger().timestamp() + 30 * 24 * 60 * 60,
                String::from_str(env, "Dispute timeline property invoice"),
                InvoiceCategory::Services,
                Vec::new(env),
            )
            .expect("invoice should build");

            let invoice_id = invoice.id.clone();
            InvoiceStorage::store(env, &invoice);
            invoice_id
        })
    }

    fn redacted_address(env: &Env) -> Address {
        Address::from_str(env, REDACTED_ADDRESS)
    }

    fn action_from_roll(roll: u64) -> Action {
        match roll % 4 {
            0 => Action::Create,
            1 => Action::Evidence,
            2 => Action::UnderReview,
            _ => Action::Resolve,
        }
    }

    fn simulate(actions: &[Action], timestamps: &[u64]) -> ModelOutcome {
        let mut state = ModelState::None;
        let mut accepted_steps = 0usize;
        let mut created_at = None;
        let mut review_at = None;
        let mut resolved_at = None;
        let mut first_error = None;

        for (index, action) in actions.iter().enumerate() {
            let timestamp = timestamps[index];
            let outcome = match (state, action) {
                (ModelState::None, Action::Create) => {
                    state = ModelState::Disputed;
                    created_at = Some(timestamp);
                    Ok(())
                }
                (ModelState::None, Action::Evidence) => Err(QuickLendXError::InvalidStatus),
                (ModelState::None, Action::UnderReview) => Err(QuickLendXError::DisputeNotFound),
                (ModelState::None, Action::Resolve) => {
                    Err(QuickLendXError::DisputeNotUnderReview)
                }
                (ModelState::Disputed, Action::Create) => {
                    Err(QuickLendXError::DisputeAlreadyExists)
                }
                (ModelState::Disputed, Action::Evidence) => Ok(()),
                (ModelState::Disputed, Action::UnderReview) => {
                    state = ModelState::UnderReview;
                    review_at = Some(timestamp);
                    Ok(())
                }
                (ModelState::Disputed, Action::Resolve) => {
                    Err(QuickLendXError::DisputeNotUnderReview)
                }
                (ModelState::UnderReview, Action::Create) => {
                    Err(QuickLendXError::DisputeAlreadyExists)
                }
                (ModelState::UnderReview, Action::Evidence) => {
                    Err(QuickLendXError::InvalidStatus)
                }
                (ModelState::UnderReview, Action::UnderReview) => {
                    Err(QuickLendXError::InvalidStatus)
                }
                (ModelState::UnderReview, Action::Resolve) => {
                    state = ModelState::Resolved;
                    resolved_at = Some(timestamp);
                    Ok(())
                }
                (ModelState::Resolved, Action::Create) => {
                    Err(QuickLendXError::DisputeAlreadyExists)
                }
                (ModelState::Resolved, Action::Evidence) => Err(QuickLendXError::InvalidStatus),
                (ModelState::Resolved, Action::UnderReview) => {
                    Err(QuickLendXError::InvalidStatus)
                }
                (ModelState::Resolved, Action::Resolve) => {
                    Err(QuickLendXError::DisputeNotUnderReview)
                }
            };

            match outcome {
                Ok(()) => accepted_steps += 1,
                Err(err) => {
                    first_error = Some(err);
                    break;
                }
            }
        }

        ModelOutcome {
            final_state: state,
            accepted_steps,
            first_error,
            created_at,
            review_at,
            resolved_at,
        }
    }

    fn apply_action(
        env: &Env,
        contract_id: &Address,
        admin: &Address,
        business: &Address,
        invoice_id: &BytesN<32>,
        action: Action,
    ) -> Result<(), QuickLendXError> {
        let reason = String::from_str(env, "Timeline dispute reason");
        let evidence = String::from_str(env, "Timeline dispute evidence");
        let updated_evidence = String::from_str(env, "Timeline dispute evidence updated");
        let resolution = String::from_str(env, "Timeline dispute resolved");

        match action {
            Action::Create => env.as_contract(contract_id, || {
                QuickLendXContract::create_dispute(
                    env.clone(),
                    invoice_id.clone(),
                    business.clone(),
                    reason.clone(),
                    evidence.clone(),
                )
            }),
            Action::Evidence => env.as_contract(contract_id, || {
                QuickLendXContract::update_dispute_evidence(
                    env.clone(),
                    invoice_id.clone(),
                    business.clone(),
                    updated_evidence.clone(),
                )
            }),
            Action::UnderReview => env.as_contract(contract_id, || {
                QuickLendXContract::put_dispute_under_review(
                    env.clone(),
                    invoice_id.clone(),
                    admin.clone(),
                )
            }),
            Action::Resolve => env.as_contract(contract_id, || {
                QuickLendXContract::resolve_dispute(
                    env.clone(),
                    invoice_id.clone(),
                    admin.clone(),
                    resolution.clone(),
                )
            }),
        }
    }

    fn assert_timeline_matches_model(
        env: &Env,
        timeline: &DisputeTimeline,
        model: &ModelOutcome,
        business: &Address,
    ) {
        let expected_status = match model.final_state {
            ModelState::None => DisputeStatus::None,
            ModelState::Disputed => DisputeStatus::Disputed,
            ModelState::UnderReview => DisputeStatus::UnderReview,
            ModelState::Resolved => DisputeStatus::Resolved,
        };
        assert_eq!(timeline.current_status, expected_status);
        assert_eq!(timeline.total as usize, timeline.entries.len() as usize);

        for i in 0..timeline.entries.len() {
            let current = timeline.entries.get(i).unwrap();
            assert_eq!(current.sequence, i);

            if i > 0 {
                let previous = timeline.entries.get(i - 1).unwrap();
                assert!(
                    previous.timestamp < current.timestamp,
                    "timeline timestamps must be strictly increasing"
                );
                assert_ne!(
                    previous.event, current.event,
                    "timeline must not contain duplicate lifecycle entries"
                );
            }
        }

        match model.final_state {
            ModelState::None => panic!("timeline should not exist without a dispute"),
            ModelState::Disputed => {
                assert_eq!(timeline.entries.len(), 1);
                let opened = timeline.entries.get(0).unwrap();
                assert_eq!(opened.event, String::from_str(env, "Opened"));
                assert_eq!(opened.timestamp, model.created_at.unwrap());
                assert_eq!(opened.actor, *business);
            }
            ModelState::UnderReview => {
                assert_eq!(timeline.entries.len(), 2);
                let opened = timeline.entries.get(0).unwrap();
                let review = timeline.entries.get(1).unwrap();
                assert_eq!(opened.event, String::from_str(env, "Opened"));
                assert_eq!(review.event, String::from_str(env, "UnderReview"));
                assert_eq!(opened.timestamp, model.created_at.unwrap());
                assert_eq!(review.timestamp, model.review_at.unwrap());
                assert_eq!(review.actor, redacted_address(env));
            }
            ModelState::Resolved => {
                assert_eq!(timeline.entries.len(), 3);
                let opened = timeline.entries.get(0).unwrap();
                let review = timeline.entries.get(1).unwrap();
                let resolved = timeline.entries.get(2).unwrap();
                assert_eq!(opened.event, String::from_str(env, "Opened"));
                assert_eq!(review.event, String::from_str(env, "UnderReview"));
                assert_eq!(resolved.event, String::from_str(env, "Resolved"));
                assert_eq!(opened.timestamp, model.created_at.unwrap());
                assert_eq!(review.timestamp, model.review_at.unwrap());
                assert_eq!(resolved.timestamp, model.resolved_at.unwrap());
                assert_eq!(review.actor, redacted_address(env));
            }
        }
    }

    #[test]
    fn test_dispute_timeline_props_doc_sync() {
        let docs = include_str!("../../docs/dispute-timeline-invariants.md");
        assert!(
            docs.contains(DOC_STATE_MACHINE_TABLE),
            "docs/dispute-timeline-invariants.md must contain the authoritative state-machine table"
        );
        assert!(
            docs.contains(DOC_GRAMMAR_LINE),
            "docs/dispute-timeline-invariants.md must contain the legal action grammar"
        );
        assert!(
            docs.contains(DOC_AUDIT_NOTE),
            "docs/dispute-timeline-invariants.md must document audit-trail interplay"
        );
    }

    #[test]
    fn test_dispute_timeline_props_randomized_sequences() {
        let mut legal_sequences = 0usize;
        let mut illegal_sequences = 0usize;
        let mut resolved_sequences = 0usize;
        let (env, contract_id, admin, business, currency) = setup();

        for seed in 1..=RANDOMIZED_SEQUENCE_CASES as u64 {
            let mut rng = XorShift64::new(seed);
            let sequence_len = (rng.next_u64() as usize % MAX_ACTIONS_PER_SEQUENCE) + 1;

            let mut action_list = RustVec::new();
            let mut timestamp_list = RustVec::new();
            let mut current_timestamp = 1_000u64;
            for _ in 0..sequence_len {
                action_list.push(action_from_roll(rng.next_u64()));
                current_timestamp = current_timestamp.saturating_add(1 + (rng.next_u64() % 17));
                timestamp_list.push(current_timestamp);
            }

            let model = simulate(&action_list, &timestamp_list);
            env.ledger().set_timestamp(1_000);
            let invoice_id = create_invoice(&env, &contract_id, &business, &currency);

            for (index, action) in action_list.iter().enumerate() {
                env.ledger().set_timestamp(timestamp_list[index]);
                let actual = apply_action(
                    &env,
                    &contract_id,
                    &admin,
                    &business,
                    &invoice_id,
                    *action,
                );

                if index < model.accepted_steps {
                    assert!(
                        actual.is_ok(),
                        "legal prefix action {index} should succeed for seed {seed}"
                    );
                } else {
                    let expected = model.first_error.expect("illegal sequence must have an error");
                    let actual_err = actual.expect_err("illegal action should fail");
                    assert_eq!(
                        actual_err, expected,
                        "illegal action mismatch at step {index} for seed {seed}"
                    );
                    break;
                }
            }

            match model.final_state {
                ModelState::None => {
                    illegal_sequences += 1;
                    let err = env
                        .as_contract(&contract_id, || {
                            QuickLendXContract::get_dispute_timeline(
                                env.clone(),
                                invoice_id.clone(),
                                0,
                                10,
                            )
                        })
                        .expect_err("timeline should not exist without a dispute");
                    assert_eq!(err, QuickLendXError::DisputeNotFound);
                }
                _ => {
                    if model.first_error.is_some() {
                        illegal_sequences += 1;
                    } else {
                        legal_sequences += 1;
                    }
                    if model.final_state == ModelState::Resolved {
                        resolved_sequences += 1;
                    }

                    let timeline = env
                        .as_contract(&contract_id, || {
                            QuickLendXContract::get_dispute_timeline(
                                env.clone(),
                                invoice_id.clone(),
                                0,
                                10,
                            )
                        })
                        .expect("timeline should load");
                    assert_timeline_matches_model(&env, &timeline, &model, &business);
                }
            }
        }

        assert!(legal_sequences > 0, "randomized harness must exercise legal sequences");
        assert!(
            illegal_sequences > 0,
            "randomized harness must exercise illegal sequences"
        );
        assert!(
            resolved_sequences > 0,
            "randomized harness must reach terminal resolved sequences"
        );
    }

    #[test]
    fn test_dispute_timeline_props_resolve_is_terminal() {
        let (env, contract_id, admin, business, currency) = setup();
        let invoice_id = create_invoice(&env, &contract_id, &business, &currency);

        env.ledger().set_timestamp(1_010);
        env.as_contract(&contract_id, || {
            QuickLendXContract::create_dispute(
                env.clone(),
                invoice_id.clone(),
                business.clone(),
                String::from_str(&env, "Reason"),
                String::from_str(&env, "Evidence"),
            )
        })
        .expect("create_dispute should succeed");

        env.ledger().set_timestamp(1_020);
        env.as_contract(&contract_id, || {
            QuickLendXContract::put_dispute_under_review(
                env.clone(),
                invoice_id.clone(),
                admin.clone(),
            )
        })
        .expect("put_dispute_under_review should succeed");

        env.ledger().set_timestamp(1_030);
        env.as_contract(&contract_id, || {
            QuickLendXContract::resolve_dispute(
                env.clone(),
                invoice_id.clone(),
                admin.clone(),
                String::from_str(&env, "Resolution"),
            )
        })
        .expect("resolve_dispute should succeed");

        env.ledger().set_timestamp(1_040);
        let resolve_err = env
            .as_contract(&contract_id, || {
                QuickLendXContract::resolve_dispute(
                    env.clone(),
                    invoice_id.clone(),
                    admin.clone(),
                    String::from_str(&env, "Overwrite"),
                )
            })
            .expect_err("expected contract error for second resolve");
        assert_eq!(resolve_err, QuickLendXError::DisputeNotUnderReview);

        env.ledger().set_timestamp(1_050);
        let review_err = env
            .as_contract(&contract_id, || {
                QuickLendXContract::put_dispute_under_review(
                    env.clone(),
                    invoice_id.clone(),
                    admin.clone(),
                )
            })
            .expect_err("expected contract error for re-review");
        assert_eq!(review_err, QuickLendXError::InvalidStatus);

        env.ledger().set_timestamp(1_060);
        let evidence_err = env
            .as_contract(&contract_id, || {
                QuickLendXContract::update_dispute_evidence(
                    env.clone(),
                    invoice_id.clone(),
                    business.clone(),
                    String::from_str(&env, "Too late"),
                )
            })
            .expect_err("expected contract error for evidence rewrite");
        assert_eq!(evidence_err, QuickLendXError::InvalidStatus);

        let timeline = env
            .as_contract(&contract_id, || {
                QuickLendXContract::get_dispute_timeline(
                    env.clone(),
                    invoice_id.clone(),
                    0,
                    10,
                )
            })
            .expect("timeline should load");
        assert_eq!(timeline.entries.len(), 3);
        let resolved = timeline.entries.get(2).unwrap();
        assert_eq!(resolved.event, String::from_str(&env, "Resolved"));
        assert_eq!(resolved.timestamp, 1_030);
    }
}
