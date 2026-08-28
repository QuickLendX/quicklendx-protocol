//! Regression coverage for content-addressed dispute evidence.

#[cfg(test)]
mod tests {
    use crate::dispute::reserve_evidence;
    use crate::errors::QuickLendXError;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, BytesN, Env, String};

    fn invoice(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    fn actor(env: &Env) -> Address {
        Address::generate(env)
    }

    #[test]
    fn first_submission_is_stored_for_its_invoice() {
        let env = Env::default();
        let id = invoice(&env, 11);
        let who = actor(&env);
        let payload = String::from_str(&env, "evidence-11");
        let digest = reserve_evidence(&env, &id, &who, &payload).unwrap();
        assert_eq!(digest, env.crypto().sha256(&payload.to_bytes()));
    }

    #[test]
    fn exact_retry_is_rejected() {
        let env = Env::default();
        let id = invoice(&env, 12);
        let who = actor(&env);
        let payload = String::from_str(&env, "same-evidence");
        reserve_evidence(&env, &id, &who, &payload).unwrap();
        assert_eq!(
            reserve_evidence(&env, &id, &who, &payload),
            Err(QuickLendXError::InvalidDisputeEvidence)
        );
    }

    #[test]
    fn cross_invoice_replay_is_rejected() {
        let env = Env::default();
        let first = invoice(&env, 13);
        let second = invoice(&env, 14);
        let who = actor(&env);
        let payload = String::from_str(&env, "cross-invoice-attachment");
        reserve_evidence(&env, &first, &who, &payload).unwrap();
        assert_eq!(
            reserve_evidence(&env, &second, &who, &payload),
            Err(QuickLendXError::InvalidDisputeEvidence)
        );
    }

    #[test]
    fn payload_change_creates_a_distinct_identity() {
        let env = Env::default();
        let id = invoice(&env, 15);
        let who = actor(&env);
        let first = String::from_str(&env, "attachment-a");
        let second = String::from_str(&env, "attachment-b");
        let first_digest = reserve_evidence(&env, &id, &who, &first).unwrap();
        let second_digest = reserve_evidence(&env, &id, &who, &second).unwrap();
        assert_ne!(first_digest, second_digest);
    }

    #[test]
    fn different_actors_do_not_bypass_content_identity() {
        let env = Env::default();
        let id = invoice(&env, 16);
        let first_actor = actor(&env);
        let second_actor = actor(&env);
        let payload = String::from_str(&env, "actor-bound-content");
        reserve_evidence(&env, &id, &first_actor, &payload).unwrap();
        let result = reserve_evidence(&env, &id, &second_actor, &payload);
        assert_eq!(result, Err(QuickLendXError::InvalidDisputeEvidence));
    }

    #[test]
    fn evidence_digest_is_stable_for_indexers() {
        let env = Env::default();
        let id = invoice(&env, 17);
        let who = actor(&env);
        let payload = String::from_str(&env, "stable-indexer-reference");
        let expected = env.crypto().sha256(&payload.to_bytes());
        let actual = reserve_evidence(&env, &id, &who, &payload).unwrap();
        assert_eq!(actual, expected);
    }
}
