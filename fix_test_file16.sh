#!/bin/bash
git restore quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/client.update_invoice_status(/client.update_invoice_status(/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/fn setup(env: &Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address) {/fn setup(env: \&Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address, Address) {/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/(client, admin, business)/(client, contract_id, admin, business)/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/std::vec::Vec::new()/alloc::vec::Vec::new()/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/soroban_sdk::alloc::format!/alloc::format!/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Let's fix the env.as_contract blocks because update_invoice_status panic'd
# Wait, why did it panic?
# HostError: Error(Contract, #1402) - 1402 is an error code. Let's look up QuickLendXError
