# API Key System - Quick Start Guide

## 🚀 Quick Start

### 1. Create Your First API Key

```bash
curl -X POST http://localhost:3000/api/v1/keys \
  -H "Authorization: Bearer YOUR_ADMIN_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "My Service Key",
    "scopes": ["read:invoices", "write:invoices"],
    "created_by": "your-user-id"
  }'
```

**Response**:
```json
{
  "data": {
    "id": "key_abc123",
    "key": "qlx_test_xxxxxxxxxxxxxxxxxxxxxxxxxxx",
    "name": "My Service Key",
    "scopes": ["read:invoices", "write:invoices"],
    "warning": "Store this key securely. It will not be shown again."
  }
}
```

⚠️ **Important**: Save the `key` value immediately. It will never be shown again!

### 2. Use Your API Key

```bash
curl http://localhost:3000/api/v1/invoices \
  -H "Authorization: Bearer qlx_test_xxxxxxxxxxxxxxxxxxxxxxxxxxx"
```

### 3. Rotate Your Key (Every 90 Days)

```bash
curl -X POST http://localhost:3000/api/v1/keys/key_abc123/rotate \
  -H "Authorization: Bearer YOUR_ADMIN_KEY" \
  -H "Content-Type: application/json" \
  -d '{"actor": "your-user-id"}'
```

## 📋 Common Operations

### List All Keys

```bash
curl http://localhost:3000/api/v1/keys \
  -H "Authorization: Bearer YOUR_ADMIN_KEY"
```

### Get Specific Key

```bash
curl http://localhost:3000/api/v1/keys/key_abc123 \
  -H "Authorization: Bearer YOUR_ADMIN_KEY"
```

### Revoke a Key

```bash
curl -X POST http://localhost:3000/api/v1/keys/key_abc123/revoke \
  -H "Authorization: Bearer YOUR_ADMIN_KEY" \
  -H "Content-Type: application/json" \
  -d '{"actor": "your-user-id"}'
```

### Get Audit Logs

```bash
curl http://localhost:3000/api/v1/keys/key_abc123/audit-logs \
  -H "Authorization: Bearer YOUR_ADMIN_KEY"
```

### Get Available Scopes

```bash
curl http://localhost:3000/api/v1/keys/scopes
```

No authentication required for this endpoint.

## 🔐 Available Scopes

| Scope | Description |
|-------|-------------|
| `read:*` | Read access to all resources |
| `write:*` | Write access to all resources |
| `admin:*` | Full administrative access |
| `admin:keys` | Manage API keys |
| `read:users` | Read user data |
| `write:users` | Create/update users |
| `read:invoices` | Read invoice data |
| `write:invoices` | Create/update invoices |
| `read:bids` | Read bid data |
| `write:bids` | Create/update bids |
| `read:settlements` | Read settlement data |
| `write:settlements` | Create/update settlements |

## 🛡️ Security Best Practices

### ✅ DO

- Store keys in environment variables or secret managers
- Use the minimum required scopes
- Rotate keys every 90 days
- Revoke keys immediately when compromised
- Monitor audit logs regularly
- Use different keys for different services

### ❌ DON'T

- Commit keys to version control
- Share keys between services
- Use wildcard scopes unless necessary
- Log keys in plaintext
- Store keys in client-side code
- Ignore failed authentication alerts

## 🔧 Integration Examples

### Node.js / Express

```javascript
const axios = require('axios');

const apiKey = process.env.API_KEY;

async function getInvoices() {
  const response = await axios.get('http://localhost:3000/api/v1/invoices', {
    headers: {
      'Authorization': `Bearer ${apiKey}`
    }
  });
  return response.data;
}
```

### Python / Requests

```python
import os
import requests

api_key = os.environ['API_KEY']

def get_invoices():
    response = requests.get(
        'http://localhost:3000/api/v1/invoices',
        headers={'Authorization': f'Bearer {api_key}'}
    )
    return response.json()
```

### cURL

```bash
export API_KEY="qlx_test_xxxxxxxxxxxxxxxxxxxxxxxxxxx"

curl http://localhost:3000/api/v1/invoices \
  -H "Authorization: Bearer $API_KEY"
```

## 🐛 Troubleshooting

### 401 Unauthorized

**Problem**: `Invalid API key`

**Solutions**:
- Verify key format: `qlx_<env>_<random>`
- Check if key has been revoked
- Verify key hasn't expired
- Ensure you're using the correct environment key

### 403 Forbidden

**Problem**: `Insufficient permissions`

**Solutions**:
- Check required scopes for the endpoint
- Verify your key has the necessary scopes
- Request a new key with additional scopes

### Key Not Working After Rotation

**Problem**: Old key still being used

**Solutions**:
- Update all services with the new key
- The old key is immediately invalidated
- Check environment variables are updated

## 📊 Monitoring

### Key Metrics to Track

1. **Failed Authentication Attempts**
   - Alert threshold: > 10 per minute

2. **Key Age**
   - Rotate keys older than 90 days

3. **Unusual IP Addresses**
   - Alert on unexpected locations

4. **Revoked Key Usage**
   - Alert immediately

## 🧪 Testing

### Run Tests

```bash
cd backend
npm test -- --testPathPattern=api-key.test.ts
```

### Run with Coverage

```bash
npm test -- --testPathPattern=api-key.test.ts --coverage
```

## 📚 Additional Resources

- **Full Documentation**: `docs/auth.md`
- **Security Checklist**: `SECURITY_CHECKLIST.md`
- **Implementation Details**: `API_KEY_IMPLEMENTATION_SUMMARY.md`

## 🆘 Support

- **Documentation**: https://docs.quicklendx.com
- **Support Email**: support@quicklendx.com
- **Security Issues**: security@quicklendx.com

## ⚡ Quick Reference

### Key Format
```
qlx_<environment>_<random>
```

### Authentication Header
```
Authorization: Bearer qlx_test_xxxxxxxxxxxxxxxxxxxxxxxxxxx
```

### Status Codes
- `200` - Success
- `201` - Created
- `400` - Bad Request (validation error)
- `401` - Unauthorized (invalid/missing key)
- `403` - Forbidden (insufficient scopes)
- `404` - Not Found
- `500` - Server Error

### Key Lifecycle
1. **Create** → Store securely
2. **Use** → Monitor usage
3. **Rotate** → Every 90 days
4. **Revoke** → When no longer needed

---

**Need Help?** Check the full documentation in `docs/auth.md`
