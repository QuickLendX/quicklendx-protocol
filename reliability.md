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
