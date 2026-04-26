# Pull Request: feat/configurable-retry-config

## 📝 Description

Exposes the previously hardcoded exponential-backoff retry parameters as a consumer-configurable `retryConfig` option on `StellarClientOptions`. SDK consumers can now tune or disable retries without forking the client.

## 🎯 Type of Change

- [x] New feature
- [x] Refactoring

## 🔧 Changes Made

### Files Modified

- `quicklendx-frontend/app/lib/api-client.ts` — added `RetryConfig` interface, `StellarClientOptions.retryConfig`, `DEFAULT_RETRY` constant, and wired values into `ErrorRecovery.retryOperation` call.
- `quicklendx-frontend/app/lib/errors.ts` — minor adjustment to `retryOperation` signature to accept `maxDelayMs`.

### New Files Added

- `quicklendx-frontend/__tests__/retry-config.test.ts` — unit tests for retry behaviour.
- `quicklendx-frontend/jest.config.ts` — Jest configuration.
- `quicklendx-frontend/tsconfig.test.json` — TypeScript config for tests.

### Key Changes

- `RetryConfig` interface: optional `maxRetries`, `initialDelayMs`, `maxDelayMs`.
- `DEFAULT_RETRY`: `{ maxRetries: 3, initialDelayMs: 1000, maxDelayMs: 30000 }`.
- Constructor merges consumer config over defaults: `{ ...DEFAULT_RETRY, ...options.retryConfig }`.
- `maxRetries: 0` short-circuits to a single attempt with no retry loop.

## 🧪 Testing

- [x] Unit tests pass
- [x] No breaking changes introduced
- [x] Edge cases tested

### Test Coverage

7 unit tests in `__tests__/retry-config.test.ts`:

| Test | What it verifies |
|---|---|
| succeeds on first attempt without retrying | happy path, op called once |
| retries up to maxRetries times then throws | exhausted retries, correct call count |
| maxRetries: 0 calls operation exactly once | retry disabled, no extra attempts |
| caps wait time at maxDelayMs | exponential backoff ceiling |
| uses DEFAULT_RETRY values when no retryConfig provided | defaults applied |
| merges consumer retryConfig over defaults | partial override works |
| maxRetries: 0 disables retries — retryOperation called with maxRetries=0 | end-to-end via ApiClient |

Run with:
```bash
cd quicklendx-frontend && npm test -- --testPathPatterns=retry-config
```

## 📋 Review Checklist

- [x] Code follows project style guidelines
- [x] No hardcoded values (defaults centralised in `DEFAULT_RETRY`)
- [x] Error handling implemented
- [x] No sensitive data exposed

## 🔗 Related Issues

Closes #<!-- issue number -->

## 🧪 How to Test

1. `cd quicklendx-frontend && npm ci`
2. `npm test -- --testPathPatterns=retry-config` — all 7 tests should pass.
3. Instantiate `new ApiClient({ retryConfig: { maxRetries: 0 } })` and confirm a failing request throws immediately without retrying.
4. Instantiate with `{ retryConfig: { maxRetries: 5, initialDelayMs: 500 } }` and confirm the merged config via `(client as any).retryConfig`.
