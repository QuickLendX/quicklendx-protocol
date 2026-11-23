#!/bin/bash
git restore quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/client.update_invoice_status(&inv_active, &InvoiceStatus::Verified);/client.try_update_invoice_status(\&inv_active, \&InvoiceStatus::Verified).unwrap();/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
