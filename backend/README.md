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

## Project Structure

- `src/app.ts`: Express application setup.
- `src/routes/v1/`: Versioned API routes.
- `src/controllers/v1/`: Logic handlers.
- `src/types/contract.ts`: TypeScript interfaces mirroring contract types.
- `src/middleware/`: Security and utility middleware.
- `openapi.yaml`: OpenAPI specification.

## API Documentation

The OpenAPI spec is located in `openapi.yaml`. You can view it using any Swagger/OpenAPI viewer.

## Security Assumptions

- **Auth Model**: Initial skeleton uses a Bearer token placeholder in the middleware. Production implementation should integrate with Soroban wallet signatures or JWT.
- **Rate Limits**: Configured for 100 requests per minute per IP.
- **Error Handling**: Internal errors are masked in production; only safe messages and codes are returned.
