#!/bin/bash
sed -i 's/client.update_invoice_status(/client.verify_invoice(/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
