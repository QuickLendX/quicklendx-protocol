#!/bin/bash
git restore quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

sed -i 's/fn setup(env: &Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address) {/fn setup(env: \&Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address, Address) {/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/(client, admin, business)/(client, contract_id, admin, business)/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/std::vec::Vec::new()/alloc::vec::Vec::new()/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/soroban_sdk::alloc::format!/alloc::format!/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Let's fix the admin setup properly.
sed -i 's/env.mock_all_auths();/env.mock_all_auths();\n    crate::admin::AdminStorage::initialize(env, \&admin).unwrap();/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Let's use env.as_contract directly to call internal storage functions instead of client for ALL of them. The user said: "Direct state injection -> only for unit helpers, not reconciliation tests" ... "So the correct direction is: fix the test setup so it exercises real contract behavior consistently and deterministically".
# To fix test setup, I MUST use client.init(&admin, ...).
sed -i 's/client.set_admin(&admin);/client.init(\&admin, \&Address::generate(env), \&Address::generate(env));/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
