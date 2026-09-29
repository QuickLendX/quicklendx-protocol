# KYC Key Hierarchy

## Overview

KYC payloads contain the most sensitive PII in QuickLendX (SSN, tax ID, passport numbers, bank account details). They are protected with **envelope encryption**: a per-record Data-Encryption Key (DEK) encrypts the payload, and a Key-Encryption Key (KEK) wraps the DEK. Only the wrapped DEK and ciphertext are persisted — no plaintext key material is ever stored or logged.

---

## Key Hierarchy

```
KEK  (Key-Encryption Key)
 └── held by KeyProvider (LocalKeyProvider or KmsKeyProvider)
 └── never leaves the provider; used only to wrap/unwrap DEKs

DEK  (Data-Encryption Key)
 └── 32 random bytes generated per KYC record (crypto.randomBytes)
 └── used to AES-256-GCM encrypt the JSON payload
 └── zeroed from memory immediately after use

eDEK (Encrypted DEK)
 └── DEK wrapped by the KEK (AES-256-GCM or KMS opaque blob)
 └── stored alongside the ciphertext in EncryptedRecord
 └── safe to persist; useless without the KEK
```

---

## Encryption Algorithm

| Layer | Algorithm | Key size | IV | Auth tag |
|-------|-----------|----------|----|----------|
| Payload | AES-256-GCM | 256-bit DEK | 96-bit random | 128-bit |
| DEK wrap (local) | AES-256-GCM | 256-bit KEK | 96-bit random | 128-bit |
| DEK wrap (KMS) | KMS-managed (AES-256) | KMS-managed | KMS-managed | KMS-managed |

---

## KeyProvider Interface

```typescript
interface KeyProvider {
  currentKeyId(): string;
  wrapKey(dek: Buffer, keyId: string): Promise<{ encryptedDek, iv, authTag }>;
  unwrapKey(encryptedDek, iv, authTag, keyId): Promise<Buffer>;
}
```

Two implementations ship out of the box:

### LocalKeyProvider

- Reads a 64-character hex KEK from the `KYC_KEK_HEX` environment variable.
- Suitable for development and testing.
- **Never use in production without a secrets manager.**

```
KYC_KEK_HEX=<64 hex chars>   # 32-byte AES-256 key
```

### KmsKeyProvider

- Delegates wrapping to AWS KMS via `GenerateDataKey` / `Decrypt`.
- The KMS key ARN is passed at construction time.
- Requires IAM permissions: `kms:GenerateDataKey`, `kms:Decrypt`.
- The KMS CiphertextBlob is stored as the `encryptedDek`; iv/authTag are empty (KMS handles its own authenticated encryption internally).

```typescript
const provider = new KmsKeyProvider(kmsClient, "arn:aws:kms:us-east-1:123456789:key/abc-def");
```

---

## EncryptedRecord Schema

```typescript
interface EncryptedRecord {
  ciphertext:   string;  // base64 AES-256-GCM ciphertext of JSON payload
  authTag:      string;  // base64 GCM auth tag for payload
  iv:           string;  // base64 96-bit IV for payload
  encryptedDek: string;  // base64 wrapped DEK (local: AES ciphertext; KMS: CiphertextBlob)
  dekIv:        string;  // base64 IV for DEK wrap (empty for KMS)
  dekAuthTag:   string;  // base64 auth tag for DEK wrap (empty for KMS)
  keyId:        string;  // opaque identifier of the KEK used
}
```

---

## Key Rotation

Rotation re-wraps the DEK under a new KEK **without decrypting the payload**. Plaintext PII is never exposed during rotation.

```typescript
// 1. Encrypt with old provider
const service = new KycService(oldProvider);
const record = await service.encrypt(payload);

// 2. Rotate DEK to new provider (ciphertext unchanged)
const rotated = await service.rotateKey(record, newProvider);

// 3. Persist rotated record; old record can be discarded
```

After rotation:
- `rotated.ciphertext`, `authTag`, and `iv` are identical to the original (PII untouched).
- `rotated.encryptedDek`, `dekIv`, `dekAuthTag`, and `keyId` reflect the new KEK.
- The original record remains decryptable with the old provider until explicitly deleted.

---

## Sensitive Fields Redaction

The following fields are automatically redacted to `"[REDACTED]"` on every `decrypt()` call, regardless of caller. All fields are set unconditionally — callers cannot distinguish "field was present" from "field was absent".

**camelCase (new API):**
- `ssn`
- `taxId`
- `dateOfBirth`
- `passportNumber`
- `bankAccountNumber`
- `routingNumber`

**snake_case (legacy API — preserved for backward compatibility):**
- `tax_id`, `customer_name`, `customer_address`, `date_of_birth`
- `passport_number`, `national_id`, `phone_number`, `email`
- `bank_account`, `kyc_document`, `kyc_data`

This ensures that even authorised callers receive a safe view. Raw values are only accessible inside the encryption boundary.

---

## Access Logging

Every `encrypt`, `decrypt`, and `rotate` operation appends an entry to the in-memory access log:

```typescript
interface AccessLogEntry {
  userId:    string;   // subject of the KYC record
  action:    "encrypt" | "decrypt" | "rotate";
  keyId:     string;   // KEK identifier
  timestamp: string;   // ISO-8601
}
```

The log **never** contains DEK material, KEK material, or plaintext PII. In production, ship log entries to your SIEM/audit trail before the process exits.

---

## Security Invariants

1. DEKs are zeroed (`Buffer.fill(0)`) immediately after use.
2. No key material or plaintext PII appears in log output.
3. Each record uses a unique random DEK and IV — no key/IV reuse.
4. GCM auth tags are verified before any plaintext is returned; tampered records throw.
5. The `KeyProvider` interface is the only seam for key material — no raw key bytes flow through `KycService`.

---

## Threat Model

| Threat | Mitigation |
|--------|-----------|
| Database breach | Attacker gets ciphertext + eDEK but not the KEK → cannot decrypt |
| Log exfiltration | No key material or PII in logs |
| Key compromise | Rotate DEKs to a new KEK without re-encrypting payloads |
| Ciphertext tampering | AES-256-GCM auth tag verification rejects tampered records |
| IV reuse | `crypto.randomBytes(12)` per operation; collision probability negligible |
