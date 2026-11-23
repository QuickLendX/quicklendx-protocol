#!/bin/bash
git restore quicklendx-contracts/src/test_platform_metrics_reconciliation.rs

sed -i 's/use crate::contract::{QuickLendXContract, QuickLendXContractClient};/use crate::{QuickLendXContract, QuickLendXContractClient};/g' quicklendx-contracts/src/test_platform_metrics_reconciliation.rs
