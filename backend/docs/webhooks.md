# Webhook Secret Rotation

This document describes the per-subscriber webhook signing system and the
**dual-verify rotation workflow** that enables zero-downtime secret rollover
for integrators.

---

## Overview

Every subscriber receives a unique HMAC-SHA256 signing secret.  Outgoing
webhook events are signed with that secret; incoming events (sent by the
subscriber back to the platform) are verified against it.

Secrets are **never** returned after initial registration, never logged, and
never included in error responses.

---

## Signing Algorithm

```
signature = "sha256=" + HMAC-SHA256(secret_bytes, raw_request_body_bytes)
```

- `secret_bytes` – the subscriber's active secret decoded from hex.
- `raw_request_body_bytes` – the exact bytes of the HTTP request body
  (before any JSON parsing).  Byte-for-byte fidelity is required.

The computed signature is placed in the `X-Webhook-Signature` header.

---

## API Reference

All endpoints are under `/api/v1/webhooks`.

### Register a subscriber

```
POST /api/v1/webhooks/subscribers
```

**Request body**

| Field                  | Type    | Required | Description                                      |
|------------------------|---------|----------|--------------------------------------------------|
| `subscriber_id`        | string  | ✓        | Unique identifier for the subscriber (≤128 chars)|
| `grace_period_seconds` | integer |          | Default dual-verify window (60–86400, default 3600)|

**Response `201`**

```json
{
  "subscriber_id": "acme-corp",
  "status": "active",
  "has_pending_secret": false,
  "grace_period_seconds": 3600,
  "initial_secret": "a3f8...c2d1",
  "created_at": "2026-04-23T10:00:00.000Z",
  "updated_at": "2026-04-23T10:00:00.000Z"
}
```

> ⚠️ `initial_secret` is returned **once only**.  Store it in a secrets
> manager immediately.  It cannot be retrieved again.

---

### Get subscriber state

```
GET /api/v1/webhooks/subscribers/:subscriberId
```

Returns the public rotation state.  Secrets are never included.

**Response `200`**

```json
{
  "subscriber_id": "acme-corp",
  "status": "active",
  "has_pending_secret": false,
  "pending_created_at": null,
  "grace_period_seconds": 3600,
  "created_at": "2026-04-23T10:00:00.000Z",
  "updated_at": "2026-04-23T10:00:00.000Z"
}
```

`status` is one of:

| Value      | Meaning                                                  |
|------------|----------------------------------------------------------|
| `active`   | Only the primary secret is in use.                       |
| `rotating` | A pending secret exists; both secrets are accepted.      |

---

## Secret Rotation

### Why rotate?

Periodic rotation limits the blast radius of a compromised secret.  The
dual-verify window ensures integrators can update their signing key without
any dropped events.

### Rotation workflow

```
┌─────────────────────────────────────────────────────────────────┐
│  Step 1 – Initiate                                              │
│  POST /subscribers/:id/rotate                                   │
│                                                                 │
│  • Generates a new pending secret.                              │
│  • Status → "rotating".                                         │
│  • Both primary (old) and pending (new) secrets are accepted    │
│    for the configured grace period.                             │
│  • new_secret returned ONCE – store it immediately.             │
└────────────────────────────┬────────────────────────────────────┘
                             │
                    grace window open
                    (both keys valid)
                             │
          ┌──────────────────┴──────────────────┐
          │                                     │
          ▼                                     ▼
┌─────────────────────┐             ┌─────────────────────────┐
│  Step 2a – Finalize │             │  Step 2b – Cancel       │
│  POST …/finalize    │             │  POST …/cancel          │
│                     │             │                         │
│  • pending → primary│             │  • Discards pending.    │
│  • Old secret gone. │             │  • Reverts to primary.  │
│  • Status → active. │             │  • Status → active.     │
└─────────────────────┘             └─────────────────────────┘
```

#### Step 1 – Initiate rotation

```
POST /api/v1/webhooks/subscribers/:subscriberId/rotate
```

**Optional request body**

```json
{ "grace_period_seconds": 3600 }
```

**Response `202`**

```json
{
  "subscriber_id": "acme-corp",
  "status": "rotating",
  "new_secret": "7b2e...f901",
  "grace_period_seconds": 3600,
  "pending_created_at": "2026-04-23T11:00:00.000Z"
}
```

> ⚠️ `new_secret` is returned **once only**.

#### Step 2a – Finalize rotation

Call this once your integrator has updated their signing key to the new secret.

```
POST /api/v1/webhooks/subscribers/:subscriberId/rotate/finalize
```

**Response `200`**

```json
{
  "subscriber_id": "acme-corp",
  "status": "active",
  "message": "Rotation finalized. The new secret is now the only accepted signing key."
}
```

#### Step 2b – Cancel rotation

Call this to abort a rotation and revert to the primary secret only.

```
POST /api/v1/webhooks/subscribers/:subscriberId/rotate/cancel
```

**Response `200`** – returns the public subscriber view with `status: "active"`.

---

### Grace period behaviour

| Scenario                                    | Result                                      |
|---------------------------------------------|---------------------------------------------|
| Signature computed with **primary** secret  | ✅ Accepted (`matched_secret: "primary"`)   |
| Signature computed with **pending** secret  | ✅ Accepted (`matched_secret: "pending"`)   |
| Grace period elapses without finalization   | Pending auto-promoted to primary (lazy)     |
| Rotation finalized                          | Only new (now primary) secret accepted      |
| Rotation cancelled                          | Only original primary secret accepted       |

---

## Ingest endpoint

```
POST /api/v1/webhooks/ingest/:subscriberId
```

Protected by HMAC-SHA256 signature verification.  Required headers:

| Header                    | Description                                      |
|---------------------------|--------------------------------------------------|
| `X-Webhook-Subscriber-Id` | The subscriber's identifier.                     |
| `X-Webhook-Signature`     | `sha256=<hmac-hex>` of the raw request body.     |

**Success `200`**

```json
{
  "received": true,
  "subscriber_id": "acme-corp",
  "matched_secret": "primary"
}
```

**Error responses**

| Status | Code                          | Cause                                      |
|--------|-------------------------------|--------------------------------------------|
| 400    | `MISSING_SUBSCRIBER_HEADER`   | `X-Webhook-Subscriber-Id` header absent.   |
| 400    | `MISSING_SIGNATURE_HEADER`    | `X-Webhook-Signature` header absent.       |
| 401    | `INVALID_WEBHOOK_SIGNATURE`   | Signature mismatch or unknown subscriber.  |

> **Security note:** Unknown subscribers return `401` (not `404`) to prevent
> subscriber enumeration.

---

## Security considerations

- Secrets are generated with Node.js `crypto.randomBytes(32)` (256-bit entropy).
- Verification uses `crypto.timingSafeEqual` to prevent timing oracle attacks.
- Secrets are never logged, never included in error responses, and never
  returned after the initial registration / rotation initiation call.
- The grace period defaults to 1 hour and is capped at 24 hours.
- If `finalizeRotation` is never called, the pending secret is automatically
  promoted to primary once the grace period elapses (lazy expiry on next
  verification call).

---

## Example: integrator rotation walkthrough

```bash
# 1. Register
curl -X POST /api/v1/webhooks/subscribers \
  -H "Content-Type: application/json" \
  -d '{"subscriber_id":"acme"}'
# → save initial_secret as OLD_SECRET

# 2. Initiate rotation
curl -X POST /api/v1/webhooks/subscribers/acme/rotate \
  -H "Content-Type: application/json" \
  -d '{"grace_period_seconds":3600}'
# → save new_secret as NEW_SECRET

# 3. Update your signing code to use NEW_SECRET
#    (OLD_SECRET still works during the grace window)

# 4. Finalize once all your services are using NEW_SECRET
curl -X POST /api/v1/webhooks/subscribers/acme/rotate/finalize
# → OLD_SECRET is now invalid
```
