# Indexer Reliability: Backpressure Strategy

## Overview

To prevent database exhaustion and infrastructure collapse during load spikes, the indexer implements an adaptive backpressure strategy.

## Metrics Monitored

1. **DB Latency**: Round-trip time for write operations.
2. **Queue Depth**: Number of events waiting in the ingestion buffer.

## Ingestion Strategies

| State         | Threshold (Latency) | Threshold (Queue) | Action                                      |
| :------------ | :------------------ | :---------------- | :------------------------------------------ |
| **Normal**    | < 200ms             | < 5,000           | Unrestricted processing speed.              |
| **Throttled** | >= 200ms            | >= 5,000          | Injects 500ms delay between batches.        |
| **Paused**    | >= 1,000ms          | >= 10,000         | Stops ingestion; re-checks health every 5s. |

## Implementation Details

- **No Silent Data Loss**: The indexer never drops events. It merely pauses the ingestion loop, allowing the database to catch up.
- **Thread Safety**: Uses asynchronous `RwLock` to ensure health updates and ingestion loops don't block each other.
- **Fail-Safe**: The system defaults to `Normal` but reacts within a single batch cycle to latency spikes.

## Monitoring Endpoints

Health status is exposed via the API:
`GET /api/v1/health/indexer`

Example Response:

```json
{
  "strategy": "Throttled",
  "db_latency_ms": 345,
  "queue_depth": 1200
}
```

## Recovery

Once metrics fall below degraded thresholds for a complete batch cycle, the controller automatically promotes the strategy back toward `Normal`.

## Contract Health Snapshot

Clients that need a single on-ledger answer to "can the protocol accept writes right now?"
should call the Soroban entrypoint `get_health_status()` instead of polling
`is_paused`, `is_maintenance_mode`, and related getters independently.

The returned `HealthStatus` struct includes:

- `is_paused`, `is_maintenance`, `maintenance_reason`
- `backpressure_active` (contract-side load shedding)
- `index_lag_seconds` and `data_is_stale` (freshness advisory fields)
- `writes_allowed` — derived `true` only when pause, maintenance, and backpressure
  all permit writes

This read is pause-exempt, requires no authentication, and performs no storage
writes. It gives UI, indexer, and monitoring clients one consistent ledger view
for gating user actions or showing degraded-state banners.

---

## On-Chain Incident Mode (Protocol Runbook)

During a **security incident**, operators must not call `pause` and
`set_maintenance_mode` as separate transactions — that leaves a window where the
protocol is half-frozen and half-live.

Use the coordinated contract entrypoints instead:

| Action | Entrypoint | Effect |
| --- | --- | --- |
| Engage incident response | `enter_incident_mode(admin, reason)` | Atomically sets hard pause **and** maintenance mode with an on-chain reason; returns `IncidentSnapshot` (`is_paused`, `is_maintenance`, `reason`, `timestamp`) for audit/runbook logs |
| Clear incident response | `exit_incident_mode(admin)` | Atomically clears both flags; idempotent when already normal |

**Operator checklist**

1. Call `enter_incident_mode` with a concise reason (≤ 256 bytes).
2. Record the returned `IncidentSnapshot` in the incident ticket.
3. Use read-only queries (`get_protocol_health`, `get_health_status`, `get_invoice`, etc.) while investigating.
4. When safe, call `exit_incident_mode` and confirm both flags are `false`.

**Recovery from drift:** If pause and maintenance were toggled independently and
are out of sync, call `exit_incident_mode` to reset both, or `enter_incident_mode`
to realign them and refresh the reason.

See [`docs/contracts/operations.md`](docs/contracts/operations.md) for the full
pause vs. maintenance matrix.
