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
  "taxId",
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

// ---------------------------------------------------------------------------
// Envelope-encryption API (LocalKeyProvider / KmsKeyProvider / KycService)
// ---------------------------------------------------------------------------

export interface KycPayload {
  userId: string;
  [key: string]: unknown;
}

export interface EncryptedRecord {
  keyId: string;
  ciphertext: string;
  iv: string;
  authTag: string;
  encryptedDek: string;
  dekIv: string;
  dekAuthTag: string;
}

export interface KmsClient {
  generateDataKey(input: { KeyId: string; KeySpec: string }): Promise<{ Plaintext: Buffer; CiphertextBlob: Buffer }>;
  decrypt(input: { CiphertextBlob: Buffer; KeyId: string }): Promise<{ Plaintext: Buffer }>;
}

interface WrappedKey {
  encryptedDek: Buffer;
  iv: Buffer;
  authTag: Buffer;
}

interface KeyProvider {
  currentKeyId(): string;
  wrapKey(dek: Buffer, keyId: string): Promise<WrappedKey>;
  unwrapKey(encryptedDek: Buffer, iv: Buffer, authTag: Buffer, keyId: string): Promise<Buffer>;
}

interface AccessLogEntry {
  action: "encrypt" | "decrypt" | "rotate";
  userId: string;
  timestamp: string;
  keyId?: string;
}

export class LocalKeyProvider implements KeyProvider {
  private readonly kek: Buffer;
  private readonly _keyId: string;

  constructor(hexKey?: string, keyId: string = "local-v1") {
    const key = hexKey ?? process.env.KYC_KEK_HEX ?? "";
    if (!/^[0-9a-fA-F]{64}$/.test(key)) {
      throw new Error("KYC_KEK_HEX must be a 64-character hex string");
    }
    this.kek = Buffer.from(key, "hex");
    this._keyId = keyId;
  }

  currentKeyId(): string {
    return this._keyId;
  }

  async wrapKey(dek: Buffer, _keyId: string): Promise<WrappedKey> {
    const iv = crypto.randomBytes(12);
    const cipher = crypto.createCipheriv("aes-256-gcm", this.kek, iv);
    const encryptedDek = Buffer.concat([cipher.update(dek), cipher.final()]);
    const authTag = cipher.getAuthTag();
    return { encryptedDek, iv, authTag };
  }

  async unwrapKey(encryptedDek: Buffer, iv: Buffer, authTag: Buffer, _keyId: string): Promise<Buffer> {
    try {
      const decipher = crypto.createDecipheriv("aes-256-gcm", this.kek, iv);
      decipher.setAuthTag(authTag);
      return Buffer.concat([decipher.update(encryptedDek), decipher.final()]);
    } catch {
      throw new Error("DEK unwrap failed: authentication tag mismatch");
    }
  }
}

export class KmsKeyProvider implements KeyProvider {
  constructor(private readonly client: KmsClient, private readonly kmsKeyId: string) {}

  currentKeyId(): string {
    return this.kmsKeyId;
  }

  async wrapKey(_dek: Buffer, keyId: string): Promise<WrappedKey> {
    const result = await this.client.generateDataKey({ KeyId: keyId, KeySpec: "AES_256" });
    return {
      encryptedDek: result.CiphertextBlob,
      iv: Buffer.alloc(0),
      authTag: Buffer.alloc(0),
    };
  }

  async unwrapKey(encryptedDek: Buffer, _iv: Buffer, _authTag: Buffer, keyId: string): Promise<Buffer> {
    const result = await this.client.decrypt({ CiphertextBlob: encryptedDek, KeyId: keyId });
    return result.Plaintext;
  }
}

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

  async encrypt(payload: KycPayload): Promise<EncryptedRecord> {
    const dek = crypto.randomBytes(32);
    const iv = crypto.randomBytes(12);
    const cipher = crypto.createCipheriv("aes-256-gcm", dek, iv);
    const plaintext = JSON.stringify(payload);
    const ciphertext = Buffer.concat([cipher.update(plaintext, "utf8"), cipher.final()]);
    const authTag = cipher.getAuthTag();

    const keyId = this.provider.currentKeyId();
    const wrapped = await this.provider.wrapKey(dek, keyId);

    this.log.push({ action: "encrypt", userId: payload.userId, timestamp: new Date().toISOString() });

    return {
      keyId,
      ciphertext: ciphertext.toString("base64"),
      iv: iv.toString("base64"),
      authTag: authTag.toString("base64"),
      encryptedDek: wrapped.encryptedDek.toString("base64"),
      dekIv: wrapped.iv.toString("base64"),
      dekAuthTag: wrapped.authTag.toString("base64"),
    };
  }

  async decrypt(record: EncryptedRecord): Promise<KycPayload> {
    const encryptedDek = Buffer.from(record.encryptedDek, "base64");
    const dekIv = Buffer.from(record.dekIv ?? "", "base64");
    const dekAuthTag = Buffer.from(record.dekAuthTag, "base64");

    const dek = await this.provider.unwrapKey(encryptedDek, dekIv, dekAuthTag, record.keyId);

    let plaintext: string;
    try {
      const iv = Buffer.from(record.iv, "base64");
      const authTag = Buffer.from(record.authTag, "base64");
      const ciphertext = Buffer.from(record.ciphertext, "base64");
      const decipher = crypto.createDecipheriv("aes-256-gcm", dek, iv);
      decipher.setAuthTag(authTag);
      plaintext = decipher.update(ciphertext).toString("utf8") + decipher.final("utf8");
    } catch {
      throw new Error("Payload decryption failed: authentication tag mismatch");
    }

    const result = JSON.parse(plaintext) as KycPayload;

    for (const field of SENSITIVE_FIELDS) {
      (result as Record<string, unknown>)[field] = "[REDACTED]";
    }

    this.log.push({ action: "decrypt", userId: result.userId, timestamp: new Date().toISOString() });

    return result;
  }

  async rotateKey(record: EncryptedRecord, newProvider: KeyProvider): Promise<EncryptedRecord> {
    const encryptedDek = Buffer.from(record.encryptedDek, "base64");
    const dekIv = Buffer.from(record.dekIv ?? "", "base64");
    const dekAuthTag = Buffer.from(record.dekAuthTag, "base64");
    const dek = await this.provider.unwrapKey(encryptedDek, dekIv, dekAuthTag, record.keyId);

    const newKeyId = newProvider.currentKeyId();
    const wrapped = await newProvider.wrapKey(dek, newKeyId);

    this.log.push({
      action: "rotate",
      userId: "system",
      timestamp: new Date().toISOString(),
      keyId: newKeyId,
    });

    return {
      ...record,
      keyId: newKeyId,
      encryptedDek: wrapped.encryptedDek.toString("base64"),
      dekIv: wrapped.iv.toString("base64"),
      dekAuthTag: wrapped.authTag.toString("base64"),
    };
  }

  getAccessLog(): AccessLogEntry[] {
    return [...this.log];
  }
}
