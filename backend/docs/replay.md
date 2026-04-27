# Deterministic Replay Mode

## Overview

The deterministic replay mode enables rebuilding derived tables from stored raw events, producing identical results and enabling controlled schema migrations and backfills. This system ensures data consistency, provides auditability, and supports safe schema evolution.

## Architecture

### Core Components

1. **Raw Event Store** (`RawEventStore`)
   - Stores validated and sanitized blockchain events
   - Provides cursor-based iteration for deterministic processing
   - Supports both in-memory and file-based implementations
   - Maintains replay cursor for checkpointing

2. **Replay Pipeline** (`ReplayService`)
   - Orchestrates deterministic event processing
   - Manages replay runs with pause/resume capabilities
   - Ensures idempotency and cursor consistency
   - Provides comprehensive audit logging

3. **Derived Table Store** (`DerivedTableStore`)
   - Manages derived tables with ACID transactions
   - Supports state hashing for verification
   - Provides rollback capabilities for failed replays
   - Handles both in-memory and persistent storage

4. **Event Validator** (`EventValidator`)
   - Validates event structure and content
   - Sanitizes potentially malicious data
   - Enforces size limits and content policies
   - Prevents injection attacks

## Key Features

### Deterministic Processing
- Events processed in strict ledger order
- Same input always produces same output
- State hash verification for consistency
- Batch processing with atomic commits

### Idempotency Guarantees
- Cursor-based replay prevents duplicate processing
- Idempotency keys prevent duplicate runs
- Transaction rollback on failures
- Safe retry mechanisms

### Security Controls
- Input validation and sanitization
- Size limits and content filtering
- Compliance hold preservation
- Audit trail for all operations

### Operational Controls
- Pause/resume functionality
- Progress monitoring and statistics
- Configurable batch sizes
- Dry-run mode for testing

## API Endpoints

### Start Replay
```
POST /api/admin/replay
Authorization: Admin required

{
  "fromLedger": 1000,
  "toLedger": 2000,
  "dryRun": false,
  "batchSize": 100,
  "forceRebuild": true,
  "idempotencyKey": "optional-unique-key"
}
```

### List Replay Runs
```
GET /api/admin/replay/runs
Authorization: Admin required

Response:
{
  "runs": [
    {
      "id": "rp_1234567890_abcdef",
      "fromLedger": 1000,
      "toLedger": 2000,
      "status": "completed",
      "processedEvents": 150,
      "actor": "admin_user",
      "createdAt": "2024-01-01T00:00:00Z",
      "completedAt": "2024-01-01T00:05:00Z"
    }
  ]
}
```

### Get Replay Details
```
GET /api/admin/replay/{runId}
Authorization: Admin required
```

### Get Replay Statistics
```
GET /api/admin/replay/{runId}/stats
Authorization: Admin required

Response:
{
  "stats": {
    "totalEvents": 1000,
    "processedEvents": 750,
    "failedEvents": 0,
    "skippedEvents": 0,
    "currentLedger": 1750,
    "estimatedCompletion": "2024-01-01T00:10:00Z"
  }
}
```

### Pause Replay
```
POST /api/admin/replay/pause
Authorization: Admin required

{
  "runId": "rp_1234567890_abcdef"
}
```

### Resume Replay
```
POST /api/admin/replay/resume
Authorization: Admin required

{
  "runId": "rp_1234567890_abcdef"
}
```

## Usage Patterns

### Full Rebuild
```javascript
const rebuildRequest = {
  fromLedger: 0,
  toLedger: await getCurrentLedger(),
  forceRebuild: true,
  batchSize: 500
};

const result = await replayService.startReplay(rebuildRequest, "admin");
```

### Incremental Backfill
```javascript
const backfillRequest = {
  fromLedger: await getReplayCursor(),
  toLedger: await getCurrentLedger(),
  forceRebuild: false,
  batchSize: 100
};

const result = await replayService.startReplay(backfillRequest, "admin");
```

### Schema Migration
```javascript
// 1. Pause existing processing
await replayService.pauseRun(currentRunId, "admin");

// 2. Update derived table schema
await updateDerivedTableSchema();

// 3. Force rebuild with new schema
const migrationRequest = {
  fromLedger: 0,
  toLedger: await getCurrentLedger(),
  forceRebuild: true,
  batchSize: 200
};

const result = await replayService.startReplay(migrationRequest, "admin");
```

## Configuration

### Environment Variables
- `REPLAY_MAX_LEDGER_RANGE`: Maximum ledger range per replay (default: 100,000)
- `REPLAY_MAX_BATCH_SIZE`: Maximum events per batch (default: 1,000)
- `REPLAY_AUDIT_LOG_PATH`: Audit log file location

### Security Limits
- Maximum payload size: 1MB per event
- Maximum ledger range: 100,000 ledgers
- Maximum batch size: 1,000 events
- Allowed event types: Whitelist only

## Testing

### Determinism Verification
```bash
# Run replay twice and compare state hashes
npm test -- replay.determinism.test.ts
```

### Idempotency Testing
```bash
# Test duplicate request handling
npm test -- replay.idempotency.test.ts
```

### Security Validation
```bash
# Test malicious content rejection
npm test -- replay.security.test.ts
```

## Monitoring

### Key Metrics
- Replay success rate
- Average processing time per event
- State hash consistency
- Error rates by event type
- Cursor progression speed

### Alerting
- Replay failures
- State hash mismatches
- Long-running replays
- High error rates

## Best Practices

### Performance
- Use appropriate batch sizes (100-1000 events)
- Monitor memory usage during large replays
- Implement backpressure for high-volume scenarios
- Use dry-run mode for estimation

### Reliability
- Always use idempotency keys
- Implement proper error handling
- Monitor replay cursor progression
- Validate state hashes after completion

### Security
- Validate all input parameters
- Use compliance holds for audit trails
- Sanitize event content before storage
- Implement proper access controls

## Troubleshooting

### Common Issues

**Replay Stuck Processing**
- Check for infinite loops in event processing
- Verify cursor progression
- Monitor memory usage
- Check transaction locks

**State Hash Mismatches**
- Verify event order consistency
- Check for non-deterministic processing
- Validate derived table logic
- Review transaction boundaries

**Performance Issues**
- Reduce batch size
- Check I/O bottlenecks
- Optimize event processing logic
- Consider parallel processing

### Debug Commands
```bash
# Check replay cursor
cat .data/replay-cursor.json

# View audit log
tail -f .data/replay-audit-log.jsonl

# Verify raw events
head -n 10 .data/raw-events/events.jsonl
```

## Migration Guide

### From Existing Indexer
1. Export current derived table state
2. Backfill raw events from blockchain
3. Run deterministic replay to rebuild
4. Verify state hash matches
5. Switch to replay-based processing

### Schema Evolution
1. Add new fields to derived tables
2. Update event processors for new logic
3. Run force rebuild to populate new fields
4. Validate backward compatibility

## Security Considerations

### Input Validation
- All events validated before storage
- Malicious content filtered out
- Size limits enforced
- Type safety guaranteed

### Access Control
- Admin-only endpoints
- Actor tracking in audit logs
- Idempotency key enforcement
- Rate limiting on replay operations

### Data Protection
- Compliance holds preserved
- Audit trails maintained
- Secure cursor storage
- Encrypted sensitive data

This replay system provides a robust foundation for deterministic event processing, enabling reliable data reconstruction and safe schema evolution while maintaining strong security guarantees.
