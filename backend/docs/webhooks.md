# Webhooks

## Overview

Server-initiated HTTP callbacks (“webhooks”) are a common integration pattern. Because the backend chooses the URL and often runs in a trusted network context, **webhook delivery is treated as untrusted egress**: the remote host, DNS, TLS stack, and response body must not be able to probe internal services or exfiltrate data via SSRF.

## Security controls

The implementation in `src/services/webhook/` enforces:

1. **HTTPS only** — Only `https:` targets are accepted. Other schemes are rejected before any network I/O.
2. **No URL credentials** — Userinfo (`https://user:pass@host/...`) is rejected to avoid leaking secrets and to simplify policy.
3. **Host allow/deny policy** — Optional allowlist (`WEBHOOK_HOST_ALLOWLIST`) and extra deny patterns (`WEBHOOK_HOST_DENYLIST`). Built-in deny rules always block obvious local and cloud-metadata names (for example `localhost`, `metadata.google.internal`, `metadata.google`) and `*.local` mDNS-style hosts.
4. **Private and reserved IP blocking** — Literal IPs in the URL are checked without DNS. For hostnames, **every** address returned by `dns.lookup(..., { all: true })` must be a public unicast address before a connection is made. That mitigates **DNS rebinding** where some records point at the public internet and others at RFC1918 space.
5. **Redirect limits** — Redirect responses are only followed up to `WEBHOOK_MAX_REDIRECTS` (default `3`). Each redirect target is re-validated (scheme, host policy, DNS checks on the next hop).
6. **Timeouts** — `WEBHOOK_TIMEOUT_MS` (default `10000`) is applied per HTTP request via the Node HTTPS client socket timeout.
7. **Response size cap** — The response body is read up to `WEBHOOK_MAX_RESPONSE_BYTES` (default `65536`); larger streams abort the request.

### Environment variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `WEBHOOK_HOST_ALLOWLIST` | _(empty = no host allow restriction)_ | Comma-separated host patterns. Use `hooks.slack.com` for exact match, or `*.example.com` to allow one DNS label under `example.com`. |
| `WEBHOOK_HOST_DENYLIST` | _(empty)_ | Extra comma-separated hostnames or suffixes to block. |
| `WEBHOOK_MAX_REDIRECTS` | `3` | Maximum number of HTTP redirects to follow. |
| `WEBHOOK_TIMEOUT_MS` | `10000` | Per-request socket timeout in milliseconds. |
| `WEBHOOK_MAX_RESPONSE_BYTES` | `65536` | Maximum webhook response body size to buffer. |

### Assumptions

- **Delivery network is untrusted** — Attackers who can influence the webhook URL (configuration, database, or compromise) must not reach loopback, RFC1918, link-local, CGNAT, documentation, or cloud metadata endpoints through this client.
- **TLS** — Standard Node TLS verification applies (system CA store). Pinning or mTLS is out of scope for this module but can be layered by callers if required.

## Usage

```typescript
import {
  deliverWebhookJson,
  loadWebhookEgressPolicyFromEnv,
} from "./services/webhook";

const policy = loadWebhookEgressPolicyFromEnv();

await deliverWebhookJson("https://hooks.example.com/path", { event: "paid" }, policy);
```

Errors are thrown as `WebhookUrlValidationError` (pre-connect validation) or `WebhookDeliveryError` (network, DNS egress block, redirect limit, timeout, or oversized response).

## Tests

SSRF-oriented tests live in `tests/webhook-ssrf.test.ts` alongside redirect-abuse and policy cases. Run:

```bash
cd backend && npm test
```
