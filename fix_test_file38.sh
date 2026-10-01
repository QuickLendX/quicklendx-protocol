#!/bin/bash
git restore quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

sed -i 's/fn setup(env: &Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address) {/fn setup(env: \&Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address, Address) {/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/(client, admin, business)/(client, contract_id, admin, business)/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/std::vec::Vec::new()/alloc::vec::Vec::new()/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/soroban_sdk::alloc::format!/alloc::format!/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# In the test, I was doing `client.update_invoice_status`. Let's fix that.
# Why did `client.update_invoice_status` fail with 1402 earlier? It didn't fail in `test_analytics_consistency` because in `test_analytics_consistency`, mock_all_auths was on and `client.set_admin(&admin)` was used. Wait! I never checked if `set_admin` actually makes `update_invoice_status` work properly!
# BUT let's just use the `call_update_invoice_status` helper we already wrote in one of the attempts.
# I'll just rewrite the file fully with that helper.
