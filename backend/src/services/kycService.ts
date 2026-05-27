/**
 * KYC Data Handling Service
 * 
 * Provides secure handling for KYC-related payloads including:
 * - Encryption-at-rest for sensitive fields
 * - PII minimization and redaction
 * - Access logging for every read
 * 
 * Security assumptions:
 * - Logs and backups are sensitive surfaces
 * - Least privilege principle
 * - No accidental PII leakage
 */

import * as crypto from "crypto";

// Configuration constants
const ENCRYPTION_ALGORITHM = "aes-256-gcm";
const KEY_DERIVATIONIterations = 100000;
const SALT_LENGTH = 32;
const IV_LENGTH = 16;
const AUTH_TAG_LENGTH = 16;

// Sensitive fields that require encryption
export const SENSITIVE_FIELDS = [
  // snake_case (legacy/storage)
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
  // camelCase (API/tests)
  "taxId",
  "dateOfBirth",
  "passportNumber",
  "bankAccountNumber",
  "routingNumber",
] as const;

// Fields that should be redacted in logs
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
  "ipAddress"
] as const;

export type SensitiveField = typeof SENSITIVE_FIELDS[number];
export type PiiField = typeof PII_FIELDS[number];

/**
 * Encryption key management
 * In production, this should be integrated with a secure key management service (KMS)
 */
interface EncryptionConfig {
  encryptionKey: string;
}

let encryptionConfig: EncryptionConfig | null = null;

/**
 * Initialize encryption with a master key
 * In production, this should come from environment variables or KMS
 */
export function initializeEncryption(masterKey: string): void {
  // Derive a 256-bit key from the master key using PBKDF2
  const salt = crypto.createHash("sha256").update("quicklendx-kyc-salt").digest();
  const key = crypto.pbkdf2Sync(masterKey, salt, KEY_DERIVATIONIterations, 32, "sha256");
  
  encryptionConfig = {
    encryptionKey: key.toString("hex")
  };
}

/**
 * Encrypt sensitive data using AES-256-GCM
 */
export function encryptSensitiveData(plaintext: string): string {
  if (!encryptionConfig) {
    throw new Error("Encryption not initialized. Call initializeEncryption first.");
  }

  const key = Buffer.from(encryptionConfig.encryptionKey, "hex");
  const iv = crypto.randomBytes(IV_LENGTH);
  const cipher = crypto.createCipheriv(ENCRYPTION_ALGORITHM, key, iv);

  let encrypted = cipher.update(plaintext, "utf8", "hex");
  encrypted += cipher.final("hex");
  
  const authTag = cipher.getAuthTag();

  // Combine IV + authTag + encrypted data
  return iv.toString("hex") + authTag.toString("hex") + encrypted;
}

/**
 * Decrypt sensitive data
 */
export function decryptSensitiveData(ciphertext: string): string {
  if (!encryptionConfig) {
    throw new Error("Encryption not initialized. Call initializeEncryption first.");
  }

  const key = Buffer.from(encryptionConfig.encryptionKey, "hex");
  
  // Extract IV, authTag, and encrypted data
  const iv = Buffer.from(ciphertext.substring(0, IV_LENGTH * 2), "hex");
  const authTag = Buffer.from(ciphertext.substring(IV_LENGTH * 2, IV_LENGTH * 2 + AUTH_TAG_LENGTH * 2), "hex");
  const encrypted = ciphertext.substring(IV_LENGTH * 2 + AUTH_TAG_LENGTH * 2);

  const decipher = crypto.createDecipheriv(ENCRYPTION_ALGORITHM, key, iv);
  decipher.setAuthTag(authTag);

  let decrypted = decipher.update(encrypted, "hex", "utf8");
  decrypted += decipher.final("utf8");

  return decrypted;
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

/**
 * Create a new KYC record with encrypted data
 */
export function createKycRecord(
  id: string,
  userId: string,
  kycData: Record<string, any>
): KycRecord {
  // Encrypt sensitive fields
  const sensitiveData = JSON.stringify(kycData);
  const encryptedData = encryptSensitiveData(sensitiveData);

  return {
    id,
    userId,
    status: "submitted",
    encryptedData,
    submittedAt: Date.now(),
    metadata: {
      version: "1.0",
      lastUpdated: Date.now()
    }
  };
}

/**
 * Decrypt and retrieve KYC data
 * This should always be accompanied by access logging
 */
export function getKycData(kycRecord: KycRecord): Record<string, any> {
  const decrypted = decryptSensitiveData(kycRecord.encryptedData);
  return JSON.parse(decrypted);
}

/**
 * Validate encryption configuration
 */
export function isEncryptionInitialized(): boolean {
  return encryptionConfig !== null;
}
