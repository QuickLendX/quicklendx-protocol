# Service Authentication

## Overview

The QuickLendX API uses a secure API key system for service-to-service authentication. This system is designed for internal services and backend integrations, separate from user-facing JWT authentication.

### When to Use API Keys vs. User JWTs

- **API Keys**: Use for service-to-service communication, backend integrations, automated processes, and administrative operations
- **User JWTs**: Use for user-facing authentication in web and mobile applications

## Creating an API Key

### Endpoint

```
POST /api/v1/keys
```

### Authentication Required

This endpoint requires an API key with the `admin:keys` scope.

### Request Body

```json
{
  "name": "Production Service Key",
  "scopes": ["read:users", "write:jobs"],
  "created_by": "admin-user-id",
  "expires_at": "2027-12-31T23:59:59Z"  // Optional
}
```

### Response

```json
{
  "data": {
    "id": "key_abc123",
    "key": "qlx_live_xxxxxxxxxxxxxxxxxxxxxxxxxxx",
    "name": "Production Service Key",
    "prefix": "qlx_live_xxxxx",
    "scopes": ["read:users", "write:jobs"],
    "created_at": "2026-04-29T10:00:00Z",
    "expires_at": "2027-12-31T23:59:59Z",
    "created_by": "admin-user-id",
    "warning": "Store this key securely. It will not be shown again."
  }
}
```

**Important**: The plaintext key is only returned once at creation time. Store it securely immediately.

### Example cURL Request

```bash
curl -X POST https://api.quicklendx.com/api/v1/keys \
  -H "Authorization: Bearer qlx_live_your_admin_key_here" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Production Service Key",
    "scopes": ["read:users", "write:jobs"],
    "created_by": "admin-user-id"
  }'
```

## Authenticating Requests

### Authorization Header Format

All authenticated requests must include the API key in the `Authorization` header using the Bearer scheme:

```
Authorization: Bearer qlx_live_xxxxxxxxxxxxxxxxxxxxxxxxxxx
```

### Example Authenticated Request

```bash
curl https://api.quicklendx.com/api/v1/invoices \
  -H "Authorization: Bearer qlx_live_xxxxxxxxxxxxxxxxxxxxxxxxxxx"
```

## Scope Reference

Scopes control what operations an API key can perform. Each endpoint requires specific scopes.

| Scope | Description | Affected Endpoints |
|-------|-------------|-------------------|
| `read:*` | Read access to all resources | All GET endpoints |
| `write:*` | Write access to all resources | All POST/PUT/DELETE endpoints |
| `read:users` | Read user data | GET /api/v1/users/* |
| `write:users` | Create/update users | POST/PUT /api/v1/users/* |
| `read:jobs` | Read job data | GET /api/v1/jobs/* |
| `write:jobs` | Create/update jobs | POST/PUT /api/v1/jobs/* |
| `read:invoices` | Read invoice data | GET /api/v1/invoices/* |
| `write:invoices` | Create/update invoices | POST/PUT /api/v1/invoices/* |
| `read:bids` | Read bid data | GET /api/v1/bids/* |
| `write:bids` | Create/update bids | POST/PUT /api/v1/bids/* |
| `read:settlements` | Read settlement data | GET /api/v1/settlements/* |
| `write:settlements` | Create/update settlements | POST/PUT /api/v1/settlements/* |
| `admin:*` | Full administrative access | All admin endpoints |
| `admin:keys` | Manage API keys | POST/DELETE /api/v1/keys/* |
| `service:ingest` | Data ingestion service | POST /api/v1/ingest/* |
| `service:export` | Data export service | GET /api/v1/export/* |

### Wildcard Scopes

Wildcard scopes (`*`) grant access to all operations within a category:

- `read:*` - Read access to all resources
- `write:*` - Write access to all resources
- `admin:*` - All administrative operations

### Scope Validation

- At least one scope is required when creating a key
- Invalid scopes will be rejected with a 400 error
- Keys with insufficient scopes receive a 403 Forbidden response
- Missing or invalid keys receive a 401 Unauthorized response

## Key Rotation

Regular key rotation is a security best practice. Rotation generates a new key while immediately invalidating the old one.

### When to Rotate

- **Scheduled rotation**: Every 90 days (recommended)
- **Security incident**: Immediately if compromise is suspected
- **Personnel changes**: When team members with key access leave
- **Service migration**: When moving services to new infrastructure

### Rotation Endpoint

```
POST /api/v1/keys/:id/rotate
```

### Request Body

```json
{
  "actor": "admin-user-id"
}
```

### Response

```json
{
  "data": {
    "id": "key_new456",
    "key": "qlx_live_yyyyyyyyyyyyyyyyyyyyyyyyyyyy",
    "name": "Production Service Key",
    "prefix": "qlx_live_yyyyy",
    "scopes": ["read:users", "write:jobs"],
    "created_at": "2026-04-29T11:00:00Z",
    "created_by": "admin-user-id",
    "old_key_id": "key_abc123",
    "warning": "Store this key securely. The old key has been revoked."
  }
}
```

### Example Rotation Request

```bash
curl -X POST https://api.quicklendx.com/api/v1/keys/key_abc123/rotate \
  -H "Authorization: Bearer qlx_live_your_admin_key_here" \
  -H "Content-Type: application/json" \
  -d '{
    "actor": "admin-user-id"
  }'
```

### Rotation Process

1. New key is generated with the same scopes and name
2. Old key is immediately revoked
3. New plaintext key is returned (only once)
4. Rotation event is logged in audit trail
5. Update your services with the new key

## Key Management Operations

### List All Keys

```bash
GET /api/v1/keys
```

Optional query parameters:
- `created_by`: Filter by creator
- `revoked`: Filter by revocation status (true/false)

### Get Specific Key

```bash
GET /api/v1/keys/:id
```

### Revoke a Key

```bash
POST /api/v1/keys/:id/revoke
```

Request body:
```json
{
  "actor": "admin-user-id"
}
```

### Get Audit Logs

```bash
GET /api/v1/keys/:id/audit-logs
```

Returns all events for a specific key (creation, usage, rotation, revocation).

### Get Available Scopes

```bash
GET /api/v1/keys/scopes
```

No authentication required. Returns the complete scope registry.

## Security Best Practices

### Key Storage

- **Never commit keys to version control**: Use environment variables or secret management systems
- **Use secret managers**: AWS Secrets Manager, HashiCorp Vault, Azure Key Vault, etc.
- **Encrypt at rest**: Store keys encrypted in databases or configuration files
- **Limit access**: Only authorized personnel should access keys
- **Separate environments**: Use different keys for development, staging, and production

### Key Rotation Cadence

- **Production keys**: Rotate every 90 days
- **Development keys**: Rotate every 180 days or when team members change
- **Compromised keys**: Revoke immediately and rotate

### Least-Privilege Scopes

- Grant only the minimum scopes required for each service
- Avoid using wildcard scopes (`*`) unless absolutely necessary
- Create separate keys for different services or functions
- Review and audit scope assignments regularly

### Monitoring and Auditing

- Monitor the audit log for unusual activity
- Set up alerts for:
  - Multiple failed authentication attempts
  - Key usage from unexpected IP addresses
  - Keys used outside normal business hours
  - Revoked or expired keys being used
- Review audit logs monthly

### Key Lifecycle

1. **Creation**: Generate with minimal required scopes
2. **Distribution**: Securely transfer to authorized services
3. **Usage**: Monitor via audit logs
4. **Rotation**: Regular scheduled rotation
5. **Revocation**: Immediate revocation when no longer needed

### Error Handling

The API returns clear, non-leaking error messages:

- `401 Unauthorized`: Missing, invalid, expired, or revoked key
- `403 Forbidden`: Valid key but insufficient scopes
- `400 Bad Request`: Malformed request or invalid parameters

Error messages never reveal whether a key exists in the system.

## Implementation Details

### Key Format

```
qlx_<environment>_<random>
```

- `qlx`: Key type prefix (QuickLendX)
- `environment`: `test` or `live`
- `random`: 32 bytes of cryptographically secure random data (base64url encoded)

### Security Features

- **CSPRNG**: Keys generated using `crypto.randomBytes()` (Node.js)
- **SHA-256 hashing**: Keys are hashed before storage
- **Timing-safe comparison**: Prevents timing attacks during verification
- **No plaintext storage**: Only hashes are stored in the database
- **Prefix indexing**: Fast lookup using key prefix
- **Audit logging**: All key events are logged asynchronously

### Database Schema

```typescript
interface ApiKey {
  id: string;              // Primary key
  key_hash: string;        // SHA-256 hash (never plaintext)
  prefix: string;          // First 15 chars for display/lookup
  name: string;            // Human-readable label
  scopes: string[];        // Granted permissions
  created_at: string;      // ISO 8601 timestamp
  last_used_at: string | null;  // Last usage timestamp
  expires_at: string | null;    // Optional expiration
  revoked: boolean;        // Revocation flag
  created_by: string;      // Creator reference
}
```

### Audit Log Schema

```typescript
interface AuditLog {
  id: string;
  key_id: string;
  event_type: 'created' | 'used' | 'rotated' | 'revoked';
  actor: string;
  timestamp: string;
  ip_address: string | null;
  endpoint: string | null;  // For 'used' events
  metadata: Record<string, any>;
}
```

## Troubleshooting

### Common Issues

**401 Unauthorized - Invalid API key**
- Verify the key format matches `qlx_<env>_<random>`
- Check if the key has been revoked
- Verify the key hasn't expired
- Ensure you're using the correct environment key (test vs. live)

**403 Forbidden - Insufficient scopes**
- Check the required scopes for the endpoint
- Verify your key has the necessary scopes
- Request a new key with additional scopes if needed

**Key not working after rotation**
- Ensure you're using the new key, not the old one
- The old key is immediately invalidated after rotation
- Update all services using the old key

**Rate limiting errors**
- API keys are subject to rate limiting
- Implement exponential backoff in your client
- Contact support if you need higher rate limits

## Support

For questions or issues with API key authentication:

- **Documentation**: https://docs.quicklendx.com
- **Support Email**: support@quicklendx.com
- **Security Issues**: security@quicklendx.com (for security concerns only)
