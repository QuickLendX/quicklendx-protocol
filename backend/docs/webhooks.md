# Webhook Retry & Backoff Policy

## Overview
Failed webhook deliveries are retried with exponential backoff and jitter
before being handed off to the dead-letter workflow.

## Retry Policy
| Parameter      | Default  | Description                        |
|----------------|----------|------------------------------------|
| maxAttempts    | 5        | Maximum delivery attempts          |
| initialDelayMs | 500ms    | Base delay before first retry      |
| maxDelayMs     | 30,000ms | Maximum delay cap between retries  |

## Backoff Formula
delay = min(initialDelayMs * 2^attempt + jitter, maxDelayMs)

Jitter is a random value between 0 and the base delay to prevent retry storms.

## Retry Conditions
- **Retried:** TIMEOUT, TRANSPORT_ERROR, 5xx responses, 429 Too Many Requests
- **Not retried:** 4xx responses (except 429), URL_INVALID, non-retryable errors

## Dead-Letter Handoff
An event is dead-lettered when:
- Max attempts are exhausted
- A permanent 4xx response is received
- A non-retryable error occurs

## Security
SSRF protections in urlValidation.ts and egressPolicy.ts are enforced on every retry attempt.
