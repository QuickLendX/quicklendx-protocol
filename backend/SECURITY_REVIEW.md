# Backend Security Review - Status Endpoint

## Status Endpoint (`GET /api/status`)
- **Visibility**: Public
- **Information Exposure**: Minimal. Only public blockchain heights and high-level health flags are exposed.
- **Cache Poisoning**: Protected by standard caching headers. The response depends only on internal service state, not user input.
- **DDoS Risk**: Lightweight computation. Safe for high-frequency polling when cached.

## Maintenance Control (`POST /api/admin/maintenance`)
- **Visibility**: Internal/Private
- **Current Implementation**: Unprotected placeholder.
- **Recommendation**: This endpoint MUST be protected by:
  1. Internal network access only (VPN/VPC).
  2. Strong Authentication (Bearer token, API Key).
  3. Rate limiting.
  4. Audit logging for every state change.

## Security Assumptions Validated
- [x] No sensitive internals (DB strings, keys) exposed in `/api/status`.
- [x] No PII exposed.
- [x] Version string is generic.
- [x] Ledger information is already public on the blockchain.
