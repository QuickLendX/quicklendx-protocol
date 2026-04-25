# Index Reconciliation Worker

## Overview
The Reconciliation Worker is a periodic job designed to detect "drift" between the indexed database state and the canonical on-chain truth (Soroban smart contracts). It ensures data integrity and triggers repairs via a bounded backfill workflow.

## Architecture

### Components
1. **Reconciliation Service**: Core logic for comparing records and identifying mismatches.
2. **Mock Data Providers**: (Current implementation) Simulates indexer and on-chain state for testing drift detection.
3. **Drift Reports**: Persistent records of identified mismatches, including status mismatches and missing rows.
4. **Bounded Backfill**: A mechanism to repair records in small, controlled batches to avoid overwhelming the system or on-chain nodes.

### Drift Detection Logic
The worker performs the following checks:
- **Missing Rows**: Records that exist on-chain but are missing from the indexer.
- **Status Mismatch**: Records where the indexer's status (e.g., `Pending`) differs from the on-chain status (e.g., `Verified`).
- **Data Mismatch**: (Optional) Comparisons of amounts, dates, or metadata.

## API Endpoints

- `GET /api/v1/reconciliation/reports`: List all historical drift reports.
- `POST /api/v1/reconciliation/run`: Manually trigger a reconciliation scan.
- `POST /api/v1/reconciliation/backfill`: Trigger a bounded backfill based on the latest report.

## Security Considerations
- **Rate Limiting**: On-chain queries are sampled and rate-limited to prevent node saturation.
- **Bounded Backfill**: Backfill operations are capped (default: 10 items per batch) to prevent runaway jobs.
- **Authentication**: Reconciliation endpoints should be restricted to admin roles in production environments.

## Maintenance
If drift exceeds a certain threshold (e.g., 5% of total records), the system should alert administrators for manual investigation.
