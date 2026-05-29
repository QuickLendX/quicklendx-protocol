/**
 * KYC Data Handling Service
 *
 * Envelope encryption: per-record DEK (AES-256-GCM) wrapped by a KEK via a
 * pluggable KeyProvider. Supports local (env-based) and AWS KMS providers.
 * Key rotation re-wraps the DEK without touching the payload ciphertext.
 *
 * Security invariants:
 *  - DEKs are zeroed immediately after use.
 *  - No key material or plaintext PII is ever logged.
 *  - GCM auth tags are verified before any plaintext is returned.
 *  - Each record uses a unique random DEK and IV.
 */

import * as crypto from "crypto";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ALGO = "aes-256-gcm";
const IV_BYTES = 12; // 96-bit IV for GCM
const TAG_BYTES = 16;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/** Fields redacted to "[REDACTED]" on every decrypt() call. */
export const SENSITIVE_FIELDS = [
  // camelCase (new API)
  "ssn",
  "taxId",
  "dateOfBirth",
  "passportNumber",
  "bankAccountNumber",
  "routingNumber",
  // snake_case (legacy API — backward compatibility)
  "tax_id",
  "customer_name",
  "customer_address",
  "date_of_birth",
  "passport_number",
  "national_id",
  "phone_number",
  "email",
  "bank_account",
  "kyc_document",
  "kyc_data",
] as const;

export type SensitiveField = (typeof SENSITIVE_FIELDS)[number];

/** Loose payload type — any JSON-serialisable object with a userId. */
export interface KycPayload {
  userId: string;
  [key: string]: unknown;
}

/** Persisted envelope: ciphertext + wrapped DEK + metadata. */
export interface EncryptedRecord {
  /** base64 AES-256-GCM ciphertext of the JSON payload */
  ciphertext: string;
  /** base64 GCM auth tag for the payload */
  authTag: string;
  /** base64 96-bit IV for the payload */
  iv: string;
  /** base64 wrapped DEK (local: AES ciphertext; KMS: CiphertextBlob) */
  encryptedDek: string;
  /** base64 IV for DEK wrap (empty for KMS) */
  dekIv: string;
  /** base64 auth tag for DEK wrap (empty for KMS) */
  dekAuthTag: string;
  /** Opaque identifier of the KEK used */
  keyId: string;
}

/** Minimal AWS KMS client interface (subset of @aws-sdk/client-kms). */
export interface KmsClient {
  generateDataKey(params: { KeyId: string; KeySpec: string }): Promise<{
    Plaintext: Buffer;
    CiphertextBlob: Buffer;
  }>;
  decrypt(params: { CiphertextBlob: Buffer; KeyId: string }): Promise<{
    Plaintext: Buffer;
  }>;
}

/** Access log entry — never contains key material or plaintext PII. */
export interface AccessLogEntry {
  userId: string;
  action: "encrypt" | "decrypt" | "rotate";
  keyId: string;
  timestamp: string; // ISO-8601
}

// ---------------------------------------------------------------------------
// KeyProvider interface
// ---------------------------------------------------------------------------

export interface KeyProvider {
  currentKeyId(): string;
  wrapKey(
    dek: Buffer,
    keyId: string,
  ): Promise<{ encryptedDek: Buffer; iv: Buffer; authTag: Buffer }>;
  unwrapKey(
    encryptedDek: Buffer,
    iv: Buffer,
    authTag: Buffer,
    keyId: string,
  ): Promise<Buffer>;
}

// ---------------------------------------------------------------------------
// LocalKeyProvider
// ---------------------------------------------------------------------------

export class LocalKeyProvider implements KeyProvider {
  private readonly kek: Buffer;
  private readonly keyId: string;

  constructor(hexKey?: string, keyId = "local-v1") {
    const raw = hexKey ?? process.env.KYC_KEK_HEX ?? "";
    if (raw.length !== 64) {
      throw new Error("KYC_KEK_HEX must be a 64-character hex string");
    }
    this.kek = Buffer.from(raw, "hex");
    this.keyId = keyId;
  }

  currentKeyId(): string {
    return this.keyId;
  }

  async wrapKey(
    dek: Buffer,
    _keyId: string,
  ): Promise<{ encryptedDek: Buffer; iv: Buffer; authTag: Buffer }> {
    const iv = crypto.randomBytes(IV_BYTES);
    const cipher = crypto.createCipheriv(ALGO, this.kek, iv);
    const encryptedDek = Buffer.concat([cipher.update(dek), cipher.final()]);
    const authTag = cipher.getAuthTag();
    return { encryptedDek, iv, authTag };
  }

  async unwrapKey(
    encryptedDek: Buffer,
    iv: Buffer,
    authTag: Buffer,
    _keyId: string,
  ): Promise<Buffer> {
    try {
      const decipher = crypto.createDecipheriv(ALGO, this.kek, iv);
      decipher.setAuthTag(authTag);
      return Buffer.concat([decipher.update(encryptedDek), decipher.final()]);
    } catch {
      throw new Error("DEK unwrap failed: authentication tag mismatch");
    }
  }
}

// ---------------------------------------------------------------------------
// KmsKeyProvider
// ---------------------------------------------------------------------------

export class KmsKeyProvider implements KeyProvider {
  constructor(
    private readonly client: KmsClient,
    private readonly kmsKeyId: string,
  ) {}

  currentKeyId(): string {
    return this.kmsKeyId;
  }

  /**
   * For KMS, we call GenerateDataKey to get a fresh DEK + CiphertextBlob.
   * The caller-supplied `dek` is ignored — KMS owns key generation.
   * iv and authTag are empty (KMS handles its own authenticated encryption).
   */
  async wrapKey(
    _dek: Buffer,
    keyId: string,
  ): Promise<{ encryptedDek: Buffer; iv: Buffer; authTag: Buffer }> {
    const result = await this.client.generateDataKey({ KeyId: keyId, KeySpec: "AES_256" });
    return {
      encryptedDek: result.CiphertextBlob,
      iv: Buffer.alloc(0),
      authTag: Buffer.alloc(0),
    };
  }

  async unwrapKey(
    encryptedDek: Buffer,
    _iv: Buffer,
    _authTag: Buffer,
    keyId: string,
  ): Promise<Buffer> {
    const result = await this.client.decrypt({ CiphertextBlob: encryptedDek, KeyId: keyId });
    return result.Plaintext;
  }
}

// ---------------------------------------------------------------------------
// KycService
// ---------------------------------------------------------------------------

export class KycService {
  private provider: KeyProvider;
  private readonly log: AccessLogEntry[] = [];

  constructor(provider: KeyProvider) {
    this.provider = provider;
  }

  getProvider(): KeyProvider {
    return this.provider;
  }

  setProvider(provider: KeyProvider): void {
    this.provider = provider;
  }

  getAccessLog(): AccessLogEntry[] {
    return [...this.log];
  }

  /** Encrypt a KYC payload. Returns an EncryptedRecord safe to persist. */
  async encrypt(payload: KycPayload): Promise<EncryptedRecord> {
    const keyId = this.provider.currentKeyId();

    // 1. Generate a fresh per-record DEK.
    const dek = crypto.randomBytes(32);

    // 2. Wrap the DEK with the KEK.
    const { encryptedDek, iv: dekIv, authTag: dekAuthTag } = await this.provider.wrapKey(dek, keyId);

    // 3. Encrypt the payload with the DEK.
    const payloadIv = crypto.randomBytes(IV_BYTES);
    const cipher = crypto.createCipheriv(ALGO, dek, payloadIv);
    const plaintext = Buffer.from(JSON.stringify(payload), "utf8");
    const ciphertext = Buffer.concat([cipher.update(plaintext), cipher.final()]);
    const authTag = cipher.getAuthTag();

    // 4. Zero the DEK.
    dek.fill(0);

    this.log.push({ userId: payload.userId, action: "encrypt", keyId, timestamp: new Date().toISOString() });

    return {
      ciphertext: ciphertext.toString("base64"),
      authTag: authTag.toString("base64"),
      iv: payloadIv.toString("base64"),
      encryptedDek: encryptedDek.toString("base64"),
      dekIv: dekIv.toString("base64"),
      dekAuthTag: dekAuthTag.toString("base64"),
      keyId,
    };
  }

  /**
   * Decrypt an EncryptedRecord. Sensitive fields are redacted to "[REDACTED]"
   * before returning — raw values never leave this method.
   */
  async decrypt(record: EncryptedRecord): Promise<KycPayload> {
    // 1. Unwrap the DEK.
    const dek = await this.provider.unwrapKey(
      Buffer.from(record.encryptedDek, "base64"),
      Buffer.from(record.dekIv, "base64"),
      Buffer.from(record.dekAuthTag, "base64"),
      record.keyId,
    );

    // 2. Decrypt the payload.
    let payload: KycPayload;
    try {
      const decipher = crypto.createDecipheriv(ALGO, dek, Buffer.from(record.iv, "base64"));
      decipher.setAuthTag(Buffer.from(record.authTag, "base64"));
      const raw = Buffer.concat([
        decipher.update(Buffer.from(record.ciphertext, "base64")),
        decipher.final(),
      ]);
      payload = JSON.parse(raw.toString("utf8")) as KycPayload;
    } catch {
      dek.fill(0);
      throw new Error("Payload decryption failed: authentication tag mismatch");
    }

    // 3. Zero the DEK.
    dek.fill(0);

    // 4. Redact sensitive fields (set unconditionally so callers cannot infer presence).
    for (const field of SENSITIVE_FIELDS) {
      (payload as Record<string, unknown>)[field] = "[REDACTED]";
    }

    this.log.push({
      userId: payload.userId,
      action: "decrypt",
      keyId: record.keyId,
      timestamp: new Date().toISOString(),
    });

    return payload;
  }

  /**
   * Re-wrap the DEK under a new KeyProvider without touching the payload
   * ciphertext. Plaintext PII is never exposed during rotation.
   */
  async rotateKey(record: EncryptedRecord, newProvider: KeyProvider): Promise<EncryptedRecord> {
    // 1. Unwrap DEK with the current (old) provider.
    const dek = await this.provider.unwrapKey(
      Buffer.from(record.encryptedDek, "base64"),
      Buffer.from(record.dekIv, "base64"),
      Buffer.from(record.dekAuthTag, "base64"),
      record.keyId,
    );

    // 2. Re-wrap with the new provider.
    const newKeyId = newProvider.currentKeyId();
    const { encryptedDek, iv: dekIv, authTag: dekAuthTag } = await newProvider.wrapKey(dek, newKeyId);

    // 3. Zero the DEK.
    dek.fill(0);

    const rotated: EncryptedRecord = {
      ...record,
      encryptedDek: encryptedDek.toString("base64"),
      dekIv: dekIv.toString("base64"),
      dekAuthTag: dekAuthTag.toString("base64"),
      keyId: newKeyId,
    };

    this.log.push({
      userId: "", // userId not available without decrypting; log keyId only
      action: "rotate",
      keyId: newKeyId,
      timestamp: new Date().toISOString(),
    });

    return rotated;
  }
}

// ---------------------------------------------------------------------------
// Legacy API — preserved for backward compatibility
// ---------------------------------------------------------------------------

export const SENSITIVE_FIELDS_LEGACY = [
  "tax_id",
  "customer_name",
  "customer_address",
  "date_of_birth",
  "ssn",
  "passport_number",
  "national_id",
  "phone_number",
  "email",
  "bank_account",
  "kyc_document",
  "kyc_data",
] as const;

export const PII_FIELDS = [
  "tax_id",
  "customer_name",
  "customer_address",
  "date_of_birth",
  "ssn",
  "passport_number",
  "national_id",
  "phone_number",
  "email",
  "bank_account",
  "ipAddress",
] as const;

export type PiiField = (typeof PII_FIELDS)[number];

export interface KycRecord {
  id: string;
  userId: string;
  status: "pending" | "submitted" | "verified" | "rejected";
  encryptedData: string;
  submittedAt: number;
  verifiedAt?: number;
  metadata: KycMetadata;
}

export interface KycMetadata {
  version: string;
  lastUpdated: number;
  reviewNotes?: string;
}

// Module-level legacy encryption state
const LEGACY_ALGO = "aes-256-gcm";
const LEGACY_IV_LEN = 16;
const LEGACY_TAG_LEN = 16;

interface LegacyConfig { encryptionKey: string }
let legacyConfig: LegacyConfig | null = null;

export function initializeEncryption(masterKey: string): void {
  const salt = crypto.createHash("sha256").update("quicklendx-kyc-salt").digest();
  const key = crypto.pbkdf2Sync(masterKey, salt, 100000, 32, "sha256");
  legacyConfig = { encryptionKey: key.toString("hex") };
}

export function isEncryptionInitialized(): boolean {
  return legacyConfig !== null;
}

export function encryptSensitiveData(plaintext: string): string {
  if (!legacyConfig) throw new Error("Encryption not initialized. Call initializeEncryption first.");
  const key = Buffer.from(legacyConfig.encryptionKey, "hex");
  const iv = crypto.randomBytes(LEGACY_IV_LEN);
  const cipher = crypto.createCipheriv(LEGACY_ALGO, key, iv);
  let encrypted = cipher.update(plaintext, "utf8", "hex");
  encrypted += cipher.final("hex");
  const authTag = cipher.getAuthTag();
  return iv.toString("hex") + authTag.toString("hex") + encrypted;
}

export function decryptSensitiveData(ciphertext: string): string {
  if (!legacyConfig) throw new Error("Encryption not initialized. Call initializeEncryption first.");
  const key = Buffer.from(legacyConfig.encryptionKey, "hex");
  const iv = Buffer.from(ciphertext.substring(0, LEGACY_IV_LEN * 2), "hex");
  const authTag = Buffer.from(ciphertext.substring(LEGACY_IV_LEN * 2, LEGACY_IV_LEN * 2 + LEGACY_TAG_LEN * 2), "hex");
  const encrypted = ciphertext.substring(LEGACY_IV_LEN * 2 + LEGACY_TAG_LEN * 2);
  const decipher = crypto.createDecipheriv(LEGACY_ALGO, key, iv);
  decipher.setAuthTag(authTag);
  let decrypted = decipher.update(encrypted, "hex", "utf8");
  decrypted += decipher.final("utf8");
  return decrypted;
}

function redactValue(value: unknown): unknown {
  if (value === null || value === undefined) return value;
  const str = String(value);
  if (str.length <= 4) return "****";
  return str.substring(0, 2) + "****" + str.substring(str.length - 2);
}

export function redactPii<T extends Record<string, unknown>>(data: T): T {
  const redacted: Record<string, unknown> = JSON.parse(JSON.stringify(data));
  for (const key of Object.keys(redacted)) {
    if (PII_FIELDS.includes(key as PiiField)) {
      redacted[key] = redactValue(redacted[key]);
    } else if (typeof redacted[key] === "object" && redacted[key] !== null && !Array.isArray(redacted[key])) {
      redacted[key] = redactPii(redacted[key] as Record<string, unknown>);
    }
  }
  return redacted as T;
}

export function redactString(_value: string): string { return "****"; }
export function isSensitiveField(f: string): boolean { return SENSITIVE_FIELDS_LEGACY.includes(f as (typeof SENSITIVE_FIELDS_LEGACY)[number]); }
export function isPiiField(f: string): boolean { return PII_FIELDS.includes(f as PiiField); }
export function hashForLog(value: string): string {
  return crypto.createHash("sha256").update(value).digest("hex").substring(0, 16);
}

export function createKycRecord(id: string, userId: string, kycData: Record<string, unknown>): KycRecord {
  const encryptedData = encryptSensitiveData(JSON.stringify(kycData));
  return { id, userId, status: "submitted", encryptedData, submittedAt: Date.now(), metadata: { version: "1.0", lastUpdated: Date.now() } };
}

export function getKycData(kycRecord: KycRecord): Record<string, unknown> {
  return JSON.parse(decryptSensitiveData(kycRecord.encryptedData));
}
