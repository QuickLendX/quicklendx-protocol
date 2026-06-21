# QuickLendX Backend – Security & Release Checklist

This document defines the mandatory quality and security gates that **must** pass
before any code change is merged to `main` or tagged for release.

---

## 1. OpenAPI Contract Conformance (MANDATORY BUILD GATE)

### Overview

The project ships a build-time contract conformance suite located at
`backend/tests/openapi-conformance.test.ts`.  It automatically:

1. Parses `backend/openapi.yaml` and extracts every documented path × HTTP
   method × status-code × example block.
2. Fires each combination as a live HTTP request through `supertest` against the
   Express app exported from `backend/src/app.ts`.
3. Validates the response HTTP status code against the spec.
4. Validates the response body against the AJV-compiled JSON Schema derived
   from the OpenAPI Schema Object for that response.
5. Enforces `additionalProperties: false` so that undocumented fields in a
   response trigger an immediate test failure.

### Why This Matters

| Risk | How the suite catches it |
|---|---|
| API drift (endpoint removed or path changed) | Supertest call returns unexpected 404 → status assertion fails |
| Schema drift (field renamed, type changed) | AJV validation fails against compiled schema |
| Missing required field | AJV `required` keyword check fails |
| Undocumented extra field | `additionalProperties: false` rejects the body |
| Enum drift (new/removed enum values) | AJV `enum` check fails |
| `oneOf`/`anyOf` branch deviation | AJV composition keywords reject the body |
| Format mismatch (e.g. number instead of string for i128) | AJV `type` check fails |

### Mandatory Release Gate Rule

> **The OpenAPI contract conformance suite MUST pass with zero failures before
> any release artifact is built, any PR is merged to `main`, or any deployment
> to any environment is triggered.**

This rule is enforced in CI via `.github/workflows/ci.yml`.  The relevant step
runs:

```bash
npm test -- --testPathPattern=openapi-conformance --forceExit
```

If this step fails the CI pipeline is blocked and the build is not promoted.

### Updating the Spec

When an endpoint changes its response shape you **must**:

1. Update `backend/openapi.yaml` to reflect the new schema and examples.
2. Verify that `npm test -- openapi-conformance` still passes.
3. Include the `openapi.yaml` diff in the PR.

Failure to keep the spec in sync will be caught by this suite in CI.

### Coverage Requirement

Code coverage for `backend/src/tests/helpers/openapi-loader.ts` and
`backend/tests/openapi-conformance.test.ts` must remain at or above **95%**
(branches, functions, lines, statements) as enforced by the `jest.config.js`
coverage thresholds.

---

## 2. Dependency Security

- All production dependencies must be pinned to an exact version or a tight
  semver range in `package.json`.
- Run `npm audit` before every release.  Any `high` or `critical` severity
  findings must be resolved before release.
- New dependencies require explicit team approval.

---

## 3. Rate Limiting

- The global rate limiter (`backend/src/middleware/rate-limit.ts`) must remain
  active on all routes.
- The limit and window must be reviewed whenever traffic patterns change.

---

## 4. Input Validation

- All user-supplied query parameters and path parameters must be validated
  before being passed to business logic.
- Zod schemas (or equivalent) must be used for request body validation on any
  POST/PUT/PATCH endpoints added in future.

---

## 5. Authentication & Authorisation

- Bearer JWT authentication (`bearerAuth` scheme) is defined in the OpenAPI
  spec.  Any new endpoint that handles sensitive data must declare this security
  scheme.
- JWT secrets must never be committed to the repository.  They must be supplied
  via environment variables.

---

## 6. Error Handling

- The global error handler (`backend/src/middleware/error-handler.ts`) must
  never leak stack traces or internal details to clients in production.
- Error responses must conform to the `ErrorResponse` schema defined in
  `backend/openapi.yaml`.

---

## 7. Pre-Release Checklist Summary

| # | Gate | Tool / Command | Must Pass? |
|---|---|---|---|
| 1 | OpenAPI conformance suite | `npm test -- openapi-conformance` | ✅ Yes |
| 2 | Full test suite with coverage | `npm run test:coverage` | ✅ Yes |
| 3 | TypeScript compilation | `npx tsc --noEmit` | ✅ Yes |
| 4 | Dependency audit | `npm audit` | ✅ Yes (no high/critical) |
| 5 | Linting | `npx eslint src` (when configured) | ✅ Yes |
| 6 | Manual smoke test on staging | n/a | ✅ Yes |
