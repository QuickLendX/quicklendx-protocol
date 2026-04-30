# QuickLendX Backend API Service

This is the backend API service skeleton for the QuickLendX protocol. It provides a versioned, secure, and contract-aware foundation for off-chain functionality.

## Features

- **Contract-Aware**: Schemas and types are aligned with the Soroban smart contracts (`Invoice`, `Bid`, `Investment`).
- **OpenAPI 3.0**: Fully documented API with explicit versioning (`v1`).
- **Security**:
  - Helmet for secure HTTP headers.
  - Rate limiting to prevent DDoS.
  - Centralized safe error handling.
- **Testing**: Comprehensive integration tests with >95% coverage.
- **TypeScript**: Typed throughout for reliability and alignment with the frontend.

## Getting Started

### Prerequisites

- Node.js >= 18.x
- npm

### Installation

```bash
cd backend
npm install
```

### Development

Start the development server with hot-reload:

```bash
npm run dev
```

The API will be available at `http://localhost:3001/api/v1`.
Health check is available at `http://localhost:3001/health`.
Swagger documentation (TODO) will be available at `http://localhost:3001/api-docs`.

### Build

```bash
npm run build
```

### Testing

Run the test suite with coverage:

```bash
npm run test:coverage
```

### Dependency Security and SBOM

Run dependency vulnerability gate locally:

```bash
npm audit --json > audit-report.json || true
npm run security:scan
```

Generate and validate the backend SBOM (CycloneDX JSON):

```bash
npm run sbom:generate
npm run sbom:check
```

## Project Structure

- `src/app.ts`: Express application setup
- `src/routes/v1/`: Versioned API routes
- `src/controllers/v1/`: Logic handlers
- `src/types/contract.ts`: TypeScript interfaces mirroring contract types
- `src/middleware/`: Security and utility middleware
- `src/models/`: Data models (API keys, etc.)
- `src/services/`: Business logic services (API key management, audit logging)
- `src/config/`: Configuration (scopes, etc.)
- `src/db/`: Database interface
- `src/tests/`: Test suites
- `docs/`: API documentation
- `openapi.yaml`: OpenAPI specification

## API Key System

The backend includes a complete API key authentication system for service-to-service communication.

### Quick Start

See [API Key Quick Start Guide](API_KEY_QUICK_START.md) for getting started.

### Documentation

- **[API Key Documentation](docs/auth.md)** - Complete authentication guide
- **[Security Checklist](SECURITY_CHECKLIST.md)** - Security validation
- **[Implementation Summary](API_KEY_IMPLEMENTATION_SUMMARY.md)** - Technical details
- **[Quick Start](API_KEY_QUICK_START.md)** - Getting started guide

### API Key Endpoints

- `POST /api/v1/keys` - Create new API key
- `GET /api/v1/keys` - List all keys
- `GET /api/v1/keys/:id` - Get specific key
- `POST /api/v1/keys/:id/rotate` - Rotate a key
- `POST /api/v1/keys/:id/revoke` - Revoke a key
- `GET /api/v1/keys/:id/audit-logs` - Get audit logs
- `GET /api/v1/keys/scopes` - Get available scopes (public)

### Running API Key Tests

```bash
npm test -- --testPathPattern=api-key.test.ts
```

## API Documentation

The OpenAPI spec is located in `openapi.yaml`. You can view it using any Swagger/OpenAPI viewer.

## Security Assumptions

- **Auth Model**: Initial skeleton uses a Bearer token placeholder in the middleware. Production implementation should integrate with Soroban wallet signatures or JWT.
- **Rate Limits**: Configured for 100 requests per minute per IP.
- **Error Handling**: Internal errors are masked in production; only safe messages and codes are returned.

## RBAC

The backend now separates privileged duties across three explicit admin roles:

- `support`: read-only troubleshooting access
- `operations_admin`: maintenance and backfill operations
- `super_admin`: dangerous configuration changes and full admin access

See [docs/rbac.md](docs/rbac.md) for the authorization matrix, environment variables, audit logging behavior, and security notes.
