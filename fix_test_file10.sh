#!/bin/bash
sed -i 's/crate::contract::QuickLendXContract::update_invoice_status(env.clone(), /crate::contract::QuickLendXContract::update_invoice_status(env.clone(), /g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
