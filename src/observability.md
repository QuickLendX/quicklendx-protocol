# Observability: Request ID Propagation and Correlation

## Overview

To facilitate end-to-end debugging and tracing across various components of the QuickLendX backend (API, indexer, webhook delivery), a request ID propagation strategy has been implemented. Every incoming API request is assigned or propagates an `x-request-id`, which is then correlated with logs and, where relevant, with indexer cursor data.

## Request ID (`x-request-id`)

The `x-request-id` is a unique identifier (UUID v4) assigned to each incoming HTTP request.

### Generation and Propagation

1.  **Incoming Request**:
    - If an `x-request-id` header is present in the incoming request and is a valid UUID, it is extracted and used.
    - If the `x-request-id` header is present but invalid (e.g., malformed), a new ID is generated, and a warning is logged.
    - If no `x-request-id` header is present, a new UUID v4 is generated for the request.
2.  **Response**: The determined `x-request-id` is always included in the response headers, allowing clients to track their requests.
3.  **Logging**: The `x-request-id` is automatically injected into the `tracing` span associated with the request. This means all log messages emitted during the processing of that request will automatically include the `x-request-id`.

### Example API Interaction

**Client Request:**

```http
GET /api/v1/invoices/123
```

**Server Response:**

```http
HTTP/1.1 200 OK
x-request-id: 1a2b3c4d-5e6f-7890-1234-567890abcdef
Content-Type: application/json

{
  "id": "123",
  "amount": 10000,
  // ...
}
```

**Server Log Output (example using `tracing`):**

```
INFO [2023-10-27T10:00:00Z] request_id=1a2b3c4d-5e6f-7890-1234-567890abcdef: Received GET /api/v1/invoices/123
DEBUG [2023-10-27T10:00:00Z] request_id=1a2b3c4d-5e6f-7890-1234-567890abcdef: Querying database for invoice 123
INFO [2023-10-27T10:00:00Z] request_id=1a2b3c4d-5e6f-7890-1234-567890abcdef: Successfully retrieved invoice 123
```

## Correlation with Indexer Cursor

For debugging scenarios involving the indexer, the `x-request-id` can be correlated with the chain cursor data. This is particularly useful when an API request triggers an action that results in on-chain events, which are then processed by the indexer.

### How it Works

1.  **Transaction Submission**: When an API request (e.g., `POST /api/v1/transactions`) submits a transaction to the Stellar network, the `x-request-id` from the API request can be included in the transaction's metadata (e.g., a transaction memo, or a custom operation field if applicable).
2.  **Indexer Processing**: The QuickLendX indexer monitors the Stellar ledger for new transactions and events. When it processes a transaction that contains an `x-request-id` in its metadata, the indexer extracts this ID.
3.  **Indexer Logging**: The extracted `x-request-id` is then included in the indexer's `tracing` logs alongside the current ledger sequence (chain cursor) it is processing. This creates a direct link between the initial API request and the subsequent indexer activity.

### Example Indexer Log Output

Suppose an API request with `x-request-id: 1a2b3c4d-...` submits a transaction that creates an invoice.

```
INFO [2023-10-27T10:00:05Z] indexer_id=idx-001 request_id=1a2b3c4d-5e6f-7890-1234-567890abcdef ledger_seq=12345678: Processing transaction for invoice creation
DEBUG [2023-10-27T10:00:05Z] indexer_id=idx-001 request_id=1a2b3c4d-5e6f-7890-1234-567890abcdef ledger_seq=12345678: Storing new invoice data in DB
INFO [2023-10-27T10:00:05Z] indexer_id=idx-001 request_id=1a2b3c4d-5e6f-7890-1234-567890abcdef ledger_seq=12345678: Successfully indexed invoice 123
```

This allows developers to search logs for a specific `x-request-id` and see the entire flow, from the initial API call to the indexer's processing of the resulting on-chain events at a particular ledger sequence.

## Security Considerations

- **No User-Controlled IDs Without Validation**: The system validates incoming `x-request-id` headers. If an invalid format is provided, a new ID is generated, preventing malicious or malformed IDs from corrupting logs or internal state.
- **UUID v4**: Uses cryptographically strong pseudo-random numbers for ID generation, minimizing collision risk.
- **Logging Best Practices**: Avoid logging sensitive information directly in the `x-request-id` or its associated metadata.

## Implementation Details

- **Middleware**: An Axum middleware (`request_id_middleware`) intercepts all incoming requests.
- **Extractor**: A custom `FromRequestParts` implementation for `XRequestId` handles ID extraction or generation.
- **Tracing Integration**: The `tracing` crate is used to automatically attach the `x-request-id` to all log events within the request's processing span.
- **Indexer Integration**: The indexer component is responsible for extracting `x-request-id` from transaction metadata (if present) and including it in its own `tracing` spans.

## Testing

Comprehensive tests cover:

- **Header Propagation**: Verifying that `x-request-id` is correctly passed from request to response.
- **ID Generation**: Ensuring new UUIDs are generated when no valid ID is provided.
- **Invalid Input Handling**: Testing behavior when malformed `x-request-id` headers are received.
- **Log Correlation**: Asserting that `x-request-id` appears in log output.
- **Indexer Mocking**: Simulating indexer behavior to confirm `x-request-id` correlation with cursor data.
