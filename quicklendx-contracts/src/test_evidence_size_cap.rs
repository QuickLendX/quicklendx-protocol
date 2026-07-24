/// Boundary tests for dispute evidence size cap (issue #1837).
///
/// # Coverage
///
/// These tests lock in the `MAX_DISPUTE_EVIDENCE_LENGTH` (2000) boundary for
/// `validate_dispute_evidence`. The boundary must be stable: any change to
/// the constant or the comparison operator must cause at least one test here
/// to fail, making regressions visible at build time.
///
/// | Test name | Input length | Expected outcome |
/// |---|---|---|
/// | `evidence_at_minimum_boundary_accepted`       | 1     | Ok(())                          |
/// | `evidence_below_cap_accepted`                 | 1999  | Ok(())                          |
/// | `evidence_at_cap_accepted`                    | 2000  | Ok(())   ← the "at-cap" case   |
/// | `evidence_one_over_cap_rejected`              | 2001  | Err(InvalidDisputeEvidence) ← "+1" |
/// | `evidence_well_over_cap_rejected`             | 4000  | Err(InvalidDisputeEvidence)     |
/// | `evidence_empty_rejected`                     | 0     | Err(InvalidDisputeEvidence)     |
///
/// All tests are deterministic and `#![no_std]`-clean: they use only
/// `soroban_sdk` primitives and `str::repeat` for string construction.
#[cfg(test)]
mod test_evidence_size_cap {
    use crate::errors::QuickLendXError;
    use crate::protocol_limits::MAX_DISPUTE_EVIDENCE_LENGTH;
    use crate::types::{EvidenceKind, InvoiceCategory};
    use crate::verification::validate_dispute_evidence;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

    // ------------------------------------------------------------------
    // Helper
    // ------------------------------------------------------------------

    /// Build a `soroban_sdk::String` of exactly `len` ASCII 'x' characters.
    ///
    /// Uses `str::repeat` — the idiomatic `no_std`-compatible approach that
    /// avoids allocating an intermediate `Vec` of chars.
    fn make_evidence(env: &Env, len: usize) -> String {
        let s = "x".repeat(len);
        String::from_str(env, &s)
    }

    // ------------------------------------------------------------------
    // Happy path: accepted inputs
    // ------------------------------------------------------------------

    /// Evidence of exactly 1 character (minimum boundary) must be accepted.
    #[test]
    fn evidence_at_minimum_boundary_accepted() {
        let env = Env::default();
        let evidence = make_evidence(&env, 1);
        assert!(
            validate_dispute_evidence(&evidence).is_ok(),
            "1-char evidence should be accepted (minimum boundary)"
        );
    }

    /// Evidence one byte below the cap (1999 chars) must be accepted.
    #[test]
    fn evidence_below_cap_accepted() {
        let env = Env::default();
        let len = (MAX_DISPUTE_EVIDENCE_LENGTH - 1) as usize;
        let evidence = make_evidence(&env, len);
        assert!(
            validate_dispute_evidence(&evidence).is_ok(),
            "{len}-char evidence should be accepted (one below cap)"
        );
    }

    /// Evidence of exactly `MAX_DISPUTE_EVIDENCE_LENGTH` (2000) characters
    /// must be accepted — this is the "at cap" boundary from issue #1837.
    ///
    /// If this test starts failing it means the constant or the comparison
    /// changed from inclusive (`>`) to exclusive (`>=`), which would be a
    /// breaking change to the contract's public interface.
    #[test]
    fn evidence_at_cap_accepted() {
        let env = Env::default();
        let len = MAX_DISPUTE_EVIDENCE_LENGTH as usize;
        let evidence = make_evidence(&env, len);
        assert!(
            validate_dispute_evidence(&evidence).is_ok(),
            "{len}-char evidence should be accepted (at cap = {MAX_DISPUTE_EVIDENCE_LENGTH})"
        );
    }

    // ------------------------------------------------------------------
    // Sad path: rejected inputs
    // ------------------------------------------------------------------

    /// Evidence of exactly `MAX_DISPUTE_EVIDENCE_LENGTH + 1` (2001) characters
    /// must be rejected — this is the "+1" boundary from issue #1837.
    ///
    /// If this test starts passing it means the cap enforcement was removed or
    /// the constant was raised without a deliberate decision.
    #[test]
    fn evidence_one_over_cap_rejected() {
        let env = Env::default();
        let len = (MAX_DISPUTE_EVIDENCE_LENGTH + 1) as usize;
        let evidence = make_evidence(&env, len);
        let result = validate_dispute_evidence(&evidence);
        assert!(
            result.is_err(),
            "{len}-char evidence should be rejected (one over cap)"
        );
        assert_eq!(
            result.unwrap_err(),
            QuickLendXError::InvalidDisputeEvidence,
            "expected InvalidDisputeEvidence for {len}-char input"
        );
    }

    /// Evidence well over cap (4000 chars) must be rejected.
    #[test]
    fn evidence_well_over_cap_rejected() {
        let env = Env::default();
        let len = (MAX_DISPUTE_EVIDENCE_LENGTH * 2) as usize;
        let evidence = make_evidence(&env, len);
        let result = validate_dispute_evidence(&evidence);
        assert!(
            result.is_err(),
            "{len}-char evidence should be rejected (well over cap)"
        );
        assert_eq!(
            result.unwrap_err(),
            QuickLendXError::InvalidDisputeEvidence,
            "expected InvalidDisputeEvidence for {len}-char input"
        );
    }

    /// An empty evidence string must be rejected.
    ///
    /// Disputes require evidence to prevent frivolous filings — an empty
    /// string is the degenerate rejection case.
    #[test]
    fn evidence_empty_rejected() {
        let env = Env::default();
        let evidence = String::from_str(&env, "");
        let result = validate_dispute_evidence(&evidence);
        assert!(result.is_err(), "empty evidence should be rejected");
        assert_eq!(
            result.unwrap_err(),
            QuickLendXError::InvalidDisputeEvidence,
            "expected InvalidDisputeEvidence for empty input"
        );
    }

    // ------------------------------------------------------------------
    // Constant sanity checks
    // ------------------------------------------------------------------

    /// `MAX_DISPUTE_EVIDENCE_LENGTH` must equal exactly 2000.
    ///
    /// This test locks the constant in place. Any change to the numeric value
    /// surfaces here before it reaches a deployed contract.
    #[test]
    fn max_evidence_constant_is_2000() {
        assert_eq!(
            MAX_DISPUTE_EVIDENCE_LENGTH, 2000,
            "MAX_DISPUTE_EVIDENCE_LENGTH must be 2000; update this test intentionally if changing the cap"
        );
    }

    // ------------------------------------------------------------------
    // update_dispute_evidence integration: same cap applies to updates
    // ------------------------------------------------------------------

    /// The cap enforced at dispute creation is also enforced by
    /// `update_dispute_evidence`. Updating with evidence at exactly the cap
    /// (2000 chars) must succeed; updating with cap+1 (2001 chars) must fail
    /// with `InvalidDisputeEvidence`.
    ///
    /// This test locks in that the contract entrypoint calls
    /// `validate_dispute_evidence` before writing to storage, and that
    /// the cap is not accidentally bypassed on updates.
    #[test]
    fn update_evidence_at_cap_accepted_one_over_rejected() {
        use crate::invoice::InvoiceCategory;
        use crate::{QuickLendXContract, QuickLendXContractClient};
        use soroban_sdk::testutils::Address as _;
        use soroban_sdk::{Address, Vec};

        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        // Bootstrap: set admin, KYC a business, store an invoice.
        client.set_admin(&admin);
        let business = Address::generate(&env);
        client.submit_kyc_application(&business, &String::from_str(&env, "kyc"));
        client.verify_business(&admin, &business);

        let currency = Address::generate(&env);
        let due_date = env.ledger().timestamp() + 86_400;
        let invoice_id = client.store_invoice(
            &business,
            &100_000_i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Invoice for evidence-cap integration test"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );

        // Open a dispute with minimal (1-char) evidence so we have a Disputed invoice.
        let reason = String::from_str(&env, "reason");
        let initial_evidence = make_evidence(&env, 1);
        client.create_dispute(&invoice_id, &business, &reason, &initial_evidence, &EvidenceKind::Document);

        // Happy path: update with evidence at the cap (2000 chars) must succeed.
        let at_cap = make_evidence(&env, MAX_DISPUTE_EVIDENCE_LENGTH as usize);
        assert!(
            client
                .try_update_dispute_evidence(&invoice_id, &business, &at_cap)
                .is_ok(),
            "update_dispute_evidence with 2000-char evidence should be accepted (at cap)"
        );

        // Sad path: update with evidence one over the cap (2001 chars) must fail
        // with the same error code that create_dispute would return.
        let one_over = make_evidence(&env, (MAX_DISPUTE_EVIDENCE_LENGTH + 1) as usize);
        let err = client
            .try_update_dispute_evidence(&invoice_id, &business, &one_over)
            .unwrap_err()
            .unwrap();
        assert_eq!(
            err,
            QuickLendXError::InvalidDisputeEvidence,
            "update_dispute_evidence with 2001-char evidence should be rejected (one over cap)"
        );
    }

    #[test]
    fn test_dispute_creator_and_evidence_kind_matrix() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let amount = 1000i128;
        let due_date = env.ledger().timestamp() + 86400;

        // Set admin, verify business and investor so KYC checks pass.
        client.set_admin(&admin);
        client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
        client.verify_business(&admin, &business);
        client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
        client.verify_investor(&investor, &(amount * 2));

        // Register a token for place_bid / accept_bid
        let token_admin = Address::generate(&env);
        let currency = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let sac = token::StellarAssetClient::new(&env, &currency);
        let tok = token::Client::new(&env, &currency);
        sac.mint(&investor, &(amount * 10));
        let expiry = env.ledger().sequence() + 10_000;
        tok.approve(&investor, &contract_id, &(amount * 10), &expiry);

        let description = String::from_str(&env, "Test invoice");

        // Test cases for Business Creator
        // Case 1: Business + Document -> Ok
        {
            let invoice_id = client.upload_invoice(
                &business,
                &amount,
                &currency,
                &due_date,
                &description,
                &InvoiceCategory::Services,
                &Vec::new(&env),
            );
            client.verify_invoice(&invoice_id);

            let result = client.try_create_dispute(
                &invoice_id,
                &business,
                &String::from_str(&env, "reason"),
                &String::from_str(&env, "evidence"),
                &EvidenceKind::Document,
            );
            assert!(result.is_ok());
        }

        // Case 2: Business + Transcript -> Ok
        {
            let invoice_id = client.upload_invoice(
                &business,
                &amount,
                &currency,
                &due_date,
                &description,
                &InvoiceCategory::Services,
                &Vec::new(&env),
            );
            client.verify_invoice(&invoice_id);

            let result = client.try_create_dispute(
                &invoice_id,
                &business,
                &String::from_str(&env, "reason"),
                &String::from_str(&env, "evidence"),
                &EvidenceKind::Transcript,
            );
            assert!(result.is_ok());
        }

        // Case 3: Business + Other -> Error (InvalidDisputeEvidenceKind)
        {
            let invoice_id = client.upload_invoice(
                &business,
                &amount,
                &currency,
                &due_date,
                &description,
                &InvoiceCategory::Services,
                &Vec::new(&env),
            );
            client.verify_invoice(&invoice_id);

            let result = client.try_create_dispute(
                &invoice_id,
                &business,
                &String::from_str(&env, "reason"),
                &String::from_str(&env, "evidence"),
                &EvidenceKind::Other,
            );
            assert_eq!(result, Err(Ok(QuickLendXError::InvalidDisputeEvidenceKind)));
        }

        // Test cases for Investor Creator
        // Case 4: Investor + Document -> Ok
        {
            let invoice_id = client.upload_invoice(
                &business,
                &amount,
                &currency,
                &due_date,
                &description,
                &InvoiceCategory::Services,
                &Vec::new(&env),
            );
            client.verify_invoice(&invoice_id);
            let bid_id = client.place_bid(
                &investor,
                &invoice_id,
                &amount,
                &(amount + 100),
                &BytesN::from_array(&env, &[0u8; 32]),
            );
            client.accept_bid_and_fund(&invoice_id, &bid_id);

            let result = client.try_create_dispute(
                &invoice_id,
                &investor,
                &String::from_str(&env, "reason"),
                &String::from_str(&env, "evidence"),
                &EvidenceKind::Document,
            );
            assert!(result.is_ok());
        }

        // Case 5: Investor + Transcript -> Ok
        {
            let invoice_id = client.upload_invoice(
                &business,
                &amount,
                &currency,
                &due_date,
                &description,
                &InvoiceCategory::Services,
                &Vec::new(&env),
            );
            client.verify_invoice(&invoice_id);
            let bid_id = client.place_bid(
                &investor,
                &invoice_id,
                &amount,
                &(amount + 100),
                &BytesN::from_array(&env, &[0u8; 32]),
            );
            client.accept_bid_and_fund(&invoice_id, &bid_id);

            let result = client.try_create_dispute(
                &invoice_id,
                &investor,
                &String::from_str(&env, "reason"),
                &String::from_str(&env, "evidence"),
                &EvidenceKind::Transcript,
            );
            assert!(result.is_ok());
        }

        // Case 6: Investor + Other -> Error (InvalidDisputeEvidenceKind)
        {
            let invoice_id = client.upload_invoice(
                &business,
                &amount,
                &currency,
                &due_date,
                &description,
                &InvoiceCategory::Services,
                &Vec::new(&env),
            );
            client.verify_invoice(&invoice_id);
            let bid_id = client.place_bid(
                &investor,
                &invoice_id,
                &amount,
                &(amount + 100),
                &BytesN::from_array(&env, &[0u8; 32]),
            );
            client.accept_bid_and_fund(&invoice_id, &bid_id);

            let result = client.try_create_dispute(
                &invoice_id,
                &investor,
                &String::from_str(&env, "reason"),
                &String::from_str(&env, "evidence"),
                &EvidenceKind::Other,
            );
            assert_eq!(result, Err(Ok(QuickLendXError::InvalidDisputeEvidenceKind)));
        }
    }
}
