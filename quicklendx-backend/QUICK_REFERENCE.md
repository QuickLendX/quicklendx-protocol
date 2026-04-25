# Quick Reference Card

## Installation

```bash
npm install
cp .env.example .env
# Edit .env with your values
```

## Common Commands

```bash
# Testing
npm test                    # Run all tests
npm run test:watch         # Watch mode
npm run test:coverage      # Coverage report

# Development
npm run dev                # Start dev server
npm run build              # Build for production
npm start                  # Start production server

# Code Quality
npm run lint               # Run ESLint
npm run format             # Format with Prettier
```

## Configuration Usage

```typescript
import { getConfig, getSafeConfig } from './config';

// Load config (fails fast if invalid)
const config = getConfig();

// Use config
const port = config.PORT;
const dbUrl = config.DATABASE_URL;

// Log safely (secrets redacted)
console.log(getSafeConfig(config));
```

## Contract Testing Usage

```typescript
import { createContractHarness } from './testing';

// Create harness
const harness = createContractHarness({
  failFast: true,
  verbose: false,
});

// Test response
const result = harness.testResponse(
  'POST',
  '/auth/login',
  200,
  responseBody
);

// Check result
if (!result.passed) {
  console.error(result.validation.errors);
}

// Get summary
const summary = harness.getSummary();
console.log(`Pass rate: ${summary.passRate}%`);
```

## Environment Variables

### Required
```bash
DATABASE_URL=postgresql://localhost:5432/db
JWT_SECRET=min-32-chars-dev-64-chars-prod
API_KEY=min-16-chars-dev-32-chars-prod
ENCRYPTION_KEY=min-32-chars-dev-64-chars-prod
STELLAR_NETWORK_URL=https://horizon-testnet.stellar.org
STELLAR_NETWORK_PASSPHRASE=Test SDF Network ; September 2015
```

### Optional
```bash
NODE_ENV=development
PORT=3000
LOG_LEVEL=info
DATABASE_POOL_SIZE=10
ENABLE_RATE_LIMITING=true
MAX_REQUESTS_PER_MINUTE=100
SENTRY_DSN=https://...
```

## Test Fixtures

```typescript
import {
  authFixtures,
  userFixtures,
  invoiceFixtures,
  bidFixtures,
  systemFixtures,
} from './testing';

// Use in tests
harness.testResponse(
  'POST',
  '/auth/login',
  200,
  authFixtures.validLoginResponse
);
```

## Common Patterns

### Adding New Config Variable

1. Update `src/config/schema.ts`:
```typescript
export const ConfigSchema = z.object({
  // ... existing
  NEW_VAR: z.string().min(1),
});
```

2. Update `.env.example`:
```bash
NEW_VAR=example-value
```

3. Update `docs/configuration.md` table

### Adding New API Endpoint

1. Update `openapi.yaml`:
```yaml
paths:
  /new-endpoint:
    get:
      responses:
        '200':
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/NewSchema'
```

2. Create fixture in `src/testing/fixtures/`:
```typescript
export const validNewResponse = {
  id: 'uuid',
  data: 'value',
};
```

3. Add contract test:
```typescript
it('should validate new endpoint', () => {
  const result = validator.validateResponse(
    'GET',
    '/new-endpoint',
    200,
    validNewResponse
  );
  expect(result.valid).toBe(true);
});
```

## Troubleshooting

### Tests Failing
```bash
rm -rf node_modules coverage
npm install
npm test
```

### Config Not Loading
- Check `.env` file exists
- Verify all required variables set
- Check for typos in variable names
- Review error message (secrets redacted)

### Contract Tests Failing
- Verify OpenAPI spec is valid
- Check response matches schema exactly
- Review validation errors for details
- Ensure fixtures are up-to-date

## File Locations

| What | Where |
|------|-------|
| Config code | `src/config/` |
| Config tests | `src/config/__tests__/` |
| Contract code | `src/testing/` |
| Contract tests | `src/testing/__tests__/` |
| Fixtures | `src/testing/fixtures/` |
| OpenAPI spec | `openapi.yaml` |
| Config docs | `docs/configuration.md` |
| Testing docs | `docs/testing.md` |
| Environment | `.env` (create from `.env.example`) |

## Coverage Thresholds

- Minimum: 95%
- Current: 97.5%
- Target: Maintain above 95%

```bash
npm run test:coverage
open coverage/index.html
```

## Security Checklist

- [ ] No secrets in `.env` committed to git
- [ ] Production secrets are 64+ characters
- [ ] All tests passing
- [ ] Coverage above 95%
- [ ] No secrets in logs (use `getSafeConfig()`)
- [ ] Contract tests validate all endpoints
- [ ] OpenAPI spec is up-to-date

## Documentation

- [README](README.md) - Project overview
- [Configuration Guide](docs/configuration.md) - Complete config reference
- [Testing Guide](docs/testing.md) - Testing strategies
- [Implementation Summary](IMPLEMENTATION_SUMMARY.md) - Technical details
- [Security Audit](SECURITY_AUDIT.md) - Security review

## Support

1. Check documentation first
2. Review test examples
3. Check error messages
4. Open GitHub issue

## Key Principles

1. **Fail Fast** - Invalid config stops startup
2. **Type Safe** - Zod enforces types at runtime
3. **Secret Safe** - Automatic redaction everywhere
4. **Test First** - 95%+ coverage required
5. **Contract Driven** - OpenAPI is source of truth

---

**Quick Start**: `npm install && cp .env.example .env && npm test`
