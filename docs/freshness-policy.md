# Freshness Oracle Drift Policy

## Overview
The QuickLendX Protocol uses external oracle feeds to evaluate and settle real-time financial invoice details. To prevent arbitrage and manipulation using obsolete or delayed oracle reports, an upper limit on data lag is strictly enforced.

## Parameters
* `max_freshness_drift_secs`: The maximum duration in seconds allowed between the oracle's verification timestamp and the current ledger timestamp.

## Default Baseline
* **Default Value**: 60 seconds.
* This parameter defaults to a conservative standard ensuring high data fidelity out of the box, mitigating systemic pricing lags.

## Governance & Security Notes
1. **Admin Gated Setter**: Modifications to `max_freshness_drift_secs` can exclusively be completed by the contract's verified administrative entity (`require_admin`).
2. **Stale Data Risks**: Setting this configuration bound to a high threshold poses a severe security risk. Over-extended drifts accept obsolete data points, opening vectors for front-running or incorrect valuation models.
3. **Rejection Events**: Whenever an oracle value exceeds the maximum drift bounds, the runtime throws a `StaleDataRejected` panic error and immediately emits a `freshness_rejected` audit event for off-chain monitoring tracking.