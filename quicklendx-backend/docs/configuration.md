# Configuration Management

## Overview

The QuickLendX backend uses a production-grade configuration system with strict validation, fail-fast behavior, and automatic secret redaction. The system ensures the application cannot start with invalid or missing environment variables.

## Features

- **Strict Type Validation**: Uses Zod schema validation to enforce types and constraints
- **Fail-Fast Behavior**: Application terminates immediately if configuration is invalid
- **Profile Management**: Supports development, test, and production environments with profile-specific rules
- **Secret Redaction**: Automatically masks sensitive values in logs and error messages
- **Comprehensive Error Messages**: Clear, actionable errors without exposing sensitive data

## Environment Profiles

### Development (default)
- Relaxed validation rules
- Shorter minimum lengths for secrets (32 characters)
- Allows any database type
- Verbose logging enabled

### Test
- Similar to development
- Minimal console output
- Optimized for CI/CD pipelines

### Production
- Stricter validation rules
- Longer minimum lengths for secrets (64 characters)
- Requires PostgreSQL database
- Enhanced security checks

## Configuration Variables

| Variable | Type | Required | Default | Description | Production Rules |
|----------|------|----------|---------|-------------|------------------|
| `NODE_ENV` | enum | No | `development` | Environment profile: development, test, production | - |
| `PORT` | integer | No | `3000` | Server port (1-65535) | - |
| `LOG_LEVEL` | enum | No | `info` | Log level: debug, info, warn, error | - |
| `DATABASE_URL` | URL | Yes | - | Database connection string | Must be PostgreSQL |
| `DATABASE_POOL_SIZE` | integer | No | `10` | Connection pool size (1-100) | - |
| `JWT_SECRET` | string | Yes | - | JWT signing secret | Min 64 chars (vs 32 in dev) |
| `API_KEY` | string | Yes | - | API authentication key | Min 32 chars (vs 16 in dev) |
| `ENCRYPTION_KEY` | string | Yes | - | Data encryption key | Min 64 chars (vs 32 in dev) |
| `STELLAR_NETWORK_URL` | URL | Yes | - | Stellar Horizon API URL | - |
| `STELLAR_NETWORK_PASSPHRASE` | string | Yes | - | Stellar network passphrase | - |
| `ENABLE_RATE_LIMITING` | boolean | No | `true` | Enable API rate limiting | - |
| `MAX_REQUESTS_PER_MINUTE` | integer | No | `100` | Rate limit threshold (1-10000) | - |
| `SENTRY_DSN` | URL | No | - | Sentry error tracking DSN | - |

## Usage

### Loading Configuration

```typescript
import { getConfig } from './config';

// Get configuration (loads automatically on first call)
const config = getConfig();

console.log(`Server starting on port ${config.PORT}`);
console.log(`Environment: ${config.NODE_ENV}`);
```

### Safe Logging

```typescript
import { getSafeConfig, formatSafeConfig } from './config';

const config = getConfig();

// Get safe version with secrets redacted
const safeConfig = getSafeConfig(config);
console.log('Config:', safeConfig);
// Output: { PORT: 3000, JWT_SECRET: '[REDACTED]', ... }

// Format as JSON string
console.log(formatSafeConfig(config));
```

### Resetting Configuration (Testing)

```typescript
import { resetConfig, getConfig } from './config';

// Reset singleton instance
resetConfig();

// Next call will reload configuration
const newConfig = getConfig();
```

## Environment Files

The system loads environment variables from multiple sources in order:

1. `.env` - Base configuration
2. `.env.{profile}` - Profile-specific (e.g., `.env.production`)
3. `.env.{profile}.local` - Local overrides (gitignored)
4. `process.env` - System environment variables

Later sources override earlier ones.

### Example .env File

```bash
# Application
NODE_ENV=development
PORT=3000
LOG_LEVEL=debug

# Database
DATABASE_URL=postgresql://localhost:5432/quicklendx_dev
DATABASE_POOL_SIZE=10

# Security (NEVER commit real secrets!)
JWT_SECRET=development-jwt-secret-minimum-32-characters-long-for-security
API_KEY=dev-api-key-1234
ENCRYPTION_KEY=development-encryption-key-minimum-32-characters-required

# Stellar Network
STELLAR_NETWORK_URL=https://horizon-testnet.stellar.org
STELLAR_NETWORK_PASSPHRASE=Test SDF Network ; September 2015

# Features
ENABLE_RATE_LIMITING=true
MAX_REQUESTS_PER_MINUTE=100

# Monitoring (optional)
# SENTRY_DSN=https://your-sentry-dsn@sentry.io/project
```

### Example .env.production File

```bash
NODE_ENV=production

# Use longer secrets in production
JWT_SECRET=${JWT_SECRET}  # From environment, min 64 chars
API_KEY=${API_KEY}        # From environment, min 32 chars
ENCRYPTION_KEY=${ENCRYPTION_KEY}  # From environment, min 64 chars

# Production database
DATABASE_URL=${DATABASE_URL}  # Must be PostgreSQL
DATABASE_POOL_SIZE=50

# Production Stellar
STELLAR_NETWORK_URL=https://horizon.stellar.org
STELLAR_NETWORK_PASSPHRASE=Public Global Stellar Network ; September 2015

# Monitoring
SENTRY_DSN=${SENTRY_DSN}
```

## Adding New Variables

### Step 1: Update Schema

Edit `src/config/schema.ts`:

```typescript
export const ConfigSchema = z.object({
  // ... existing fields ...
  
  // Add your new field
  NEW_VARIABLE: z.string().min(1),
  NEW_NUMBER: z.coerce.number().int().min(0).default(100),
});
```

### Step 2: Add Production Rules (if needed)

```typescript
export const ProductionConfigSchema = ConfigSchema.extend({
  // Override with stricter rules for production
  NEW_VARIABLE: z.string().min(10),
});
```

### Step 3: Update Documentation

Add the new variable to the table above with:
- Variable name
- Type
- Required/Optional
- Default value
- Description
- Production-specific rules

### Step 4: Update .env.example

```bash
# Add to .env.example
NEW_VARIABLE=example-value
NEW_NUMBER=100
```

## Security

### Sensitive Key Detection

The system automatically identifies sensitive keys using patterns:
- `password`
- `secret`
- `token`
- `key`
- `auth`
- `credential`
- `private`
- `api_key` / `api-key`

These values are automatically redacted in:
- Console logs
- Error messages
- String representations
- Debug output

### Secret Redaction Example

```typescript
const config = {
  PORT: 3000,
  JWT_SECRET: 'super-secret-value',
  DATABASE_PASSWORD: 'db-password-123',
};

console.log(getSafeConfig(config));
// Output:
// {
//   PORT: 3000,
//   JWT_SECRET: '[REDACTED]',
//   DATABASE_PASSWORD: '[REDACTED]'
// }
```

### Error Message Safety

When validation fails, error messages never include the actual values of sensitive fields:

```
❌ CONFIGURATION ERROR

Configuration validation failed for profile "production":
  - JWT_SECRET: String must contain at least 64 character(s)
  - API_KEY: String must contain at least 32 character(s)

Please check your environment variables and try again.
```

Note: The actual secret values are NOT shown in the error.

## Validation Errors

### Common Errors

#### Missing Required Variable
```
- DATABASE_URL: Required
```
**Solution**: Add the variable to your .env file

#### Invalid Type
```
- PORT: Expected number, received string
```
**Solution**: Ensure the value is a valid number (no quotes needed in .env)

#### Invalid URL Format
```
- DATABASE_URL: Invalid url
```
**Solution**: Ensure the URL is properly formatted (e.g., `postgresql://host:port/db`)

#### Value Out of Range
```
- PORT: Number must be greater than or equal to 1
```
**Solution**: Provide a value within the allowed range

#### Invalid Enum Value
```
- NODE_ENV: Invalid enum value. Expected 'development' | 'test' | 'production'
```
**Solution**: Use one of the allowed values

### Production-Specific Errors

#### Secret Too Short
```
- JWT_SECRET: String must contain at least 64 character(s)
```
**Solution**: Use a longer secret in production (generate with `openssl rand -base64 64`)

#### Wrong Database Type
```
- DATABASE_URL: Production database must use PostgreSQL
```
**Solution**: Use a PostgreSQL connection string (starts with `postgresql://` or `postgres://`)

## Testing

### Unit Tests

```bash
npm test src/config
```

### Coverage

```bash
npm run test:coverage -- src/config
```

Target: 95%+ coverage

### Test with Different Profiles

```bash
# Development
NODE_ENV=development npm test

# Production validation
NODE_ENV=production npm test

# Test profile
NODE_ENV=test npm test
```

## CI/CD Integration

### GitHub Actions Example

```yaml
- name: Validate Configuration
  env:
    NODE_ENV: production
    DATABASE_URL: ${{ secrets.DATABASE_URL }}
    JWT_SECRET: ${{ secrets.JWT_SECRET }}
    API_KEY: ${{ secrets.API_KEY }}
    ENCRYPTION_KEY: ${{ secrets.ENCRYPTION_KEY }}
    STELLAR_NETWORK_URL: https://horizon.stellar.org
    STELLAR_NETWORK_PASSPHRASE: Public Global Stellar Network ; September 2015
  run: |
    npm run build
    node -e "require('./dist/config').getConfig()"
```

## Troubleshooting

### Application Won't Start

1. Check console output for validation errors
2. Verify all required variables are set
3. Ensure values match expected types and formats
4. Check production-specific rules if `NODE_ENV=production`

### Secrets Appearing in Logs

This should never happen. If it does:
1. Check if the key name matches sensitive patterns
2. Verify you're using `getSafeConfig()` for logging
3. Report as a security issue

### Type Coercion Not Working

Environment variables are always strings. Use `z.coerce.number()` or `z.coerce.boolean()` for automatic conversion.

## Best Practices

1. **Never commit secrets** - Use `.env.local` for local secrets (gitignored)
2. **Use environment variables in production** - Don't use .env files in production
3. **Generate strong secrets** - Use `openssl rand -base64 64` for production secrets
4. **Validate early** - Configuration is validated at startup, before any other code runs
5. **Log safely** - Always use `getSafeConfig()` when logging configuration
6. **Document new variables** - Update this file when adding configuration options
7. **Test with production profile** - Ensure your configuration passes production validation

## Security Audit Summary

✅ **No secrets in logs**: All sensitive values automatically redacted  
✅ **No secrets in errors**: Validation errors never expose secret values  
✅ **Fail-fast**: Invalid configuration prevents application startup  
✅ **Type safety**: Zod ensures runtime type correctness  
✅ **Production hardening**: Stricter rules for production environment  

Last Updated: 2024-01-25
