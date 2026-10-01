# Off-Chain Signatures in QuickLendX

> **Audience:** Contributors who need to understand which operations involve
> off-chain signatures, how those signatures are structured, what the threat
> model is, and how the contract defends against signature-related attacks.
>
> **Closes:** #1894

---

## Overview

QuickLendX is a Soroban smart contract.  All *on-chain* authorisation uses
Soroban's native `require_auth()` mechanism — the host verifies the
transaction-level Ed25519 (or other wallet) signature before the contract body
runs.  No custom signature-verification code is needed for on-chain callers.

However, several operations rely on **off-chain signatures** — data that is
signed outside the contract and submitted as an argument.  These fall into two
categories:

| Category | Examples |
|----------|---------|
| **KYC payload envelopes** | Encrypted PII blobs signed by a backend service before being written to the chain or passed as `kyc_data` arguments |
| **Webhook and event attestations** | Backend assertions about chain events (replay protection, cursor certificates) |

Understanding the distinction matters because a flaw in off-chain signature
handling can bypass the on-chain `require_auth()` guard.

---

## On-Chain Auth: `require_auth()` (reference)

Every state-changing Soroban entrypoint calls `require_auth()` on the relevant
principal **before** reading or writing storage.  The Soroban host enforces
that the transaction carries a valid authorisation frame for that address.

```rust
// src/contract.rs — store_invoice (simplified)
pub fn store_invoice(env: Env, business: Address, ...) -> Result<...> {
    // Layer 1: Soroban host verifies the tx is signed by `business`
    business.require_auth();

    // Layer 2: KYC gate — business must be Verified
    BusinessVerificationStorage::require_verified(&env, &business)?;
    // ...
}
```

This is **not** an off-chain signature — the host handles it.  It is included
here for completeness so contributors do not mistake the two paths.

---

## Off-Chain Signature Operations

### 1. KYC Payload Submission

**Entrypoints**: `submit_kyc_application`, `submit_investor_kyc`

**What is signed off-chain**: The raw KYC JSON payload (SSN, tax ID, passport
number, bank account details, etc.) is encrypted by the backend before being
passed to the contract as an opaque `Bytes` blob.

**How it works**:

```
Business / Investor
       │
       │  POST /kyc/submit  {plain-text KYC fields}
       ▼
   Backend KYC Service
       │
       │  1. Generate a per-record 32-byte DEK  (crypto.randomBytes)
       │  2. AES-256-GCM encrypt JSON payload with DEK
       │  3. Wrap DEK under the KEK (LocalKeyProvider or AWS KMS)
       │  4. Produce EncryptedRecord:
       │     { ciphertext, authTag, iv, encryptedDek, dekIv, dekAuthTag, keyId }
       │
       ▼
   Stellar Transaction
       │  store_invoice(env, business, ..., kyc_data: Bytes)
       │  submit_kyc_application(env, business, kyc_data: Bytes)
       │
       ▼
   Contract storage  ←── only ciphertext + eDEK persisted; KEK never on-chain
```

The `kyc_data` bytes stored on-chain are the AES-256-GCM ciphertext.  The
**contract does not verify a cryptographic signature over the ciphertext** — it
trusts that the caller (who passed `require_auth()`) is the legitimate submitter.

**Threat model**:

| Threat | Mitigation |
|--------|-----------|
| Attacker submits forged `kyc_data` for a legitimate business | `business.require_auth()` blocks; only the business key-holder can call `submit_kyc_application` |
| Malicious backend replaces ciphertext between encrypt and submit | AES-256-GCM auth tag verification fails on `decrypt()`; tampered records are rejected |
| Database breach exposes `kyc_data` column | Attacker gets ciphertext + eDEK; KEK never stored in DB → cannot decrypt |
| Admin injects KYC for an unverified address | KYC-gate checks run after `require_auth()`; admin cannot bypass the business's own auth |

**Key length and algorithm constants** (from `src/protocol_limits.rs`):

```rust
pub const MAX_KYC_DATA_LENGTH: u32 = 4096; // bytes, after encryption
```

**Reference implementation**: See
[`quicklendx-backend/docs/secrets.md`](../quicklendx-backend/docs/secrets.md)
and [`docs/security/kyc-keys.md`](security/kyc-keys.md) for the full key
hierarchy, rotation procedure, and `EncryptedRecord` schema.

---

### 2. Admin KYC Verification Signature

**Entrypoint**: `verify_business`, `verify_investor`

```rust
// src/verification.rs
pub fn verify_business(env: &Env, admin: &Address, business: &Address) -> Result<(), ...> {
    admin.require_auth();  // on-chain Soroban auth
    BusinessVerificationStorage::verify_business(env, business, admin)?;
    // ...
}
```

The **admin's approval** is an on-chain signature (via `require_auth()`).
There is no additional off-chain attestation; the Stellar transaction signed by
the admin key is the canonical approval record.

**Threat model**: Only the configured admin address can approve KYC.  Rotating
the admin key (via `set_admin` / `propose_new_admin`) does not retroactively
invalidate previously approved KYC records.

---

### 3. Invoice Storage — Dual Signature Requirement

**Entrypoint**: `store_invoice`

This is the most important auth pattern in the codebase because it requires
**two simultaneous on-chain signatures** in one transaction:

```rust
// src/contract.rs
pub fn store_invoice(env: Env, business: Address, ...) {
    // 1. Business must sign the transaction
    business.require_auth();

    // 2. Business must be KYC-verified (admin previously approved)
    BusinessVerificationStorage::require_verified(&env, &business)?;
    // ...
}
```

There is no off-chain signature here, but contributors often ask why the KYC
check is not an off-chain proof.  The design choice is intentional:

- KYC approval is a **persisted on-chain state**, not a token or proof.
- This prevents KYC proof replay across deployments and network forks.
- Revocation (`reject_business`) takes effect immediately on the next call
  without needing proof expiry.

---

### 4. Webhook Cursor Attestations (Backend)

**Location**: Backend off-chain service

The backend event processor maintains a **cursor** (last ingested ledger
sequence).  When resuming after downtime or a reorg, the backend re-verifies
cursor certificates before replaying.

```typescript
// backend/src/services/eventProcessor.ts (conceptual)
interface CursorCertificate {
  ledger:    number;          // last successfully processed ledger
  hash:      string;          // SHA-256 of all event IDs in that ledger
  signature: string;          // HMAC-SHA-256 with CURSOR_SIGNING_KEY
  issued_at: string;          // ISO-8601
}
```

**Threat model**:

| Threat | Mitigation |
|--------|-----------|
| Attacker rewinds cursor to replay stale events | HMAC signature binds cursor to ledger hash; modified ledger → invalid signature |
| Cursor file tampered on disk | HMAC verified on load; tampered file rejected before replay starts |
| HMAC key compromise | Rotate `CURSOR_SIGNING_KEY`; all existing certificates become invalid |

**Implementation note**: The HMAC key is sourced from `CURSOR_SIGNING_KEY`
in the environment.  See
[`backend/docs/replay.md`](../backend/docs/replay.md) and the
[`backend/docs/REPLAY_RUNBOOK.md`](../backend/docs/REPLAY_RUNBOOK.md) for
operational recovery procedures.

---

### 5. Dispute Evidence Submission

**Entrypoint**: `submit_dispute_evidence` / `open_dispute`

Evidence strings are submitted as plain Soroban `String` values — no separate
cryptographic signature is attached to the evidence.  The evidence is linked to
a specific invoice and dispute record by the contract's storage logic.

```rust
// src/dispute.rs
pub fn open_dispute(
    env: Env,
    creator: Address,
    invoice_id: BytesN<32>,
    reason: String,
    evidence: String,
) -> Result<(), QuickLendXError> {
    creator.require_auth();  // on-chain auth only
    // evidence stored as-is; no cryptographic witness
    // ...
}
```

**Threat model**:

| Threat | Mitigation |
|--------|-----------|
| Third party submits evidence on behalf of a party | `creator.require_auth()` — only the business or investor on the invoice can open a dispute |
| Evidence is post-hoc modified | Soroban storage is append-only at the ledger level; a new submission creates a new dispute record |
| Evidence exceeds size budget | `MAX_DISPUTE_EVIDENCE_LENGTH` (from `src/protocol_limits.rs`) enforced at validation |

**Size limit**:

```rust
pub const MAX_DISPUTE_EVIDENCE_LENGTH: u32 = 1024; // characters
pub const MAX_DISPUTE_REASON_LENGTH:   u32 = 256;  // characters
```

---

## Threat Model Summary

The table below covers the full surface area of off-chain or partially
off-chain operations:

| Operation | Off-chain component | Contract-side guard | Key risk |
|-----------|--------------------|--------------------|----------|
| KYC payload submit | AES-256-GCM encrypt by backend | `require_auth()` on submitter | Malformed ciphertext (detected on decrypt) |
| KYC approval | Admin on-chain signature | `admin.require_auth()` + admin equality check | Admin key compromise |
| Invoice store | None (pure on-chain) | `business.require_auth()` + KYC gate | Unverified business bypasses KYC |
| Cursor replay | HMAC-SHA-256 certificate | Verified before replay starts | HMAC key compromise |
| Dispute evidence | None (plain string) | `creator.require_auth()` + party check | Unauthorized dispute opener |
| Webhook delivery | HMAC-SHA-256 (optional) | Consumer must verify before acting | Replay, spoofed payload |

---

## Implementation Notes for Contributors

### Adding a new off-chain-signed operation

1. **Prefer on-chain `require_auth()` over off-chain proofs** when the signer
   is a Stellar keypair — Soroban gives you replay protection and signature
   verification for free.

2. If you must accept an off-chain blob (e.g., encrypted KYC, signed attestation):
   - Store only the ciphertext/MAC, never plaintext secrets.
   - Verify the MAC/authTag before trusting any field from the blob.
   - Bind the signature to the specific contract address and network passphrase
     to prevent cross-deployment replay:

     ```rust
     // Anti-replay: include network context in the signed message
     let context = env.ledger().network_passphrase();
     ```

3. **Length-cap all externally supplied `String` or `Bytes` arguments** using
   constants from `src/protocol_limits.rs`.  Off-chain data that is not
   length-capped can be used as a denial-of-service vector against storage.

4. **Log every off-chain signature verification** to the audit module
   (`src/audit.rs`) so operators can trace approval chains.

### Testing off-chain signature paths

All test files use `env.mock_all_auths()` (or `env.mock_all_signatures()`),
which satisfies every `require_auth()` check unconditionally.  To test that
an entrypoint *correctly rejects* a wrong caller, use a fresh `Address` that
was not passed to `mock_auths`:

```rust
#[test]
fn test_unauthorized_kyc_submit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let attacker = Address::generate(&env);
    let victim   = Address::generate(&env);

    // attacker tries to submit KYC for victim — must fail
    let result = client.try_submit_kyc_application(
        &attacker,
        &String::from_str(&env, "fake KYC"),
    );
    // With mock_all_auths, the require_auth passes; the KYC check gate
    // must then reject because `attacker` != `victim`.
    // Design: each entrypoint must verify the *caller's identity* against
    // the target record, not just that *someone* signed the transaction.
}
```

---

## Related Documentation

- [`docs/security/kyc-keys.md`](security/kyc-keys.md) — KYC key hierarchy,
  encryption algorithm, DEK rotation, and sensitive-field redaction.
- [`docs/INVOICE_LIFECYCLE_DIAGRAM.md`](INVOICE_LIFECYCLE_DIAGRAM.md) — Full
  invoice state machine showing where auth checks occur on each transition.
- [`docs/contracts/verification.md`](contracts/verification.md) — Business and
  investor KYC verification entrypoints.
- [`docs/contracts/access-control.md`](contracts/access-control.md) — Admin
  setup, admin transfer, and role-based gates.
- [`docs/contracts/audit-trail.md`](contracts/audit-trail.md) — How auth events
  are recorded in the on-chain audit log.
- [`backend/docs/replay.md`](../backend/docs/replay.md) — Cursor attestation
  and event-replay runbook.
- [`backend/docs/auth.md`](../backend/docs/auth.md) — Backend JWT / API-key
  authentication model (separate from Stellar transaction auth).
