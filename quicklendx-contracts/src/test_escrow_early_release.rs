#[cfg(test)]
mod tests {
    use crate::errors::QuickLendXError;
    use crate::invoice::InvoiceCategory;
    use crate::payments::EscrowStatus;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token, Address, BytesN, Env, String, Vec,
    };

    fn funded_invoice(
        env: &Env,
        client: &QuickLendXContractClient,
        contract_id: &Address,
    ) -> (BytesN<32>, Address, Address, Address) {
        let admin = Address::generate(env);
        let business = Address::generate(env);
        let investor = Address::generate(env);
        let token_admin = Address::generate(env);
        let currency = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let token_client = token::Client::new(env, &currency);
        let sac_client = token::StellarAssetClient::new(env, &currency);
        let amount = 10_000i128;
        sac_client.mint(&business, &amount);
        sac_client.mint(&investor, &amount);
        let expiration = env.ledger().sequence() + 10_000;
        token_client.approve(&business, contract_id, &amount, &expiration);
        token_client.approve(&investor, contract_id, &amount, &expiration);
        client.set_admin(&admin);
        client.submit_kyc_application(&business, &String::from_str(env, "business kyc"));
        client.verify_business(&admin, &business);
        client.submit_investor_kyc(&investor, &String::from_str(env, "investor kyc"));
        client.verify_investor(&investor, &amount);
        let due_date = env.ledger().timestamp() + 86_400;
        let invoice_id = client.store_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &String::from_str(env, "early release invoice"),
            &InvoiceCategory::Services,
            &Vec::new(env),
            &None);
        client.verify_invoice(&invoice_id);
        let bid_id = client.place_bid(
            &investor,
            &invoice_id,
            &amount,
            &(amount + 500),
            &BytesN::from_array(env, &[0; 32]),
        );
        client.accept_bid(&invoice_id, &bid_id);
        (invoice_id, business, investor, currency)
    }

    #[test]
    fn early_release_stays_held_when_only_business_approves() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, business, _, _) = funded_invoice(&env, &client, &contract_id);
        client.approve_early_escrow_release(&invoice_id, &business);
        let err = client
            .try_execute_early_escrow_release(&invoice_id)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, QuickLendXError::OperationNotAllowed);
        assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
    }

    #[test]
    fn early_release_succeeds_when_business_and_investor_approve() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, business, investor, currency) =
            funded_invoice(&env, &client, &contract_id);
        let token_client = token::Client::new(&env, &currency);
        let business_before = token_client.balance(&business);
        client.approve_early_escrow_release(&invoice_id, &business);
        client.approve_early_escrow_release(&invoice_id, &investor);
        client.execute_early_escrow_release(&invoice_id);
        assert_eq!(
            client.get_escrow_status(&invoice_id),
            EscrowStatus::Released
        );
        assert!(token_client.balance(&business) > business_before);
    }

    #[test]
    fn release_escrow_funds_rejected_without_dual_approval() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, business, _, _) = funded_invoice(&env, &client, &contract_id);

        // release_escrow_funds must not bypass the dual-approval gate.
        let err = client
            .try_release_escrow_funds(&invoice_id)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, QuickLendXError::OperationNotAllowed);
        assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
    }

    #[test]
    fn release_escrow_funds_succeeds_with_dual_approval() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, business, investor, _) = funded_invoice(&env, &client, &contract_id);
        client.approve_early_escrow_release(&invoice_id, &business);
        client.approve_early_escrow_release(&invoice_id, &investor);
        client.release_escrow_funds(&invoice_id);
        assert_eq!(
            client.get_escrow_status(&invoice_id),
            EscrowStatus::Released
        );
    }

    #[test]
    fn early_release_stays_held_when_one_party_approves_then_revokes() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let (invoice_id, business, investor, _) = funded_invoice(&env, &client, &contract_id);
        client.approve_early_escrow_release(&invoice_id, &business);
        client.approve_early_escrow_release(&invoice_id, &investor);
        client.revoke_early_escrow_release(&invoice_id, &business);
        let err = client
            .try_execute_early_escrow_release(&invoice_id)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, QuickLendXError::OperationNotAllowed);
        assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
    }
}
