#!/bin/bash
git restore quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

sed -i 's/use crate::contract::{QuickLendXContract, QuickLendXContractClient};/use crate::{QuickLendXContract, QuickLendXContractClient};/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/fn setup(env: &Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address) {/fn setup(env: \&Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address, Address) {/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/(client, admin, business)/(client, contract_id, admin, business)/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# We have `std::vec::Vec::new()` which shouldn't be touched in the `upload` parameters. Ah!
# The `alloc::vec::Vec` was replacing the `soroban_sdk::Vec::new(env)`
# Let's not use sed on `Vec::new` blindly!
sed -i 's/std::vec::Vec::new()/alloc::vec::Vec::new()/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Let's fix the admin setup properly.
sed -i 's/env.mock_all_auths();/env.mock_all_auths();\n    env.as_contract(\&contract_id, || { crate::admin::AdminStorage::initialize(env, \&admin).unwrap(); });/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Use try_update_invoice_status
# BE CAREFUL, sed replaces all occurrences.
sed -i 's/client.update_invoice_status(/client.try_update_invoice_status(/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Replace specific ends of line
sed -i 's/InvoiceStatus::Verified);/InvoiceStatus::Verified).unwrap();/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/InvoiceStatus::Funded);/InvoiceStatus::Funded).unwrap();/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/InvoiceStatus::Paid);/InvoiceStatus::Paid).unwrap();/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/InvoiceStatus::Defaulted);/InvoiceStatus::Defaulted).unwrap();/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Cancelled requires `cancel_invoice`
sed -i 's/client.try_update_invoice_status(\&inv_cancelled, \&InvoiceStatus::Cancelled);/client.try_cancel_invoice(\&inv_cancelled).unwrap();/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
