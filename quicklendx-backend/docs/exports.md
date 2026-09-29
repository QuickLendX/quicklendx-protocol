# Data Export Documentation

## Overview

The QuickLendX backend provides secure, audited data export capabilities with configurable limits and integrity verification. This document describes the export system, usage, limits, and security considerations.

## Features

- **Query Parameter Validation**: Strict validation of date ranges, formats, and row limits
- **Audit Logging**: Every export is recorded in the audit trail with user context and metadata
- **Streaming with Back-Pressure**: Large exports are streamed to avoid memory exhaustion
- **Size Limits**: Configurable per-request byte and row caps to prevent abuse
- **Integrity Digest**: SHA256 checksums for export verification

## Supported Export Types

### Data Types
- `invoices`: Invoice records
- `bids`: Bid records  
- `settlements`: Settlement records
- `disputes`: Dispute records
- `audit`: Audit log records (admin-only)

### Export Formats
- `ndjson`: Newline-delimited JSON (default)
- `json`: Standard JSON array
- `csv`: Comma-separated values with proper escaping

## Export Limits

### Default Configuration

```typescript
{
  maxRowsPerRequest: 10000,        // Maximum rows per export
  maxBytesPerRequest: 50MB,        // Maximum data size
  allowedFormats: ['ndjson', 'json', 'csv'],
  chunkSize: 1000                  // Rows buffered before streaming
}
```

### Date Range Constraint
- Maximum date range: **90 days**
- Prevents excessively large exports
- Date format: ISO 8601 (YYYY-MM-DD)

## API Endpoints

### Export Invoices
```
GET /api/v1/exports/invoices
```

Query Parameters:
- `startDate` (optional): ISO 8601 date string
- `endDate` (optional): ISO 8601 date string
- `format` (optional): `ndjson`, `json`, or `csv` (default: `ndjson`)
- `limit` (optional): number of rows (default: 10000, max: 10000)

Example:
```bash
curl "http://localhost:3000/api/v1/exports/invoices?startDate=2024-01-01&endDate=2024-01-31&format=csv&limit=5000" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -o invoices.csv
```

### Export Bids
```
GET /api/v1/exports/bids
```

Same query parameters as invoices endpoint.

### Export Settlements
```
GET /api/v1/exports/settlements
```

Same query parameters as invoices endpoint.

### Export Disputes
```
GET /api/v1/exports/disputes
```

Same query parameters as invoices endpoint.

### Export Audit Log (Admin)
```
GET /admin/exports/audit
```

Query Parameters: Same as above.

**Note**: Requires admin role. Records its own audit entry.

### Export Statistics (Admin)
```
GET /admin/exports/stats
```

Returns summary statistics of all exports for the authenticated user or all users (if admin).

## Response Format

### Success Response (HTTP 200)

```
HTTP/1.1 200 OK
Content-Type: application/x-ndjson
Content-Disposition: attachment; filename="export-invoices-1705276800000.ndjson"
X-Export-Id: 550e8400-e29b-41d4-a716-446655440000
X-Content-Digest: sha256

{"id":"INV-0","amount":5000,"currency":"USD","status":"paid","createdAt":"2024-01-15T10:30:00Z"}
{"id":"INV-1","amount":7500,"currency":"USD","status":"paid","createdAt":"2024-01-16T14:22:00Z"}
...
# Content-Digest: sha256=abc123def456...
```

**Response Headers:**
- `Content-Type`: Format-specific MIME type
- `Content-Disposition`: Attachment with filename
- `X-Export-Id`: Unique export request ID
- `X-Content-Digest`: Integrity algorithm (sha256)

### Error Response (HTTP 400)

```json
{
  "error": "Invalid export parameters",
  "details": [
    "Date range cannot exceed 90 days",
    "Format not allowed: xml. Allowed: ndjson, json, csv"
  ]
}
```

## Validation Rules

### Date Validation
- ✓ ISO 8601 format required (YYYY-MM-DD)
- ✓ `startDate` must be before `endDate`
- ✓ Date range cannot exceed 90 days
- ✗ Dates cannot be null individually (both or neither)

### Format Validation
- ✓ Must be one of: `ndjson`, `json`, `csv`
- ✓ Case-sensitive

### Limit Validation
- ✓ Must be between 1 and 10000
- ✓ Cannot exceed `maxRowsPerRequest` config
- ✓ Default: 10000

## Integrity Verification

### Checksum Verification

Each export includes a SHA256 checksum for integrity verification:

```bash
# Download export
curl "http://localhost:3000/api/v1/exports/invoices" -o export.ndjson

# Extract checksum from file
CHECKSUM=$(tail -1 export.ndjson | grep -oP 'Content-Digest: sha256=\K[a-f0-9]{64}')

# Verify integrity (exclude checksum line)
head -n -1 export.ndjson | sha256sum

# Compare with reported checksum
```

### Using in Applications

```typescript
import crypto from 'crypto';

async function verifyExportIntegrity(
  exportData: string,
  reportedChecksum: string
): Promise<boolean> {
  const calculated = crypto
    .createHash('sha256')
    .update(exportData)
    .digest('hex');
  
  return calculated === reportedChecksum;
}
```

## Audit Trail

### Recorded Information

Every export is audited with:
- **User ID**: Who performed the export
- **Data Type**: What was exported (invoices, bids, etc.)
- **Format**: Export format (ndjson, json, csv)
- **Row Count**: Number of records exported
- **Bytes Transferred**: Total data size
- **Date Range**: If applicable
- **Timestamp**: When export occurred
- **Status**: completed, failed, or in-progress
- **Checksum**: Integrity digest for verification

### Audit Entry Example

```typescript
{
  id: "550e8400-e29b-41d4-a716-446655440000",
  userId: "user-123",
  dataType: "invoices",
  format: "ndjson",
  rowCount: 5000,
  bytesTransferred: 2500000,
  startDate: "2024-01-01T00:00:00Z",
  endDate: "2024-01-31T23:59:59Z",
  checksum: "abc123def456...",
  status: "completed",
  createdAt: "2024-01-15T10:30:00Z",
  completedAt: "2024-01-15T10:35:00Z"
}
```

### Viewing Audit History

```bash
# User's export history
GET /api/v1/exports/history

# Admin audit export
GET /admin/exports/audit
```

## Security Considerations

### Rate Limiting
- Recommended: 10 exports per user per hour
- Configure in API gateway or middleware

### Access Control
- All exports require authentication
- Admin exports require admin role
- Audit logs require admin role to view

### Data Classification
- Mark exports containing PII for audit trails
- Implement row-level security per organization
- Consider encryption for exports at rest

### Compliance
- GDPR: Exports include user context for data requests
- SOC2: All exports are audited and traceable
- PCI: Payment data exports require additional approval

## Configuration

### Environment Variables

```bash
# Export limits
EXPORT_MAX_ROWS=10000
EXPORT_MAX_BYTES=52428800  # 50 MB in bytes
EXPORT_MAX_DATE_RANGE=90   # days

# Chunk size for streaming
EXPORT_CHUNK_SIZE=1000     # rows

# Audit retention
AUDIT_LOG_RETENTION_DAYS=365
```

### Runtime Configuration

```typescript
import { ExportService } from './services/exportService';

const service = new ExportService({
  maxRowsPerRequest: 5000,
  maxBytesPerRequest: 10 * 1024 * 1024, // 10 MB
  allowedFormats: ['ndjson', 'json'],
  chunkSize: 500
});
```

## Common Use Cases

### Case 1: Monthly Invoice Reconciliation
```bash
curl "http://localhost:3000/api/v1/exports/invoices?startDate=2024-01-01&endDate=2024-01-31&format=csv" \
  -H "Authorization: Bearer TOKEN" \
  -o invoices-2024-01.csv
```

### Case 2: Dispute Analysis (JSON Format)
```bash
curl "http://localhost:3000/api/v1/exports/disputes?startDate=2024-01-01&endDate=2024-03-31&format=json&limit=5000" \
  -H "Authorization: Bearer TOKEN" | jq '.[] | select(.status == "pending")'
```

### Case 3: Admin Audit Review
```bash
curl "http://localhost:3000/admin/exports/audit?format=csv" \
  -H "Authorization: Bearer ADMIN_TOKEN" \
  -o audit-trail.csv
```

## Troubleshooting

### Export Returns 400 Bad Request
Check:
- Date format is ISO 8601 (YYYY-MM-DD)
- Format is one of: ndjson, json, csv
- Limit is between 1 and 10000
- Date range does not exceed 90 days
- startDate is before endDate

### Export Times Out
- Reduce the date range or row limit
- Use smaller chunks if downloading locally
- Check network connectivity

### Checksum Mismatch
- Ensure file was not modified after download
- Re-download the export
- Contact support if persistent

## Performance Tips

### Optimal Export Size
- **Small exports**: < 1MB, use JSON format
- **Medium exports**: 1-10MB, use NDJSON (streaming friendly)
- **Large exports**: 10-50MB, use NDJSON with smaller date ranges

### Streaming Best Practices
```typescript
// Good: Process as stream
const response = await fetch('/api/v1/exports/invoices');
const reader = response.body.getReader();

while (true) {
  const { done, value } = await reader.read();
  if (done) break;
  processChunk(value);
}

// Avoid: Buffering entire response
const data = await response.text(); // Bad for large exports
```

## Future Enhancements

- [ ] Scheduled exports (cron)
- [ ] Export templates with saved parameters
- [ ] Email delivery of exports
- [ ] Compressed export formats (gzip, brotli)
- [ ] Real-time export webhooks
- [ ] Row-level security filtering per export

## Support

For issues or questions about exports:
1. Check this documentation
2. Review audit logs for error details
3. Contact the QuickLendX support team
4. Open an issue on GitHub with export request ID
