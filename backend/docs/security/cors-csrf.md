# CORS and CSRF Security Policy

This document outlines the Cross-Origin Resource Sharing (CORS) and Cross-Site Request Forgery (CSRF) protection mechanisms for the QuickLendX protocol backend. 

The backend architecture distinguishes between different request surfaces to balance security with interoperability for machine-to-machine communications.

## 1. Request Surfaces

The backend exposes two primary request surfaces:

1. **Browser-driven API (`/api/v1`)**: Used by frontend applications accessed by users.
2. **Machine-to-machine Webhooks (`/api/webhooks`, `/api/v1/webhooks/ingest`)**: Used for background service communication and third-party integrations.

Each surface has distinct CORS configurations and CSRF protection requirements.

## 2. CORS Configuration

We enforce a strict allow-list-driven CORS policy for browser interactions while maintaining permissive constraints for webhook endpoints.

### Browser-Driven Routes
* **Options**: Defined in `browserCorsOptions`.
* **Allowed Origins**: Governed by the `ALLOWED_ORIGINS` environment variable. Preflight `OPTIONS` requests and actual CORS requests are evaluated dynamically. If the origin matches the allow-list (or if `*` is explicitly enabled in the allow-list for development), the origin is reflected in the `Access-Control-Allow-Origin` header.
* **Credentials**: `Access-Control-Allow-Credentials: true` is set for allowed origins to support standard authenticated sessions if required in the future.
* **Allowed Headers**: Includes standard headers alongside our custom `X-CSRF-Token`.

### Webhook Routes
* **Options**: Defined in `webhookCorsOptions`.
* **Allowed Origins**: `*` (Wildcard). Webhook callbacks and ingests are intentionally origin-agnostic since they are executed by backend services, not browsers.
* **Credentials**: `Access-Control-Allow-Credentials: false`.
* **Allowed Headers**: Specific to webhook verification requirements (e.g., `X-Webhook-Signature`, `X-Webhook-Subscriber-Id`).

## 3. CSRF Protection Requirements

The `csrfMiddleware` evaluates all incoming state-changing HTTP requests (`POST`, `PUT`, `PATCH`, `DELETE`).

### API Key and Webhook Exemptions
CSRF protection is inherently a browser-security concept. Consequently, the following machine-to-machine surfaces are entirely **exempt** from CSRF validation:

* **Webhook Ingress/Callbacks**: Any path starting with `/api/webhooks` or containing `/webhooks/ingest` is exempted. These routes enforce security using HMAC signatures (`X-Webhook-Signature`).
* **API Key Authenticated Routes**: Any request that authenticates using an API key (providing the `X-API-Key` header or an `Authorization` header starting with `Bearer qlx_`) is treated as a server-to-server request and bypassed by the CSRF middleware.

### Browser-Driven Write Operations
For any non-exempt, state-changing request (i.e., a standard browser-driven interaction), the following strict conditions MUST be met:

1. **Origin Verification**: If the request contains an `Origin` header, it must be explicitly present in the `ALLOWED_ORIGINS` list.
2. **Custom CSRF Token**: The request must include the `X-CSRF-Token` HTTP header. Due to Same-Origin Policy (SOP), malicious external websites cannot append custom HTTP headers to cross-origin requests. The presence of this header acts as a stateless, robust defense against CSRF attacks.
3. **Content-Type Restriction**: The `Content-Type` header must be explicitly set to `application/json`. This mitigates simple form-based CSRF attacks which typically use `application/x-www-form-urlencoded` or `multipart/form-data`.

## 4. Helmet Security Headers
The system uses `helmet()` to enforce standard security headers (like HSTS, Content-Security-Policy, and X-Content-Type-Options). These headers are applied globally and remain intact irrespective of the CORS/CSRF surface evaluation.
