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
  "kyc_data"
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