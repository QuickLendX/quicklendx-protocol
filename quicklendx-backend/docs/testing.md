# Testing Guide

## Overview

The QuickLendX backend implements comprehensive testing strategies including unit tests, integration tests, and contract tests. This guide covers all testing approaches and best practices.

## Test Structure

```
src/
├── config/
│   ├── __tests__/
│   │   ├── loader.test.ts
│   │   └── masking.test.ts
│   ├── index.ts
│   ├── loader.ts
│   ├── masking.ts
│   └── schema.ts
└── testing/
    ├── __tests__/
    │   ├── contract-validator.test.ts
    │   └── contract-harness.test.ts
    ├── fixtures/
    │   ├── auth.fixtures.ts
    │   ├── user.fixtures.ts
    │   ├── invoice.fixtures.ts
    │   ├── bid.fixtures.ts
    │   └── system.fixtures.ts
    ├── contract-validator.ts
    └── contract-harness.ts
```

## Running Tests

### All Tests
```bash
npm test
```

### Watch Mode
```bash
npm run test:watch
```

### Coverage Report
```bash
npm run test:coverage
```

### Specific Test File
```bash
npm test src/config/__tests__/loader.test.ts
```

### Specific Test Suite
```bash
npm test -- --grep "ContractValidator"
```

## Contract Testing

### What is Contract Testing?

Contract testing validates that API responses strictly adhere to the OpenAPI specification. This prevents breaking changes and ensures API consistency.

### Benefits

- **Prevents Breaking Changes**: Catches schema violations before deployment
- **Documentation as Tests**: OpenAPI spec serves as single source of truth
- **Fail-Fast**: Tests fail immediately on contract violations
- **Clear Error Messages**: Shows exactly what doesn't match the spec

### OpenAPI Specification

The API contract is defined in `openapi.yaml` at the project root. This file:
- Defines all endpoints, methods, and status codes
- Specifies request/response schemas
- Documents data types, formats, and constraints
- Serves as the contract between frontend and backend

### Contract Test Harness

The contract testing harness provides utilities for validating API responses:

```typescript
import { createContractHarness } from './testing/contract-harness';

// Create harness instance
const harness = createContractHarness({
  specPath: './openapi.yaml',  // Path to OpenAPI spec
  failFast: true,              // Throw on first failure
  verbose: false,              // Log each test result
});

// Test a response
const result = harness.testResponse(
  'POST',                      // HTTP method
  '/auth/login',               // Endpoint path
  200,                         // Status code
  responseBody                 // Actual response data
);

// Check result
if (!result.passed) {
  console.error('Contract violation:', result.validation.errors);
}
```

### Running Contract Tests Locally

```bash
# Run all contract tests
npm test src/testing/__tests__/contract-validator.test.ts

# Run with coverage
npm run test:coverage -- src/testing/__tests__/contract-validator.test.ts

# Watch mode for development
npm run test:watch -- src/testing/__tests__/contract-validator.test.ts
```

### Contract Test Examples

#### Valid Response Test
```typescript
import { createContractHarness } from '../testing/contract-harness';
import { validLoginResponse } from '../testing/fixtures/auth.fixtures';

const harness = createContractHarness();

// This should pass
const result = harness.testResponse(
  'POST',
  '/auth/login',
  200,
  validLoginResponse
);

expect(result.passed).toBe(true);
```

#### Invalid Response Test
```typescript
// Missing required field
const invalidResponse = {
  token: 'some-token',
  // Missing 'user' field
};

const result = harness.testResponse(
  'POST',
  '/auth/login',
  200,
  invalidResponse
);

expect(result.passed).toBe(false);
expect(result.validation.errors).toContainEqual(
  expect.objectContaining({
    path: 'user',
    message: 'Required property missing',
  })
);
```

#### Type Mismatch Test
```typescript
const invalidResponse = {
  token: 12345,  // Should be string
  user: { /* ... */ },
};

const result = harness.testResponse(
  'POST',
  '/auth/login',
  200,
  invalidResponse
);

expect(result.passed).toBe(false);
expect(result.validation.errors).toContainEqual(
  expect.objectContaining({
    path: 'token',
    message: 'Type mismatch',
    expected: 'string',
    actual: 'number',
  })
);
```

### Test Fixtures

Fixtures provide consistent test data for contract tests. They're organized by domain:

#### Authentication Fixtures (`fixtures/auth.fixtures.ts`)
```typescript
import { validLoginResponse, invalidCredentialsError } from './fixtures/auth.fixtures';

// Use in tests
harness.testResponse('POST', '/auth/login', 200, validLoginResponse);
harness.testResponse('POST', '/auth/login', 401, invalidCredentialsError);
```

#### Invoice Fixtures (`fixtures/invoice.fixtures.ts`)
```typescript
import { validInvoice, validInvoiceListResponse } from './fixtures/invoice.fixtures';

harness.testResponse('POST', '/invoices', 201, validInvoice);
harness.testResponse('GET', '/invoices', 200, validInvoiceListResponse);
```

#### Creating New Fixtures

When adding new endpoints:

1. Create fixture file in `src/testing/fixtures/`
2. Export valid and invalid response examples
3. Ensure fixtures match OpenAPI schema
4. Use realistic but fake data

Example:
```typescript
// src/testing/fixtures/payment.fixtures.ts
export const validPayment = {
  id: '123e4567-e89b-12d3-a456-426614174000',
  amount: '1000.00',
  currency: 'USDC',
  status: 'completed',
  createdAt: '2024-01-25T10:00:00Z',
};

export const paymentNotFoundError = {
  error: 'NOT_FOUND',
  message: 'Payment not found',
};
```

### Updating Fixtures When API Changes

When the OpenAPI spec changes:

1. **Update the spec** in `openapi.yaml`
2. **Update fixtures** to match new schema
3. **Run contract tests** to verify changes
4. **Update integration tests** if needed

Example workflow:
```bash
# 1. Edit openapi.yaml
vim openapi.yaml

# 2. Update fixtures
vim src/testing/fixtures/invoice.fixtures.ts

# 3. Run tests to verify
npm test src/testing/__tests__/contract-validator.test.ts

# 4. Check coverage
npm run test:coverage
```

### Handling Breaking Changes

When contract tests fail, you have two options:

#### Option 1: Fix the Response (Recommended)
If the response is wrong, fix the implementation:
```typescript
// Before (wrong)
return { data: invoices };

// After (correct per spec)
return {
  data: invoices,
  total: invoices.length,
  limit: 20,
  offset: 0,
};
```

#### Option 2: Update the Contract
If the spec needs to change (breaking change):

1. **Document the breaking change** in CHANGELOG
2. **Update OpenAPI spec** with new schema
3. **Update fixtures** to match
4. **Version the API** if needed (e.g., `/api/v2`)
5. **Notify frontend team** of changes

### CI/CD Integration

Contract tests run automatically in CI/CD:

```yaml
# .github/workflows/test.yml
- name: Run Contract Tests
  run: |
    npm test src/testing/__tests__/contract-validator.test.ts
    npm test src/testing/__tests__/contract-harness.test.ts

- name: Check Coverage
  run: |
    npm run test:coverage
    # Fail if coverage < 95%
```

### Contract Test Best Practices

1. **Test All Endpoints**: Every endpoint should have contract tests
2. **Test All Status Codes**: Test success and error responses
3. **Use Fixtures**: Don't inline test data, use fixtures
4. **Test Edge Cases**: Empty arrays, null values, optional fields
5. **Keep Fixtures Realistic**: Use valid UUIDs, dates, emails
6. **Update Together**: Keep spec, fixtures, and tests in sync
7. **Fail Fast**: Use `failFast: true` in CI/CD
8. **No Real Secrets**: Use mock tokens and test credentials

## Security Testing

### No Secret Leakage

Contract tests verify that sensitive data is never exposed:

```typescript
it('should not expose sensitive data in error messages', () => {
  const responseWithSecrets = {
    token: 'super-secret-jwt-token-12345',
    user: { id: 'invalid-uuid', /* ... */ },
  };

  try {
    harness.testResponse('POST', '/auth/login', 200, responseWithSecrets);
  } catch (error) {
    const errorMessage = error.message;
    
    // Should mention the field
    expect(errorMessage).toContain('id');
    
    // Should NOT contain the secret
    expect(errorMessage).not.toContain('super-secret-jwt-token-12345');
  }
});
```

### Mock Authentication

Contract tests use mock authentication:

```typescript
// Don't use real tokens
const mockToken = 'mock-jwt-token-for-testing';

// Don't use real credentials
const testCredentials = {
  email: 'test@example.com',
  password: 'TestPassword123!',
};
```

### Environment Isolation

Tests run in isolated environment:

```typescript
beforeEach(() => {
  process.env.NODE_ENV = 'test';
  // No production secrets required
});
```

## Configuration Testing

### Testing Valid Configuration

```typescript
it('should load valid configuration', () => {
  process.env = {
    DATABASE_URL: 'postgresql://localhost:5432/testdb',
    JWT_SECRET: 'test-secret-minimum-32-characters',
    // ... other required vars
  };

  const config = loadConfig();
  expect(config.DATABASE_URL).toBe('postgresql://localhost:5432/testdb');
});
```

### Testing Invalid Configuration

```typescript
it('should fail when required field is missing', () => {
  process.env = {
    // Missing DATABASE_URL
    JWT_SECRET: 'test-secret',
  };

  loadConfig();
  expect(process.exit).toHaveBeenCalledWith(1);
});
```

### Testing Secret Redaction

```typescript
it('should redact sensitive values', () => {
  const config = {
    PORT: 3000,
    JWT_SECRET: 'super-secret',
  };

  const safe = getSafeConfig(config);
  expect(safe.JWT_SECRET).toBe('[REDACTED]');
  expect(safe.PORT).toBe(3000);
});
```

## Coverage Requirements

### Minimum Coverage: 95%

All modules must maintain at least 95% test coverage:

```bash
npm run test:coverage

# Output:
# File                | % Stmts | % Branch | % Funcs | % Lines
# --------------------|---------|----------|---------|--------
# All files           |   97.5  |   95.2   |   98.1  |   97.8
# config/             |   98.2  |   96.5   |   100   |   98.5
# testing/            |   96.8  |   94.0   |   96.2  |   97.1
```

### Coverage Reports

Coverage reports are generated in `coverage/`:
- `coverage/index.html` - HTML report (open in browser)
- `coverage/lcov.info` - LCOV format (for CI tools)
- `coverage/coverage-summary.json` - JSON summary

### Improving Coverage

If coverage is below 95%:

1. **Identify uncovered lines**:
   ```bash
   npm run test:coverage
   open coverage/index.html
   ```

2. **Add missing tests** for:
   - Edge cases
   - Error paths
   - Boundary conditions
   - Type validations

3. **Verify improvement**:
   ```bash
   npm run test:coverage
   ```

## Test Organization

### Test File Naming

- Unit tests: `*.test.ts` next to source file
- Integration tests: `__tests__/*.test.ts` in module directory
- Fixtures: `fixtures/*.fixtures.ts`

### Test Structure

```typescript
describe('ModuleName', () => {
  describe('functionName', () => {
    it('should handle valid input', () => {
      // Arrange
      const input = 'valid';
      
      // Act
      const result = functionName(input);
      
      // Assert
      expect(result).toBe('expected');
    });

    it('should handle invalid input', () => {
      expect(() => functionName('invalid')).toThrow();
    });
  });
});
```

### Test Lifecycle

```typescript
describe('TestSuite', () => {
  beforeAll(() => {
    // Run once before all tests
  });

  beforeEach(() => {
    // Run before each test
    resetConfig();
  });

  afterEach(() => {
    // Run after each test
    vi.clearAllMocks();
  });

  afterAll(() => {
    // Run once after all tests
  });
});
```

## Debugging Tests

### Run Single Test

```bash
npm test -- --grep "should validate login response"
```

### Debug in VS Code

Add to `.vscode/launch.json`:
```json
{
  "type": "node",
  "request": "launch",
  "name": "Debug Tests",
  "runtimeExecutable": "npm",
  "runtimeArgs": ["test", "--", "--no-coverage"],
  "console": "integratedTerminal"
}
```

### Verbose Output

```bash
npm test -- --reporter=verbose
```

## Continuous Integration

### GitHub Actions

```yaml
name: Test

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup Node.js
        uses: actions/setup-node@v3
        with:
          node-version: '20'
      
      - name: Install dependencies
        run: npm ci
      
      - name: Run tests
        run: npm test
      
      - name: Check coverage
        run: npm run test:coverage
      
      - name: Upload coverage
        uses: codecov/codecov-action@v3
        with:
          files: ./coverage/lcov.info
```

### Pre-commit Hooks

```json
// package.json
{
  "husky": {
    "hooks": {
      "pre-commit": "npm test",
      "pre-push": "npm run test:coverage"
    }
  }
}
```

## Troubleshooting

### Tests Failing Locally

1. Clear node_modules: `rm -rf node_modules && npm ci`
2. Clear test cache: `npm test -- --clearCache`
3. Check Node version: `node --version` (should be 20+)

### Tests Passing Locally, Failing in CI

1. Check environment variables
2. Verify Node version matches
3. Check for timing issues (use `vi.useFakeTimers()`)
4. Review CI logs for specific errors

### Coverage Not Updating

1. Clear coverage directory: `rm -rf coverage`
2. Run with clean cache: `npm test -- --clearCache --coverage`

## Best Practices

1. **Write Tests First** (TDD) - Define behavior before implementation
2. **Test Behavior, Not Implementation** - Focus on what, not how
3. **Keep Tests Simple** - One assertion per test when possible
4. **Use Descriptive Names** - Test names should explain what they test
5. **Avoid Test Interdependence** - Each test should run independently
6. **Mock External Dependencies** - Don't call real APIs or databases
7. **Test Edge Cases** - Null, undefined, empty, boundary values
8. **Maintain Fixtures** - Keep test data organized and reusable
9. **Review Coverage** - Aim for 95%+ coverage
10. **Update Tests with Code** - Keep tests in sync with implementation

## Resources

- [Vitest Documentation](https://vitest.dev/)
- [OpenAPI Specification](https://swagger.io/specification/)
- [Contract Testing Guide](https://martinfowler.com/bliki/ContractTest.html)
- [Testing Best Practices](https://testingjavascript.com/)

Last Updated: 2024-01-25
