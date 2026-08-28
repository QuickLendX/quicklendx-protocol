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
import { getPreparedStatement } from "../lib/database";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ALGO = "aes-256-gcm";
const IV_BYTES = 12; // 96-bit IV for GCM
const TAG_BYTES = 16;
const ENVELOPE_V2_PREFIX = "v2:";
const PBKDF2_ITERATIONS = 100000;
const PBKDF2_SALT = "quicklendx-kyc-salt";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/** Fields redacted to "[REDACTED]" on every decrypt() call. */
export const SENSITIVE_FIELDS = [
  // snake_case (legacy/storage)
  "tax_id",
  "customer_name",
  "customer_address",
  "date_of_birth",
  "dateOfBirth",
  "ssn",
  "passport_number",
  "passportNumber",
  "national_id",
  "phone_number",
  "email",
  "bank_account",
  "bankAccountNumber",
  "routingNumber",
  "kyc_document",
  "kyc_data",
  // camelCase (API/tests)
  "taxId",
  "dateOfBirth",
  "passportNumber",
  "bankAccountNumber",
  "routingNumber",
] as const;

export type SensitiveField = (typeof SENSITIVE_FIELDS)[number];

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
// KeyRing interface and implementation
// ---------------------------------------------------------------------------

export interface KeyRing {
  getActiveKeyId(): string;
  getKey(keyId: string): Buffer;
}

interface KeyConfig {
  activeKeyId: string;
  keys: Record<string, string>; // keyId -> master key (for PBKDF2)
}

let keyRing: KeyRing | null = null;
let encryptionConfig: KeyConfig | null = null;

/**
 * Initialize encryption with a key ring (backward compatible with single key)
 */
export function initializeEncryption(
  masterKeyOrConfig: string | { activeKeyId: string; keys: Record<string, string> }
): void {
  if (typeof masterKeyOrConfig === "string") {
    // Backward compatibility: single key, default keyId = "v1"
    encryptionConfig = {
      activeKeyId: "v1",
      keys: { "v1": masterKeyOrConfig }
    };
  } else {
    encryptionConfig = masterKeyOrConfig;
  }

  keyRing = {
    getActiveKeyId(): string {
      if (!encryptionConfig) throw new Error("Encryption not initialized");
      return encryptionConfig.activeKeyId;
    },
    getKey(keyId: string): Buffer {
      if (!encryptionConfig) throw new Error("Encryption not initialized");
      const masterKey = encryptionConfig.keys[keyId];
      if (!masterKey) throw new Error(`Key not found: ${keyId}`);
      const salt = crypto.createHash("sha256").update(PBKDF2_SALT).digest();
      return crypto.pbkdf2Sync(masterKey, salt, PBKDF2_ITERATIONS, 32, "sha256");
    }
  };
}

/**
 * Encrypt sensitive data using AES-256-GCM (legacy v1 format)
 */
export function encryptSensitiveData(plaintext: string): string {
  if (!keyRing || !encryptionConfig) {
    throw new Error("Encryption not initialized. Call initializeEncryption first.");
  }

  const key = keyRing.getKey(encryptionConfig.activeKeyId);
  const iv = crypto.randomBytes(IV_BYTES);
  const cipher = crypto.createCipheriv(ALGO, key, iv);

  let encrypted = cipher.update(plaintext, "utf8", "hex");
  encrypted += cipher.final("hex");
  
  const authTag = cipher.getAuthTag();

  // Combine IV + authTag + encrypted data
  return iv.toString("hex") + authTag.toString("hex") + encrypted;
}

/**
 * Encrypt sensitive data using versioned envelope v2 format
 */
export function encryptSensitiveDataV2(plaintext: string): string {
  if (!keyRing || !encryptionConfig) {
    throw new Error("Encryption not initialized. Call initializeEncryption first.");
  }

  const keyId = keyRing.getActiveKeyId();
  const key = keyRing.getKey(keyId);
  const iv = crypto.randomBytes(IV_BYTES);
  const cipher = crypto.createCipheriv(ALGO, key, iv);

  let encrypted = cipher.update(plaintext, "utf8", "hex");
  encrypted += cipher.final("hex");
  
  const authTag = cipher.getAuthTag();

  // Envelope v2 format: "v2:<keyId>:<iv>:<authTag>:<encrypted>"
  return `${ENVELOPE_V2_PREFIX}${keyId}:${iv.toString("hex")}:${authTag.toString("hex")}:${encrypted}`;
}

/**
 * Decrypt sensitive data (supports both v1 and v2 formats)
 */
export function decryptSensitiveDataAny(ciphertext: string): string {
  if (!keyRing) {
    throw new Error("Encryption not initialized. Call initializeEncryption first.");
  }

  if (ciphertext.startsWith(ENVELOPE_V2_PREFIX)) {
    // Handle v2 format
    const parts = ciphertext.slice(ENVELOPE_V2_PREFIX.length).split(":");
    if (parts.length !== 4) {
      throw new Error("Invalid v2 envelope format");
    }
    const [keyId, ivHex, authTagHex, encryptedHex] = parts;
    const key = keyRing.getKey(keyId);
    const iv = Buffer.from(ivHex, "hex");
    const authTag = Buffer.from(authTagHex, "hex");

    const decipher = crypto.createDecipheriv(ALGO, key, iv);
    decipher.setAuthTag(authTag);

    let decrypted = decipher.update(encryptedHex, "hex", "utf8");
    decrypted += decipher.final("utf8");
    return decrypted;
  } else {
    // Handle legacy v1 format
    if (!encryptionConfig) {
      throw new Error("Encryption not initialized");
    }
    // Use any available key to try decrypting v1 (backward compatible)
    // First try active key, then try others
    let key: Buffer | null = null;
    try {
      key = keyRing.getKey(encryptionConfig.activeKeyId);
    } catch {
      for (const k in encryptionConfig.keys) {
        try {
          key = keyRing.getKey(k);
          break;
        } catch {}
      }
    }
    if (!key) throw new Error("No key available to decrypt v1 format");

    const iv = Buffer.from(ciphertext.substring(0, IV_BYTES * 2), "hex");
    const authTag = Buffer.from(ciphertext.substring(IV_BYTES * 2, IV_BYTES * 2 + TAG_BYTES * 2), "hex");
    const encrypted = ciphertext.substring(IV_BYTES * 2 + TAG_BYTES * 2);

    const decipher = crypto.createDecipheriv(ALGO, key, iv);
    decipher.setAuthTag(authTag);

    let decrypted = decipher.update(encrypted, "hex", "utf8");
    decrypted += decipher.final("utf8");
    return decrypted;
  }
}

/**
 * Decrypt sensitive data (legacy, alias for decryptSensitiveDataAny for backward compatibility)
 */
export function decryptSensitiveData(ciphertext: string): string {
  return decryptSensitiveDataAny(ciphertext);
}

/**
 * Redact PII from an object for safe logging
 * Returns a new object with sensitive fields redacted
 */
export function redactPii<T extends Record<string, any>>(data: T): T {
  // Create a deep clone to avoid modifying the original
  const redacted: Record<string, any> = JSON.parse(JSON.stringify(data));
  
  for (const key of Object.keys(redacted)) {
    if (PII_FIELDS.includes(key as PiiField)) {
      redacted[key] = redactValue(redacted[key]);
    } else if (typeof redacted[key] === "object" && redacted[key] !== null && !Array.isArray(redacted[key])) {
      redacted[key] = redactPii(redacted[key] as Record<string, any>);
    }
  }
  
  return redacted as T;
}

/**
 * Redact a single value
 */
function redactValue(value: any): string {
  if (value === null || value === undefined) {
    return value;
  }
  
  const str = String(value);
  
  if (str.length <= 4) {
    return "****";
  }
  
  // Show first 2 and last 2 characters
  const firstTwo = str.substring(0, 2);
  const lastTwo = str.substring(str.length - 2);
  return firstTwo + "****" + lastTwo;
}

/**
 * Redact a string value completely
 */
export function redactString(value: string): string {
  return "****";
}

/**
 * Check if a field is sensitive
 */
export function isSensitiveField(fieldName: string): boolean {
  return SENSITIVE_FIELDS.includes(fieldName as SensitiveField);
}

/**
 * Check if a field contains PII
 */
export function isPiiField(fieldName: string): boolean {
  return PII_FIELDS.includes(fieldName as PiiField);
}

// ---------------------------------------------------------------------------
// KMS + Local key providers and KycService (exported for tests)
// ---------------------------------------------------------------------------

export type KycPayload = Record<string, any>;

export type EncryptedRecord = {
  keyId: string;
  ciphertext: string; // base64
  iv: string; // base64
  authTag: string; // base64
  encryptedDek: string; // base64
  dekIv?: string; // base64
  dekAuthTag?: string; // base64
  createdAt: string;
};

export type KmsClient = {
  generateDataKey(params: { KeyId: string; KeySpec: string }): Promise<{ Plaintext: Buffer; CiphertextBlob: Buffer }>;
  decrypt(params: { CiphertextBlob: Buffer; KeyId?: string }): Promise<{ Plaintext: Buffer }>;
};

const DEK_LENGTH = 32;
const WRAP_IV_LENGTH = 16;

export class LocalKeyProvider {
  private kekHex: string;
  private keyIdStr: string;

  constructor(kekHex?: string, keyId?: string) {
    const envKey = process.env.KYC_KEK_HEX;
    this.kekHex = kekHex || envKey || "";
    if (this.kekHex.length !== 64) {
      throw new Error("KYC_KEK_HEX must be a 64-character hex string");
    }
    this.keyIdStr = keyId || "local-v1";
  }

  currentKeyId(): string {
    return this.keyIdStr;
  }

  async wrapKey(dek: Buffer, keyId: string): Promise<{ encryptedDek: Buffer; iv: Buffer; authTag: Buffer }> {
    const salt = Buffer.from("quicklendx-kyc-salt");
    const key = crypto.pbkdf2Sync(this.kekHex, salt, 100000, 32, "sha256");
    const iv = crypto.randomBytes(WRAP_IV_LENGTH);
    const cipher = crypto.createCipheriv("aes-256-gcm", key, iv);
    const enc = Buffer.concat([cipher.update(dek), cipher.final()]);
    const authTag = cipher.getAuthTag();
    // Return encrypted DEK
    return { encryptedDek: enc, iv, authTag };
  }

  async unwrapKey(encryptedDek: Buffer, iv: Buffer, authTag: Buffer, keyId: string): Promise<Buffer> {
    try {
      const salt = Buffer.from("quicklendx-kyc-salt");
      const key = crypto.pbkdf2Sync(this.kekHex, salt, 100000, 32, "sha256");
      const decipher = crypto.createDecipheriv("aes-256-gcm", key, iv);
      decipher.setAuthTag(authTag);
      const dec = Buffer.concat([decipher.update(encryptedDek), decipher.final()]);
      return dec;
    } catch (e) {
      throw new Error("DEK unwrap failed: authentication tag mismatch");
    }
  }
}

export class KmsKeyProvider {
  private client: KmsClient;
  private keyIdStr: string;

  constructor(client: KmsClient, keyId: string) {
    this.client = client;
    this.keyIdStr = keyId;
  }

  currentKeyId(): string {
    return this.keyIdStr;
  }

  async wrapKey(_dek: Buffer, keyId: string): Promise<{ encryptedDek: Buffer; iv: Buffer; authTag: Buffer }> {
    // For KMS provider, we generate a data key and return the ciphertext blob.
    const res = await this.client.generateDataKey({ KeyId: keyId, KeySpec: "AES_256" });
    return { encryptedDek: res.CiphertextBlob, iv: Buffer.alloc(0), authTag: Buffer.alloc(0) };
  }

  async unwrapKey(encryptedDek: Buffer, _iv: Buffer, _authTag: Buffer, keyId: string): Promise<Buffer> {
    const res = await this.client.decrypt({ CiphertextBlob: encryptedDek, KeyId: keyId });
    return res.Plaintext;
  }

  // Expose a helper to generate a data key (returns dek plaintext and encrypted blob)
  async generateDataKey(keyId: string): Promise<{ dek: Buffer; encryptedDek: Buffer }> {
    const res = await this.client.generateDataKey({ KeyId: keyId, KeySpec: "AES_256" });
    return { dek: res.Plaintext, encryptedDek: res.CiphertextBlob };
  }
}

export class KycService {
  private provider: LocalKeyProvider | KmsKeyProvider;
  private accessLog: Array<Record<string, any>> = [];

  constructor(provider: LocalKeyProvider | KmsKeyProvider) {
    this.provider = provider;
  }

  getProvider() {
    return this.provider;
  }

  setProvider(p: LocalKeyProvider | KmsKeyProvider) {
    this.provider = p;
  }

  getAccessLog(): Array<{ action: string; keyId?: string; userId?: string; timestamp: string }> {
    return JSON.parse(JSON.stringify(this.accessLog));
  }

  private redact(obj: Record<string, any>): Record<string, any> {
    const out: Record<string, any> = { ...obj };
    for (const field of SENSITIVE_FIELDS) {
      if (field in out) {
        out[field] = "[REDACTED]";
      }
    }
    return out;
  }

  async encrypt(payload: KycPayload): Promise<EncryptedRecord> {
    // Obtain DEK from KMS provider when applicable, otherwise generate locally
    let dek: Buffer;
    let wrap: { encryptedDek: Buffer; iv: Buffer; authTag: Buffer };

    if (this.provider instanceof KmsKeyProvider) {
      const kd = await (this.provider as KmsKeyProvider).generateDataKey(this.provider.currentKeyId());
      dek = kd.dek;
      wrap = { encryptedDek: kd.encryptedDek, iv: Buffer.alloc(0), authTag: Buffer.alloc(0) };
    } else {
      dek = crypto.randomBytes(DEK_LENGTH);
      wrap = await this.provider.wrapKey(dek, this.provider.currentKeyId());
    }

    const iv = crypto.randomBytes(12);
    const cipher = crypto.createCipheriv("aes-256-gcm", dek, iv);
    const plaintext = Buffer.from(JSON.stringify(payload), "utf8");
    const ciphertext = Buffer.concat([cipher.update(plaintext), cipher.final()]);
    const authTag = cipher.getAuthTag();

    // zero DEK
    dek.fill(0);

    const record: EncryptedRecord = {
      keyId: this.provider.currentKeyId(),
      ciphertext: ciphertext.toString("base64"),
      iv: iv.toString("base64"),
      authTag: authTag.toString("base64"),
      encryptedDek: wrap.encryptedDek.toString("base64"),
      dekIv: wrap.iv.toString("base64"),
      dekAuthTag: wrap.authTag.toString("base64"),
      createdAt: new Date().toISOString(),
    };

    this.accessLog.push({ action: "encrypt", userId: payload.userId, timestamp: new Date().toISOString(), keyId: record.keyId });
    return record;
  }

  async decrypt(record: EncryptedRecord): Promise<KycPayload> {
    const encDek = Buffer.from(record.encryptedDek, "base64");
    const dekIv = Buffer.from(record.dekIv || "", "base64");
    const dekAuthTag = Buffer.from(record.dekAuthTag || "", "base64");

    const dek = await this.provider.unwrapKey(encDek, dekIv, dekAuthTag, record.keyId);
    try {
      try {
        const iv = Buffer.from(record.iv, "base64");
        const authTag = Buffer.from(record.authTag, "base64");
        const decipher = crypto.createDecipheriv("aes-256-gcm", dek, iv);
        decipher.setAuthTag(authTag);
        const pt = Buffer.concat([decipher.update(Buffer.from(record.ciphertext, "base64")), decipher.final()]);
        const parsed = JSON.parse(pt.toString("utf8"));
        const redacted = this.redact(parsed);
        this.accessLog.push({ action: "decrypt", userId: parsed.userId, timestamp: new Date().toISOString(), keyId: record.keyId });
        return redacted;
      } catch (e) {
        throw new Error("Payload decryption failed: authentication tag mismatch");
      }
    } finally {
      dek.fill(0);
    }
  }

  async rotateKey(record: EncryptedRecord, newProvider: LocalKeyProvider | KmsKeyProvider): Promise<EncryptedRecord> {
    const encDek = Buffer.from(record.encryptedDek, "base64");
    const dekIv = Buffer.from(record.dekIv || "", "base64");
    const dekAuthTag = Buffer.from(record.dekAuthTag || "", "base64");
    const dek = await this.provider.unwrapKey(encDek, dekIv, dekAuthTag, record.keyId);
    try {
      const wrap = await newProvider.wrapKey(dek, newProvider.currentKeyId());
      const rotated: EncryptedRecord = {
        ...record,
        keyId: newProvider.currentKeyId(),
        encryptedDek: wrap.encryptedDek.toString("base64"),
        dekIv: wrap.iv.toString("base64"),
        dekAuthTag: wrap.authTag.toString("base64"),
      };
      this.accessLog.push({ action: "rotate", timestamp: new Date().toISOString(), keyId: rotated.keyId });
      return rotated;
    } finally {
      dek.fill(0);
    }
  }
}

/**
 * Hash sensitive identifier for logging (non-reversible)
 */
export function hashForLog(value: string): string {
  return crypto.createHash("sha256").update(value).digest("hex").substring(0, 16);
}

/**
 * KYC Data storage model
 * Represents how KYC data should be stored securely
 */
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

export function isEncryptionInitialized(): boolean {
  return encryptionConfig !== null;
}

export function createKycRecord(id: string, userId: string, kycData: Record<string, any>): KycRecord {
  const encryptedData = encryptSensitiveDataV2(JSON.stringify(kycData));
  return { id, userId, status: "submitted", encryptedData, submittedAt: Date.now(), metadata: { version: "2.0", lastUpdated: Date.now() } };
}

export function getKycData(kycRecord: KycRecord): Record<string, any> {
  return JSON.parse(decryptSensitiveDataAny(kycRecord.encryptedData));
}

export function getKycStatus(businessId: string): { status: string; verifiedAt?: number } | null {
  try {
    const stmt = getPreparedStatement("SELECT status, verified_at FROM kyc_records WHERE user_id = ?");
    const row = stmt.get(businessId);
    if (!row) return null;
    return {
      status: row.status as string,
      verifiedAt: row.verified_at ? Number(row.verified_at) : undefined,
    };
  } catch (err: any) {
    const msg = err && err.message ? String(err.message) : "";
    if (process.env.NODE_ENV === "test" && /no such table/i.test(msg)) {
      // Return null in test environments where the migration hasn't run
      return null;
    }
    throw err;
  }
}


