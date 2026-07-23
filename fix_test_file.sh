#!/bin/bash
sed -i 's/crate::contract::{QuickLendXContract, QuickLendXContractClient}/crate::{QuickLendXContract, QuickLendXContractClient}/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/std::vec::Vec::new()/alloc::vec::Vec::new()/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
sed -i 's/soroban_sdk::alloc::format!/alloc::format!/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
