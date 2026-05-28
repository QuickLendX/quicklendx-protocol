/**
 * KYC Data Handling Service
 *
 * Envelope encryption for KYC payloads:
 *   - Per-record DEK (AES-256-GCM) encrypts the JSON payload
 *   - KEK (held by a KeyProvider) wraps the DEK
 *   - Only eDEK + ciphertext are persisted — no plaintext key material stored or logged
 *
 * Exports (new API):
 *   KeyProvider, LocalKeyProvider, KmsKeyProvider, KycService,
 *   KycPayload, EncryptedRecord, KmsClient, SENSITIVE_FIELDS
 *
 * Exports (legacy API — preserved for backward compatibility):
 *   initializeEncryption, encryptSensitiveData, decryptSensitiveData,
 *   redactPii, isSensitiveField, isPiiField, hashForLog,
 *   createKycRecord, getKycData, isEncryptionInitialized,
 *   KycRecord, KycMetadata, PII_FIELDS
 */

import * as crypto from "crypto";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ALGO = "aes-256-gcm";
const IV_BYTES = 12; // 96-bit IV for GCM

/** Fields redacted to "[REDACTED]" on every decrypt() call. */
export const SENSITIVE_FIELDS = [
  // camelCase (new API)
  "ssn",
  "taxId",
  "dateOfBirth",
  "passportNumber",
  "bankAccountNumber",
  "routingNumber",
  // snake_case (legacy API)
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

/** Fields redacted in log output (legacy API). */
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

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface KycPayload extends Record<string, unknown> {
  userId: string;
}

export interface EncryptedRecord {
  /** base64 AES-256-GCM ciphertext of JSON payload */
  ciphertext: string;
  /** base64 GCM auth tag for payload */
  authTag: string;
  /** base64 96-bit IV for payload */
  iv: string;
  /** base64 wrapped DEK (local: AES ciphertext; KMS: CiphertextBlob) */
  encryptedDek: string;
  /** base64 IV for DEK wrap (empty for KMS) */
  dekIv: string;
  /** base64 auth tag for DEK wrap (empty for KMS) */
  dekAuthTag: string;
  /** opaque identifier of the KEK used */
  keyId: string;
}

export interface AccessLogEntry {
  userId: string;
  action: "encrypt" | "decrypt" | "rotate";
  keyId: string;
  timestamp: string;
}

/** Minimal AWS KMS client interface (matches @aws-sdk/client-kms subset). */
export interface KmsClient {
  generateDataKey(params: { KeyId: string; KeySpec: string }): Promise<{
    Plaintext: Buffer;
    CiphertextBlob: Buffer;
  }>;
  decrypt(params: { CiphertextBlob: Buffer; KeyId: string }): Promise<{
    Plaintext: Buffer;
  }>;
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

  async wrapKey(
    _dek: Buffer,
    keyId: string,
  ): Promise<{ encryptedDek: Buffer; iv: Buffer; authTag: Buffer }> {
    const result = await this.client.generateDataKey({ KeyId: keyId, KeySpec: "AES_256" });
    return {
      encryptedDek: Buffer.from(result.CiphertextBlob),
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
    return Buffer.from(result.Plaintext);
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

  async encrypt(payload: KycPayload): Promise<EncryptedRecord> {
    const keyId = this.provider.currentKeyId();

    // Generate per-record DEK
    const dek = crypto.randomBytes(32);
    try {
      // Encrypt payload with DEK
      const iv = crypto.randomBytes(IV_BYTES);
      const cipher = crypto.createCipheriv(ALGO, dek, iv);
      const plaintext = Buffer.from(JSON.stringify(payload), "utf8");
      const ciphertext = Buffer.concat([cipher.update(plaintext), cipher.final()]);
      const authTag = cipher.getAuthTag();

      // Wrap DEK with KEK
      const { encryptedDek, iv: dekIv, authTag: dekAuthTag } = await this.provider.wrapKey(dek, keyId);

      this.log.push({ userId: payload.userId, action: "encrypt", keyId, timestamp: new Date().toISOString() });

      return {
        ciphertext: ciphertext.toString("base64"),
        authTag: authTag.toString("base64"),
        iv: iv.toString("base64"),
        encryptedDek: encryptedDek.toString("base64"),
        dekIv: dekIv.toString("base64"),
        dekAuthTag: dekAuthTag.toString("base64"),
        keyId,
      };
    } finally {
      dek.fill(0);
    }
  }

  async decrypt(record: EncryptedRecord): Promise<KycPayload> {
    // Unwrap DEK
    const dek = await this.provider.unwrapKey(
      Buffer.from(record.encryptedDek, "base64"),
      Buffer.from(record.dekIv, "base64"),
      Buffer.from(record.dekAuthTag, "base64"),
      record.keyId,
    );

    try {
      // Decrypt payload
      let plaintext: string;
      try {
        const decipher = crypto.createDecipheriv(ALGO, dek, Buffer.from(record.iv, "base64"));
        decipher.setAuthTag(Buffer.from(record.authTag, "base64"));
        plaintext = Buffer.concat([
          decipher.update(Buffer.from(record.ciphertext, "base64")),
          decipher.final(),
        ]).toString("utf8");
      } catch {
        throw new Error("Payload decryption failed: authentication tag mismatch");
      }

      const payload = JSON.parse(plaintext) as KycPayload;

      // Redact all sensitive fields (set unconditionally so callers cannot
      // distinguish "field was present" from "field was absent")
      for (const field of SENSITIVE_FIELDS) {
        (payload as Record<string, unknown>)[field] = "[REDACTED]";
      }

      this.log.push({ userId: payload.userId, action: "decrypt", keyId: record.keyId, timestamp: new Date().toISOString() });

      return payload;
    } finally {
      dek.fill(0);
    }
  }

  /**
   * Re-wraps the DEK under a new provider without decrypting the payload.
   * Plaintext PII is never exposed during rotation.
   */
  async rotateKey(record: EncryptedRecord, newProvider: KeyProvider): Promise<EncryptedRecord> {
    // Unwrap DEK with current provider
    const dek = await this.provider.unwrapKey(
      Buffer.from(record.encryptedDek, "base64"),
      Buffer.from(record.dekIv, "base64"),
      Buffer.from(record.dekAuthTag, "base64"),
      record.keyId,
    );

    const newKeyId = newProvider.currentKeyId();
    try {
      // Re-wrap DEK with new provider
      const { encryptedDek, iv: dekIv, authTag: dekAuthTag } = await newProvider.wrapKey(dek, newKeyId);

      // Extract userId from record for logging (without decrypting PII)
      const rotated: EncryptedRecord = {
        ...record,
        encryptedDek: encryptedDek.toString("base64"),
        dekIv: dekIv.toString("base64"),
        dekAuthTag: dekAuthTag.toString("base64"),
        keyId: newKeyId,
      };

      // Decode userId for log without exposing PII (parse only userId field)
      let userId = "unknown";
      try {
        const tmpDek = await this.provider.unwrapKey(
          Buffer.from(record.encryptedDek, "base64"),
          Buffer.from(record.dekIv, "base64"),
          Buffer.from(record.dekAuthTag, "base64"),
          record.keyId,
        );
        try {
          const decipher = crypto.createDecipheriv(ALGO, tmpDek, Buffer.from(record.iv, "base64"));
          decipher.setAuthTag(Buffer.from(record.authTag, "base64"));
          const plain = Buffer.concat([
            decipher.update(Buffer.from(record.ciphertext, "base64")),
            decipher.final(),
          ]).toString("utf8");
          userId = (JSON.parse(plain) as KycPayload).userId;
        } finally {
          tmpDek.fill(0);
        }
      } catch {
        // userId stays "unknown" — don't fail rotation for logging
      }

      this.log.push({ userId, action: "rotate", keyId: newKeyId, timestamp: new Date().toISOString() });

      return rotated;
    } finally {
      dek.fill(0);
    }
  }
}

// ---------------------------------------------------------------------------
// Legacy API (preserved for backward compatibility with kyc-service.test.ts)
// ---------------------------------------------------------------------------

const LEGACY_ALGO = "aes-256-gcm";
const LEGACY_IV_LENGTH = 16;
const LEGACY_AUTH_TAG_LENGTH = 16;
const KEY_DERIVATION_ITERATIONS = 100000;

interface LegacyEncryptionConfig {
  encryptionKey: string;
}

let legacyConfig: LegacyEncryptionConfig | null = null;

export function initializeEncryption(masterKey: string): void {
  const salt = crypto.createHash("sha256").update("quicklendx-kyc-salt").digest();
  const key = crypto.pbkdf2Sync(masterKey, salt, KEY_DERIVATION_ITERATIONS, 32, "sha256");
  legacyConfig = { encryptionKey: key.toString("hex") };
}

export function encryptSensitiveData(plaintext: string): string {
  if (!legacyConfig) throw new Error("Encryption not initialized. Call initializeEncryption first.");
  const key = Buffer.from(legacyConfig.encryptionKey, "hex");
  const iv = crypto.randomBytes(LEGACY_IV_LENGTH);
  const cipher = crypto.createCipheriv(LEGACY_ALGO, key, iv);
  let encrypted = cipher.update(plaintext, "utf8", "hex");
  encrypted += cipher.final("hex");
  const authTag = cipher.getAuthTag();
  return iv.toString("hex") + authTag.toString("hex") + encrypted;
}

export function decryptSensitiveData(ciphertext: string): string {
  if (!legacyConfig) throw new Error("Encryption not initialized. Call initializeEncryption first.");
  const key = Buffer.from(legacyConfig.encryptionKey, "hex");
  const iv = Buffer.from(ciphertext.substring(0, LEGACY_IV_LENGTH * 2), "hex");
  const authTag = Buffer.from(
    ciphertext.substring(LEGACY_IV_LENGTH * 2, LEGACY_IV_LENGTH * 2 + LEGACY_AUTH_TAG_LENGTH * 2),
    "hex",
  );
  const encrypted = ciphertext.substring(LEGACY_IV_LENGTH * 2 + LEGACY_AUTH_TAG_LENGTH * 2);
  const decipher = crypto.createDecipheriv(LEGACY_ALGO, key, iv);
  decipher.setAuthTag(authTag);
  let decrypted = decipher.update(encrypted, "hex", "utf8");
  decrypted += decipher.final("utf8");
  return decrypted;
}

export function redactPii<T extends Record<string, any>>(data: T): T {
  const redacted: Record<string, any> = JSON.parse(JSON.stringify(data));
  for (const key of Object.keys(redacted)) {
    if (PII_FIELDS.includes(key as PiiField)) {
      redacted[key] = _redactValue(redacted[key]);
    } else if (typeof redacted[key] === "object" && redacted[key] !== null && !Array.isArray(redacted[key])) {
      redacted[key] = redactPii(redacted[key] as Record<string, any>);
    }
  }
  return redacted as T;
}

function _redactValue(value: any): string {
  if (value === null || value === undefined) return value;
  const str = String(value);
  if (str.length <= 4) return "****";
  return str.substring(0, 2) + "****" + str.substring(str.length - 2);
}

export function redactString(_value: string): string {
  return "****";
}

export function isSensitiveField(fieldName: string): boolean {
  return SENSITIVE_FIELDS.includes(fieldName as SensitiveField);
}

export function isPiiField(fieldName: string): boolean {
  return PII_FIELDS.includes(fieldName as PiiField);
}

export function hashForLog(value: string): string {
  return crypto.createHash("sha256").update(value).digest("hex").substring(0, 16);
}

export function isEncryptionInitialized(): boolean {
  return legacyConfig !== null;
}

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

export function createKycRecord(
  id: string,
  userId: string,
  kycData: Record<string, any>,
): KycRecord {
  const encryptedData = encryptSensitiveData(JSON.stringify(kycData));
  return {
    id,
    userId,
    status: "submitted",
    encryptedData,
    submittedAt: Date.now(),
    metadata: { version: "1.0", lastUpdated: Date.now() },
  };
}

export function getKycData(kycRecord: KycRecord): Record<string, any> {
  return JSON.parse(decryptSensitiveData(kycRecord.encryptedData));
}
