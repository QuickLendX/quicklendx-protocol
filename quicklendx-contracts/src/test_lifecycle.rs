//! Full invoice lifecycle integration tests for the QuickLendX protocol.

#[cfg(test)]
mod test_lifecycle {
    extern crate alloc;
    use crate::bid::BidStatus;
    use crate::investment::InvestmentStatus;
    use crate::invoice::{InvoiceCategory, InvoiceStatus};
    use crate::verification::BusinessVerificationStatus;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        symbol_short,
        testutils::{Address as _, Events, Ledger},
        token, Address, Env, IntoVal, String, Vec, Symbol, Val, xdr,
    };

    // ─── shared helpers ───────────────────────────────────────────────────────────

    fn make_env() -> (Env, QuickLendXContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        (env, client, admin)
    }

    fn make_real_token(
        env: &Env,
        contract_id: &Address,
        business: &Address,
        investor: &Address,
        business_initial: i128,
        investor_initial: i128,
    ) -> Address {
        let token_admin = Address::generate(env);
        let currency = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let sac = token::StellarAssetClient::new(env, &currency);
        let tok = token::Client::new(env, &currency);

        sac.mint(business, &business_initial);
        sac.mint(investor, &investor_initial);
        sac.mint(contract_id, &1i128);

        let exp = env.ledger().sequence() + 10_000;
        tok.approve(business, contract_id, &(business_initial * 4), &exp);
        tok.approve(investor, contract_id, &(investor_initial * 4), &exp);

        currency
    }

    fn has_event_with_topic(env: &Env, topic: soroban_sdk::Symbol) -> bool {
        let events = env.events().all();
        // Build the topic's XDR ScVal::Symbol by encoding to bytes via env conversion
        // We need to get the symbol name as bytes to match against XDR ScVal::Symbol
        // Use the symbol's binary representation via a soroban Vec<u8> approach
        // symbol_short! produces symbols with names up to 9 chars; encode as ScSymbol
        let topic_str_val = topic.to_val();
        // Convert through env to XDR using the soroban_sdk xdr module's Write trait
        // The simplest approach: compare event count > 0 for now, then filter by symbol name
        // We use symbol bytes via xdr since Symbol doesn't expose string directly
        // Encode a new ScVal::Symbol from the symbol's raw bit encoding
        let topic_scval = {
            // soroban-sdk Symbol is a Val with symbol bit-encoding.
            // Strip to the raw bits and re-encode via env's host directly.
            // Use the soroban_sdk xdr::ScVal conversion available in testutils
            use soroban_sdk::IntoVal;
            let val: soroban_sdk::Val = topic.into_val(env);
            // Use env to_xdr to convert Val to ScVal equivalent
            // The only safe way: compare raw bits of topic as ScVal::Symbol
            // xdr::ScVal::Symbol stores the symbol name as xdr::ScSymbol (a String32)
            // Reconstructing: convert Val to raw u64, then decode symbol chars
            // Symbol chars are 6-bit encoded. We reconstruct the string.
            let raw = val.get_payload();
            // Symbol tag check: bits [0..4] should be SymbolSmall tag (0x2) or SymbolObject
            // For symbol_short! the raw bits encode chars in 6-bit groups, 9 chars max
            let TAG_MASK: u64 = 0xF;
            let SYMBOL_SMALL_TAG: u64 = 0xA; // SymbolSmall tag value in soroban-env
            if raw & TAG_MASK == SYMBOL_SMALL_TAG {
                // Extract up to 9 chars, 6 bits each
                let bits = raw >> 8; // skip 8 tag bits
                let mut name = alloc::vec::Vec::<u8>::new();
                let mut b = bits;
                for _ in 0..9 {
                    let c = b & 0x3F;
                    if c == 0 { break; }
                    let ch = if c < 27 { b'a' + (c as u8 - 1) }
                              else if c < 53 { b'A' + (c as u8 - 27) }
                              else if c < 63 { b'0' + (c as u8 - 53) }
                              else if c == 63 { b'_' }
                              else { b' ' };
                    name.push(ch);
                    b >>= 6;
                }
                name.reverse();
                xdr::ScVal::Symbol(
                    xdr::ScSymbol(xdr::StringM::try_from(name).unwrap_or_default())
                )
            } else {
                xdr::ScVal::Void
            }
        };
        for event in events.events() {
            if let xdr::ContractEventBody::V0(v0) = &event.body {
                for t in v0.topics.iter() {
                    if t == &topic_scval {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn assert_lifecycle_events_emitted(env: &Env) {
        let all = env.events().all();
        let _count = all.events().len();
        assert!(
            has_event_with_topic(env, symbol_short!("inv_up")),
            "InvoiceUploaded (inv_up) event should be emitted"
        );
        assert!(
            has_event_with_topic(env, symbol_short!("inv_ver")),
            "InvoiceVerified (inv_ver) event should be emitted"
        );
        assert!(
            has_event_with_topic(env, symbol_short!("bid_plc")),
            "BidPlaced (bid_plc) event should be emitted"
        );
        assert!(
            has_event_with_topic(env, symbol_short!("bid_acc")),
            "BidAccepted (bid_acc) event should be emitted"
        );
        assert!(
            has_event_with_topic(env, symbol_short!("esc_cr")),
            "EscrowCreated (esc_cr) event should be emitted"
        );
        assert!(
            has_event_with_topic(env, symbol_short!("inv_set")),
            "InvoiceSettled (inv_set) event should be emitted"
        );
        assert!(
            has_event_with_topic(env, symbol_short!("rated")),
            "Rated (rated) event should be emitted"
        );
    }

    fn run_kyc_and_bid(
        env: &Env,
        client: &QuickLendXContractClient,
        admin: &Address,
        business: &Address,
        investor: &Address,
        currency: &Address,
        invoice_amount: i128,
        bid_amount: i128,
    ) -> (soroban_sdk::BytesN<32>, soroban_sdk::BytesN<32>) {
        client.submit_kyc_application(business, &String::from_str(env, "Business KYC"));
        client.verify_business(admin, business);

        let due_date = env.ledger().timestamp() + 86_400;
        let invoice_id = client.upload_invoice(
            business,
            &invoice_amount,
            currency,
            &due_date,
            &String::from_str(env, "Consulting services invoice"),
            &InvoiceCategory::Consulting,
            &Vec::new(env),
        );
        client.verify_invoice(&invoice_id);

        client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
        client.verify_investor(investor, &50_000i128);

        let bid_id = client.place_bid(investor, &invoice_id, &bid_amount, &invoice_amount);

        (invoice_id, bid_id)
    }

    #[test]
    fn test_full_invoice_lifecycle() {
        let (env, client, admin) = make_env();
        let contract_id = client.address.clone();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);

        let invoice_amount: i128 = 10_000;
        let bid_amount: i128 = 9_000;
        let currency = make_real_token(&env, &contract_id, &business, &investor, 20_000, 15_000);
        let tok = token::Client::new(&env, &currency);

        let (invoice_id, bid_id) = run_kyc_and_bid(
            &env, &client, &admin, &business, &investor, &currency,
            invoice_amount, bid_amount,
        );

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.status, InvoiceStatus::Verified);

        let bid = client.get_bid(&bid_id).unwrap();
        assert_eq!(bid.status, BidStatus::Placed);

        client.accept_bid(&invoice_id, &bid_id);
        assert_eq!(tok.balance(&investor), 15_000 - bid_amount);
        assert_eq!(tok.balance(&contract_id), 1 + bid_amount);

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.status, InvoiceStatus::Funded);

        let investment = client.get_invoice_investment(&invoice_id);
        assert_eq!(investment.status, InvestmentStatus::Active);

        let sac = token::StellarAssetClient::new(&env, &currency);
        sac.mint(&business, &invoice_amount);
        let tok_exp = env.ledger().sequence() + 10_000;
        tok.approve(&business, &contract_id, &(invoice_amount * 4), &tok_exp);

        client.settle_invoice(&invoice_id, &invoice_amount);
        assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Paid);
        assert_eq!(
            client.get_invoice_investment(&invoice_id).status,
            InvestmentStatus::Completed
        );

        let rating: u32 = 5;
        client.add_invoice_rating(
            &invoice_id,
            &rating,
            &String::from_str(&env, "Excellent! Payment on time."),
            &investor,
        );

        assert_lifecycle_events_emitted(&env);
    }

    #[test]
    fn test_lifecycle_escrow_token_flow() {
        let (env, client, admin) = make_env();
        let contract_id = client.address.clone();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);

        let invoice_amount: i128 = 10_000;
        let bid_amount: i128 = 9_000;
        let currency = make_real_token(&env, &contract_id, &business, &investor, 5_000, 15_000);
        let tok = token::Client::new(&env, &currency);

        let (invoice_id, bid_id) = run_kyc_and_bid(
            &env, &client, &admin, &business, &investor, &currency,
            invoice_amount, bid_amount,
        );

        client.accept_bid(&invoice_id, &bid_id);
        client.release_escrow_funds(&invoice_id);

        assert_eq!(tok.balance(&business), 5_000 + bid_amount);
        assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Funded);

        let rating: u32 = 4;
        client.add_invoice_rating(
            &invoice_id,
            &rating,
            &String::from_str(&env, "Good experience overall."),
            &investor,
        );

        assert!(env.events().all().events().len() >= 5);
    }

    #[test]
    fn test_full_lifecycle_step_by_step() {
        let (env, client, admin) = make_env();
        let contract_id = client.address.clone();
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let invoice_amount: i128 = 10_000;
        let bid_amount: i128 = 9_000;
        let currency = make_real_token(&env, &contract_id, &business, &investor, 20_000, 15_000);
        let tok = token::Client::new(&env, &currency);

        client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
        assert!(has_event_with_topic(&env, symbol_short!("kyc_sub")));

        client.verify_business(&admin, &business);
        assert!(has_event_with_topic(&env, symbol_short!("bus_ver")));

        let due_date = env.ledger().timestamp() + 86_400;
        let invoice_id = client.upload_invoice(
            &business,
            &invoice_amount,
            &currency,
            &due_date,
            &String::from_str(&env, "Consulting services invoice"),
            &InvoiceCategory::Consulting,
            &Vec::new(&env),
        );
        assert!(has_event_with_topic(&env, symbol_short!("inv_up")));

        client.verify_invoice(&invoice_id);
        assert!(has_event_with_topic(&env, symbol_short!("inv_ver")));

        client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
        client.verify_investor(&investor, &50_000i128);
        assert!(has_event_with_topic(&env, symbol_short!("inv_veri")));

        let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &invoice_amount);
        assert!(has_event_with_topic(&env, symbol_short!("bid_plc")));

        client.accept_bid(&invoice_id, &bid_id);
        assert!(has_event_with_topic(&env, symbol_short!("bid_acc")));
        assert!(has_event_with_topic(&env, symbol_short!("esc_cr")));

        let sac = token::StellarAssetClient::new(&env, &currency);
        sac.mint(&business, &invoice_amount);
        let exp = env.ledger().sequence() + 10_000;
        tok.approve(&business, &contract_id, &(invoice_amount * 4), &exp);
        client.settle_invoice(&invoice_id, &invoice_amount);
        assert!(has_event_with_topic(&env, symbol_short!("inv_set")));

        let rating: u32 = 5;
        client.add_invoice_rating(
            &invoice_id,
            &rating,
            &String::from_str(&env, "Excellent! Payment on time."),
            &investor,
        );
        assert!(has_event_with_topic(&env, symbol_short!("rated")));

        assert_lifecycle_events_emitted(&env);
    }
}
