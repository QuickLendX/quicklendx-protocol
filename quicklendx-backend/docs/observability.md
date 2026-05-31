# Observability Guide

## Overview

The QuickLendX backend exposes operational metrics through Prometheus-compatible endpoints for monitoring, alerting, and dashboarding. This guide covers metric collection, scrape configuration, and integration with monitoring systems.

## Metrics Endpoints

### Prometheus Metrics (`/metrics`)

**Endpoint**: `GET /v1/metrics`  
**Authentication**: Required (Bearer token)  
**Content-Type**: `text/plain; version=0.0.4; charset=utf-8`  
**Format**: Prometheus text exposition format

Returns system health metrics in Prometheus format for direct scraping by Prometheus, Grafana, and compatible tools.

### Health Check (`/health`)

**Endpoint**: `GET /v1/health`  
**Authentication**: Not required  
**Content-Type**: `application/json`

Returns basic health status without requiring authentication. Useful for load balancer health checks.

## Available Metrics

All metrics follow the `qlx_` prefix convention for QuickLendX namespace:

| Metric | Type | Description |
|--------|------|-------------|
| `qlx_ingest_lag_ledgers` | Gauge | Current ledger indexing lag (ledgers behind latest) |
| `qlx_webhook_queue_depth` | Gauge | Current number of webhooks pending in queue |
| `qlx_webhook_overflow_total` | Counter | Total webhook queue overflows since startup |
| `qlx_rpc_circuit_state` | Gauge | RPC circuit breaker state: 0=closed, 1=open, 2=half-open |
| `qlx_invariant_violations_total` | Counter | Total invariant violations detected |

## Example Prometheus Configuration

Add the following job to your `prometheus.yml`:

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'quicklendx-backend'
    scheme: http
    static_configs:
      - targets: ['localhost:3000']
    bearer_token: 'your-api-key-here'
    metrics_path: '/v1/metrics'
    scrape_interval: 30s
    scrape_timeout: 10s
```

## Example Metrics Output

```
# HELP qlx_ingest_lag_ledgers Current ingest lag in ledgers
# TYPE qlx_ingest_lag_ledgers gauge
qlx_ingest_lag_ledgers 5

# HELP qlx_webhook_queue_depth Current webhook queue depth
# TYPE qlx_webhook_queue_depth gauge
qlx_webhook_queue_depth 42

# HELP qlx_webhook_overflow_total Total webhook queue overflows
# TYPE qlx_webhook_overflow_total counter
qlx_webhook_overflow_total 3

# HELP qlx_rpc_circuit_state RPC circuit breaker state (0=closed, 1=open, 2=half-open)
# TYPE qlx_rpc_circuit_state gauge
qlx_rpc_circuit_state 0

# HELP qlx_invariant_violations_total Total invariant violations detected
# TYPE qlx_invariant_violations_total counter
qlx_invariant_violations_total 0
```

## Alert Rules

Example alert rules for Prometheus/AlertManager:

```yaml
groups:
  - name: quicklendx
    rules:
      - alert: HighIngestLag
        expr: qlx_ingest_lag_ledgers > 100
        for: 5m
        annotations:
          summary: "High ingest lag: {{ $value }} ledgers"

      - alert: WebhookQueueBackup
        expr: qlx_webhook_queue_depth > 1000
        for: 2m
        annotations:
          summary: "Webhook queue depth exceeds 1000: {{ $value }}"

      - alert: RPCCircuitBreakerOpen
        expr: qlx_rpc_circuit_state == 1
        for: 1m
        annotations:
          summary: "RPC circuit breaker is open"

      - alert: InvariantViolations
        expr: increase(qlx_invariant_violations_total[5m]) > 0
        for: 1m
        annotations:
          summary: "Invariant violations detected: {{ $value }} in past 5m"
```

## Grafana Integration

### Data Source Setup

1. Add Prometheus as a data source in Grafana
2. Configure URL: `http://localhost:9090` (your Prometheus instance)
3. Test the connection

### Dashboard Queries

Create panels using these PromQL queries:

**Ingest Lag:**
```promql
qlx_ingest_lag_ledgers
```

**Webhook Queue Depth (with trend):**
```promql
qlx_webhook_queue_depth
```

**Webhook Overflow Rate:**
```promql
rate(qlx_webhook_overflow_total[5m])
```

**RPC Circuit State:**
```promql
qlx_rpc_circuit_state
```

**Invariant Violation Rate:**
```promql
rate(qlx_invariant_violations_total[5m])
```

## Implementation Details

### Metric Aggregation

Metrics are aggregated from multiple backend services:

- **lagMonitor**: Tracks Soroban ledger indexing lag
- **webhookQueueService**: Monitors webhook queue depth and overflow events
- **invariantService**: Counts invariant violations
- **rpcClient**: Tracks RPC circuit breaker state

Each service implements error handling and graceful degradation. If a service fails to report metrics, the `/metrics` endpoint will continue to serve the last known values.

### Security Considerations

1. **Authentication Required**: The `/metrics` endpoint requires a valid API key via Bearer token
2. **No PII Leakage**: Metrics do not include sensitive data (user IDs, amounts, etc.)
3. **Label Escaping**: All label values are properly escaped per Prometheus format spec
4. **Rate Limiting**: Consider applying rate limits to `/metrics` to prevent abuse

### Performance Impact

- Metric aggregation runs synchronously during request handling
- Typical response time: <50ms
- No persistent storage required; metrics kept in memory
- Suitable for high-frequency scraping (15-30s intervals)

## Troubleshooting

### Metrics Endpoint Returns 401

- Verify Bearer token is provided in Authorization header
- Format: `Authorization: Bearer <api-key>`
- Ensure API key is valid

### Missing or Zero Metrics

- Check that backend services are running and healthy
- Review service logs for aggregation errors
- Metrics default to 0 if services fail gracefully

### Invalid Prometheus Format

- Ensure label values contain no unescaped special characters
- Check that metric names match `[a-zA-Z_:][a-zA-Z0-9_:]*` pattern
- Review Prometheus format specification

## Related Documentation

- [Prometheus Format Spec](https://prometheus.io/docs/instrumenting/exposition_formats/)
- [PromQL Guide](https://prometheus.io/docs/prometheus/latest/querying/basics/)
- [Grafana Dashboard Creation](https://grafana.com/docs/grafana/latest/dashboards/)
