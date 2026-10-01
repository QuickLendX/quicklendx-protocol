#!/bin/bash
git restore quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

sed -i 's/fn setup(env: &Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address) {/fn setup(env: \&Env) -> (QuickLendXContractClient<'"'"'_>, Address, Address, Address) {/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/(client, admin, business)/(client, contract_id, admin, business)/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/std::vec::Vec::new()/alloc::vec::Vec::new()/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/soroban_sdk::alloc::format!/alloc::format!/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

# In the test, I am using `client.update_invoice_status` AND `client.verify_invoice`.
# Wait, NO. I replaced client.update_invoice_status with force_invoice_status.
# So I should use the proper helper I just wrote.

cat << 'INNER_EOF' >> quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

fn force_invoice_status(env: &Env, contract_id: &Address, invoice_id: &BytesN<32>, status: InvoiceStatus) {
    env.as_contract(contract_id, || {
        let mut invoice = InvoiceStorage::get_invoice(env, invoice_id).unwrap();
        InvoiceStorage::remove_from_status_invoices(env, invoice.status, invoice_id);
        invoice.status = status.clone();
        InvoiceStorage::update_invoice(env, &invoice);
        InvoiceStorage::add_to_status_invoices(env, status, invoice_id);
    });
}
INNER_EOF

sed -i 's/client.update_invoice_status(/force_invoice_status(\&env, \&contract_id, /g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
