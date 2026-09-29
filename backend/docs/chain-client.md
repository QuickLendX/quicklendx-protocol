# Reliable RPC Client for Stellar/Soroban

The `ReliableRpcClient` provides a robust wrapper around JSON-RPC calls to the Stellar network (Soroban). It implements several resiliency patterns to ensure high availability and prevent "retry storms" in case of network instability.

## Features

- **Exponential Backoff with Jitter**: Retries failed network requests with increasing delays. Full jitter is added to the backoff to prevent a large number of clients from hitting the RPC server simultaneously after a recovery.
- **Circuit Breaker**: Detects repeated failures and "trips" the circuit, temporarily blocking all calls. This protects the backend from wasting resources on a failing provider and gives the RPC server time to recover.
- **Concurrency Caps**: Limits the number of concurrent outgoing RPC requests to prevent overwhelming the client or the server.
- **SSRF Protection**: Validates the RPC host against a strict allow-list configured via environment variables.

## Configuration

Configuration is handled via environment variables in `src/config.ts`:

- `STELLAR_RPC_URL`: The full URL of the Soroban RPC provider.
- `RPC_ALLOWED_HOSTS`: A comma-separated list of allowed hostnames (e.g., `soroban-testnet.stellar.org,localhost`).

## Implementation Details

### Circuit Breaker States
1. **CLOSED**: Normal operation. Requests flow through.
2. **OPEN**: Threshold of failures reached. Requests are blocked immediately for a `resetTimeout`.
3. **HALF_OPEN**: After the timeout, a single "probe" request is allowed. If it succeeds, the circuit closes. If it fails, it re-opens.

### Retry Logic
The client retries on:
- Network failures (Connection reset, timeout, etc.)
- HTTP 429 (Too Many Requests)
- HTTP 5xx (Server errors)

It does **NOT** retry on RPC protocol errors (e.g., "Method not found", "Invalid params") as these are typically permanent errors.

## Usage

```typescript
import { rpcClient } from './services/rpcClient';

async function getContractData() {
  try {
    const result = await rpcClient.call('getLedgerEntries', { keys: [...] });
    return result;
  } catch (error) {
    if (error.message.includes('Circuit breaker is OPEN')) {
      // Handle circuit breaker state (e.g., use local cache)
    }
    throw error;
  }
}
```

## Testing

Comprehensive tests are located in `tests/rpcClient.test.ts`. They mock the network layer and use fake timers to verify:
- Backoff timing and jitter.
- Circuit state transitions.
- Concurrency limit enforcement.
- Host validation.
