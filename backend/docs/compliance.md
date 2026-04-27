# Backend KYC Data Handling Compliance

## Overview

This document outlines the KYC (Know Your Customer) data handling policy for the QuickLendX backend. It defines how KYC-related payloads are stored, processed, and protected to ensure compliance with security best practices and regulatory requirements.

## Security Assumptions

- **Logs and backups are sensitive surfaces**: All logs containing any user data must be treated as sensitive
- **Least privilege**: Access to KYC data should be restricted to authorized personnel only
- **No accidental PII leakage**: All outputs (API responses, logs, errors) must be validated for PII before release

## Data Classification

### Sensitive Fields (Require Encryption-at-Rest)

| Field | Description | Encryption |
|-------|-------------|------------|
| `tax_id` | Tax identification number | AES-256-GCM |
| `customer_name` | Full legal name | AES-256-GCM |
| `customer_address` | Physical address | AES-256-GCM |
| `date_of_birth` | Date of birth | AES-256-GCM |
| `ssn` | Social Security Number | AES-256-GCM |
| `passport_number` | Passport ID | AES-256-GCM |
| `national_id` | National ID | AES-256-GCM |
| `phone_number` | Contact phone | AES-256-GCM |
| `email` | Email address | AES-256-GCM |
| `bank_account` | Bank account details | AES-256-GCM |
| `kyc_document` | KYC document data | AES-256-GCM |
| `kyc_data` | Raw KYC payload | AES-256-GCM |

### PII Fields (Require Redaction in Logs)

| Field | Redaction Pattern |
|-------|-------------------|
| `tax_id` | Show first 2 + "****" + last 2 |
| `customer_name` | Show first 2 + "****" + last 2 |
| `customer_address` | Show first 2 + "****" + last 2 |
| `ssn` | Show first 2 + "****" + last 2 |
| `phone_number` | Show first 2 + "****" + last 2 |
| `email` | Show first 2 + "****" + last 2 |

## Encryption Implementation

### Algorithm

- **Cipher**: AES-256-GCM
- **Key Derivation**: PBKDF2 with 100,000 iterations
- **Salt**: SHA-256 hash of application-specific salt
- **IV**: Random 16-byte IV for each encryption

### Key Management

```typescript
// Initialize with master key from environment
initializeEncryption(process.env.KYC_ENCRYPTION_KEY);
```

In production, integrate with a Key Management Service (KMS) such as:
- AWS KMS
- Google Cloud KMS
- HashiCorp Vault

## Access Logging

### Requirements

Every read access to KYC data must be logged with:

- **Timestamp**: ISO 8601 format
- **User ID**: Authenticated user identifier (hashed if not authenticated)
- **IP Address**: Client IP (from X-Forwarded-For or direct connection)
- **Resource**: Type of resource accessed
- **Resource ID**: Specific record identifier
- **Fields Accessed**: List of fields included in the response
- **Status**: Success or failure

### Log Retention

- **In-Memory**: Last 10,000 entries
- **Production**: Forward to centralized logging (ELK, CloudWatch, etc.)
- **Retention Period**: Minimum 1 year for compliance

## API Endpoints

### KYC Data Access

All endpoints that access KYC data must use the access logging middleware:

```typescript
app.use("/api/v1/kyc", kycAccessLogMiddleware("read"));
```

### Response Filtering

API responses must not include raw sensitive data. Use the redaction utilities:

```typescript
// Before sending response
const safeResponse = redactPii(kycData);
res.json(safeResponse);
```

## Testing Requirements

### Minimum Coverage: 95%

All security-related code must have comprehensive tests:

1. **Encryption Tests**
   - Encrypt/decrypt roundtrip
   - Different ciphertext for same plaintext
   - Handle edge cases (empty, unicode, long data)

2. **Redaction Tests**
   - All PII fields redacted correctly
   - Nested objects handled
   - Non-PII fields unchanged

3. **Access Log Tests**
   - Log creation
   - Filtering
   - Statistics

4. **Security Tests**
   - No PII in encrypted data
   - No PII in logs
   - Hash non-reversible

## Deployment Checklist

- [ ] Encryption key configured in environment
- [ ] KMS integration verified (production)
- [ ] Access logging enabled for all KYC endpoints
- [ ] Log forwarding configured
- [ ] Tests passing with >95% coverage
- [ ] Security review completed
- [ ] Documentation updated

## Incident Response

In case of suspected data breach:

1. **Immediate**: Revoke encryption keys
2. **Short-term**: Identify affected records via access logs
3. **Long-term**: Audit all access, notify regulators/users

## Compliance References

- GDPR Article 32: Security of processing
- CCPA: Data minimization principles
- SOC 2 Type II: Access controls