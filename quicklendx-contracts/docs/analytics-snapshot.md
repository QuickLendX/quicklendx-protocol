# Analytics Snapshot Export

`export_analytics_snapshot()` is the stable read-only analytics entrypoint for
off-chain indexers and dashboards. It bundles the platform and performance
metrics that were previously fetched with separate calls into one deterministic
snapshot.

## Version contract

The current schema version is `ANALYTICS_SCHEMA_VERSION = 1`.

Indexers should persist and validate `schema_version` with every snapshot. A
breaking change to field names, field types, or field semantics requires a
schema-version bump so consumers can reject incompatible data instead of silently
misreading it.

## JSON-equivalent shape

Soroban returns contract values, not JSON, but off-chain indexers can decode the
snapshot into this equivalent JSON shape:

```json
{
  "schema_version": 1,
  "ledger_timestamp": 1710000000,
  "platform_metrics": {
    "total_invoices": 0,
    "total_investments": 0,
    "total_volume": 0,
    "total_fees_collected": 0,
    "active_investors": 0,
    "verified_businesses": 0,
    "average_invoice_amount": 0,
    "average_investment_amount": 0,
    "platform_fee_rate": 0,
    "default_rate": 0,
    "success_rate": 0,
    "timestamp": 1710000000
  },
  "performance_metrics": {
    "platform_uptime": 1710000000,
    "average_settlement_time": 0,
    "average_verification_time": 0,
    "dispute_resolution_time": 0,
    "system_response_time": 0,
    "transaction_success_rate": 0,
    "error_rate": 0,
    "user_satisfaction_score": 0,
    "platform_efficiency": 0
  }
}
```

## Consistency guarantees

The snapshot composes `calculate_platform_metrics` and
`calculate_performance_metrics` inside one contract invocation. Because a
Soroban invocation observes one ledger close and this entrypoint performs no
storage writes, indexers do not see torn reads where one metric reflects a newer
ledger than another.

## Iteration bound

The entrypoint reuses the existing calculators and scans the stored invoice
status indexes. Invoice creation is constrained by the protocol's
`max_invoices_per_business` limit (default: 100 active invoices per business),
and bid-related indexes are separately capped by their module limits. The
snapshot itself introduces no new unbounded storage growth or additional nested
iteration beyond those existing bounded indexes.
