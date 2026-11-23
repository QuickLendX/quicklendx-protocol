#!/bin/bash
git restore quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

sed -i 's/fn setup(env: &Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address) {/fn setup(env: \&Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address, Address) {/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/(client, admin, business)/(client, contract_id, admin, business)/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/std::vec::Vec::new()/alloc::vec::Vec::new()/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/soroban_sdk::alloc::format!/alloc::format!/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Let's fix the admin setup properly using explicit mock auth for update_invoice_status
# Wait, actually, if I just replace client.update_invoice_status(id, status) with a direct write it works perfectly...
# Oh, the user explicitly asked me NOT to do that! "Good instinct to pause here — but you shouldn’t bypass update_invoice_status. That would quietly defeat the whole point of a reconciliation test suite".

# So we need to call update_invoice_status correctly.
# Why did client.update_invoice_status fail with 1402 (OperationNotAllowed)?
# Because AdminStorage requires the contract to be initialized!

sed -i 's/client.set_admin(&admin);/client.init(\&admin, \&Address::generate(env), \&Address::generate(env));/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
