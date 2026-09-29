#!/bin/bash
git restore quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

sed -i 's/fn setup(env: &Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address) {/fn setup(env: \&Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address, Address) {/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/(client, admin, business)/(client, contract_id, admin, business)/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/std::vec::Vec::new()/alloc::vec::Vec::new()/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/soroban_sdk::alloc::format!/alloc::format!/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Let's fix the admin setup properly.
sed -i 's/env.mock_all_auths();/env.mock_all_auths();\n    env.as_contract(\&contract_id, || { crate::admin::AdminStorage::initialize(env, \&admin).unwrap(); });/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Let's completely undo the attempt to write verify_invoice.
# Use try_update_invoice_status
sed -i 's/client.update_invoice_status(/client.try_update_invoice_status(/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

sed -i 's/InvoiceStatus::Verified);/InvoiceStatus::Verified).unwrap();/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/InvoiceStatus::Funded);/InvoiceStatus::Funded).unwrap();/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/InvoiceStatus::Paid);/InvoiceStatus::Paid).unwrap();/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/InvoiceStatus::Defaulted);/InvoiceStatus::Defaulted).unwrap();/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/InvoiceStatus::Cancelled);/InvoiceStatus::Cancelled).unwrap();/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# In test_analytics_consistency setup uses client.set_admin(&admin). We need this too.
sed -i 's/client.set_admin(&admin);/client.init(\&admin, \&Address::generate(env), \&Address::generate(env));\n    client.set_admin(\&admin);/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
