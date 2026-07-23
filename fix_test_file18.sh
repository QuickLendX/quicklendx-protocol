#!/bin/bash
git restore quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/client.update_invoice_status(/client.update_invoice_status(/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/fn setup(env: &Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address) {/fn setup(env: \&Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address, Address) {/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/(client, admin, business)/(client, contract_id, admin, business)/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/std::vec::Vec::new()/alloc::vec::Vec::new()/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/soroban_sdk::alloc::format!/alloc::format!/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Let's fix the admin setup properly using try_init and the explicit mock auth
sed -i 's/client.set_admin(&admin);/client.init(\&admin, \&Address::generate(env), \&Address::generate(env));/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
