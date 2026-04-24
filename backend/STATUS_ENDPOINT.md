# Backend Status Endpoint

A lightweight status endpoint has been implemented to report the health and indexing state of the QuickLendX backend.

## Endpoint Details

- **URL**: `/api/status`
- **Method**: `GET`
- **Caching**: `public, max-age=30` (Safe for CDN and browser caching)

## Response Format

```json
{
  "status": "operational",
  "maintenance": false,
  "degraded": false,
  "index_lag": 5,
  "last_ledger": 100005,
  "timestamp": "2026-04-23T12:00:00.000Z",
  "version": "1.0.0"
}
```

### Fields

- `status`: Overall health signal (`operational`, `degraded`, or `maintenance`).
- `maintenance`: Boolean flag indicating if the protocol is intentionally paused or in maintenance mode.
- `degraded`: Boolean flag indicating if system performance is sub-optimal (e.g., high indexing lag).
- `index_lag`: Number of ledgers the off-chain indexer is behind the blockchain.
- `last_ledger`: The height of the last ledger successfully processed by the backend.
- `timestamp`: Current server time.
- `version`: Backend software version.

## Usage in Frontend

Clients should poll this endpoint periodically (e.g., every 60 seconds) to:
1. Show maintenance banners if `maintenance` is true.
2. Show "Data may be delayed" banners if `degraded` is true or `index_lag` is high.
3. Adjust retry backoff strategies if the system is under stress.

## Implementation Notes

- **Location**: `backend/src/services/statusService.ts`
- **Security**: The endpoint is public but exposes no sensitive internal state (e.g., database connection strings or internal IP addresses).
- **Test Coverage**: 100% (Unit and Integration tests).
