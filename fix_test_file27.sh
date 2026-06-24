#!/bin/bash
git restore quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

sed -i 's/fn setup(env: &Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address) {/fn setup(env: \&Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address, Address) {/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/(client, admin, business)/(client, contract_id, admin, business)/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/std::vec::Vec::new()/alloc::vec::Vec::new()/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/soroban_sdk::alloc::format!/alloc::format!/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# Let's bypass default grace period constraints in tests by advancing ledger timestamp
# When testing defaulting, we need timestamp to be past due_date + grace_period
# due_date is ts + 86_400. grace_period is typically 3 days (259_200). Let's just advance by 10_000_000 seconds before update_invoice_status.
# Wait, actually it's way easier to just add a helper to update invoice status directly via InvoiceStorage like test_insurance.rs does! No, user said NO direct state injection.

sed -i 's/client.update_invoice_status(\&inv_defaulted, \&InvoiceStatus::Defaulted);/env.ledger().set_timestamp(env.ledger().timestamp() + 10_000_000);\n    client.update_invoice_status(\&inv_defaulted, \&InvoiceStatus::Defaulted);/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/client.update_invoice_status(\&inv1, \&InvoiceStatus::Defaulted);/env.ledger().set_timestamp(env.ledger().timestamp() + 10_000_000);\n    client.update_invoice_status(\&inv1, \&InvoiceStatus::Defaulted);/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/client.update_invoice_status(inv, \&InvoiceStatus::Defaulted);/env.ledger().set_timestamp(env.ledger().timestamp() + 10_000_000);\n        client.update_invoice_status(inv, \&InvoiceStatus::Defaulted);/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
