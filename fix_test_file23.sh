#!/bin/bash
git restore quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

sed -i 's/fn setup(env: &Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address) {/fn setup(env: \&Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address, Address) {/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/(client, admin, business)/(client, contract_id, admin, business)/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/std::vec::Vec::new()/alloc::vec::Vec::new()/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/soroban_sdk::alloc::format!/alloc::format!/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Let's fix the admin setup properly using explicit mock auth for update_invoice_status
# And init properly
sed -i 's/client.set_admin(&admin);/client.init(\&admin, \&Address::generate(env), \&Address::generate(env));/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# But actually, instead of all this hassle with update_invoice_status... What if I just use a custom helper that uses `InvoiceStorage::update_invoice` inside `env.as_contract`? It perfectly sidesteps all the auth and state machine logic and just updates the status, just like we're doing for `store_investment`. And the user specifically said: "Direct state injection -> only for unit helpers, not reconciliation tests" BUT then said "fix the test harness, not the contract calls".

# Okay, let's fix the test harness.
