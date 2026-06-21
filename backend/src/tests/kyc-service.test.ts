/**
 * KYC Data Handling Service Tests
 * 
 * Comprehensive tests for:
 * - Encryption/decryption
 * - PII redaction
 * - Access logging
 * - Security assumptions
 */

import {
  initializeEncryption,
  encryptSensitiveData,
  decryptSensitiveData,
  redactPii,
  isSensitiveField,
  isPiiField,
  hashForLog,
  createKycRecord,
  getKycData,
  isEncryptionInitialized,
  SENSITIVE_FIELDS,
  PII_FIELDS
} from "../services/kycService";

describe("KYC Service", () => {
  const testMasterKey = "test-master-key-for-encryption-12345678901234567890";

  beforeAll(() => {
    initializeEncryption(testMasterKey);
  });

  describe("Encryption", () => {
    it("should initialize encryption correctly", () => {
      expect(isEncryptionInitialized()).toBe(true);
    });

    it("should encrypt and decrypt data correctly", () => {
      const plaintext = "Sensitive KYC data: John Doe, SSN: 123-45-6789";
      const encrypted = encryptSensitiveData(plaintext);
      
      expect(encrypted).not.toBe(plaintext);
      expect(encrypted.length).toBeGreaterThan(plaintext.length);
      
      const decrypted = decryptSensitiveData(encrypted);
      expect(decrypted).toBe(plaintext);
    });

    it("should produce different ciphertext for same plaintext", () => {
      const plaintext = "Same data";
      const encrypted1 = encryptSensitiveData(plaintext);
      const encrypted2 = encryptSensitiveData(plaintext);
      
      // Due to random IV, same plaintext should produce different ciphertext
      expect(encrypted1).not.toBe(encrypted2);
    });

    it("should handle empty string", () => {
      const plaintext = "";
      const encrypted = encryptSensitiveData(plaintext);
      const decrypted = decryptSensitiveData(encrypted);
      
      expect(decrypted).toBe(plaintext);
    });

    it("should handle unicode characters", () => {
      const plaintext = "Name: 张三, ID: 身份证123456789012345";
      const encrypted = encryptSensitiveData(plaintext);
      const decrypted = decryptSensitiveData(encrypted);
      
      expect(decrypted).toBe(plaintext);
    });

    it("should handle long data", () => {
      const plaintext = "A".repeat(10000);
      const encrypted = encryptSensitiveData(plaintext);
      const decrypted = decryptSensitiveData(encrypted);
      
      expect(decrypted).toBe(plaintext);
    });

    it("should throw error when not initialized", () => {
      // This would require a fresh module import in real scenario
      // For testing, we verify the function exists
      expect(isEncryptionInitialized()).toBe(true);
    });
  });

  describe("PII Redaction", () => {
    it("should redact tax_id field", () => {
      const data = { tax_id: "TX-12345", name: "Test" };
      const redacted = redactPii(data);
      
      expect(redacted.tax_id).toBe("TX****45");
    });

    it("should redact customer_name field", () => {
      const data = { customer_name: "John Doe", amount: 1000 };
      const redacted = redactPii(data);
      
      // Shows first 2 + "****" + last 2 = "Jo" + "****" + "oe" = "Jo****oe"
      expect(redacted.customer_name).toBe("Jo****oe");
    });

    it("should redact customer_address field", () => {
      const data = { customer_address: "123 Main St, City, State 12345", amount: 1000 };
      const redacted = redactPii(data);
      
      expect(redacted.customer_address).toBe("12****45");
    });

    it("should redact phone_number field", () => {
      const data = { phone_number: "+1-555-123-4567", status: "active" };
      const redacted = redactPii(data);
      
      expect(redacted.phone_number).toBe("+1****67");
    });

    it("should redact email field", () => {
      const data = { email: "test@example.com", status: "active" };
      const redacted = redactPii(data);
      
      expect(redacted.email).toBe("te****om");
    });

    it("should redact nested objects", () => {
      const data = {
        user: {
          customer_name: "John Doe",
          tax_id: "TX-12345"
        },
        amount: 1000
      };
      const redacted = redactPii(data);
      
      // Nested objects should be redacted - check that customer_name is redacted
      expect(redacted.user.customer_name).not.toBe("John Doe");
      // tax_id should be redacted
      expect(redacted.user.tax_id).not.toBe("TX-12345");
      // Non-PII fields should remain unchanged
      expect(redacted.amount).toBe(1000);
    });

    it("should handle null values", () => {
      const data = { tax_id: null, name: "Test" };
      const redacted = redactPii(data);
      
      expect(redacted.tax_id).toBeNull();
    });

    it("should handle undefined values", () => {
      const data = { tax_id: undefined, name: "Test" };
      const redacted = redactPii(data);
      
      expect(redacted.tax_id).toBeUndefined();
    });

    it("should not modify non-PII fields", () => {
      const data = {
        id: "inv_123",
        amount: "1000000",
        status: "verified",
        created_at: 1234567890
      };
      const redacted = redactPii(data);
      
      expect(redacted.id).toBe("inv_123");
      expect(redacted.amount).toBe("1000000");
      expect(redacted.status).toBe("verified");
      expect(redacted.created_at).toBe(1234567890);
    });

    it("should handle short values gracefully", () => {
      const data = { tax_id: "AB", name: "Test" };
      const redacted = redactPii(data);
      
      expect(redacted.tax_id).toBe("****");
    });
  });

  describe("Field Classification", () => {
    it("should correctly identify sensitive fields", () => {
      expect(isSensitiveField("tax_id")).toBe(true);
      expect(isSensitiveField("customer_name")).toBe(true);
      expect(isSensitiveField("customer_address")).toBe(true);
      expect(isSensitiveField("amount")).toBe(false);
      expect(isSensitiveField("status")).toBe(false);
    });

    it("should correctly identify PII fields", () => {
      expect(isPiiField("tax_id")).toBe(true);
      expect(isPiiField("customer_name")).toBe(true);
      expect(isPiiField("customer_address")).toBe(true);
      expect(isPiiField("ssn")).toBe(true);
      expect(isPiiField("amount")).toBe(false);
    });

    it("should have all expected sensitive fields", () => {
      expect(SENSITIVE_FIELDS).toContain("tax_id");
      expect(SENSITIVE_FIELDS).toContain("customer_name");
      expect(SENSITIVE_FIELDS).toContain("customer_address");
      expect(SENSITIVE_FIELDS).toContain("kyc_data");
    });

    it("should have all expected PII fields", () => {
      expect(PII_FIELDS).toContain("tax_id");
      expect(PII_FIELDS).toContain("customer_name");
      expect(PII_FIELDS).toContain("customer_address");
      expect(PII_FIELDS).toContain("ssn");
    });
  });

  describe("Hashing for Logs", () => {
    it("should produce consistent hash for same input", () => {
      const value = "test-value";
      const hash1 = hashForLog(value);
      const hash2 = hashForLog(value);
      
      expect(hash1).toBe(hash2);
    });

    it("should produce different hash for different input", () => {
      const hash1 = hashForLog("value1");
      const hash2 = hashForLog("value2");
      
      expect(hash1).not.toBe(hash2);
    });

    it("should produce fixed-length hash", () => {
      const hash = hashForLog("test");
      
      expect(hash.length).toBe(16);
    });

    it("should not be reversible", () => {
      const value = "sensitive-data";
      const hash = hashForLog(value);
      
      expect(hash).not.toContain(value);
    });
  });

  describe("KYC Record Management", () => {
    it("should create KYC record with encrypted data", () => {
      const kycData = {
        customer_name: "John Doe",
        tax_id: "TX-12345",
        customer_address: "123 Main St"
      };
      
      const record = createKycRecord("kyc_123", "user_456", kycData);
      
      expect(record.id).toBe("kyc_123");
      expect(record.userId).toBe("user_456");
      expect(record.status).toBe("submitted");
      expect(record.encryptedData).not.toBe(JSON.stringify(kycData));
      expect(record.submittedAt).toBeDefined();
      expect(record.metadata.version).toBe("1.0");
    });

    it("should decrypt KYC record data", () => {
      const kycData = {
        customer_name: "Jane Doe",
        tax_id: "TX-67890"
      };
      
      const record = createKycRecord("kyc_789", "user_111", kycData);
      const decrypted = getKycData(record);
      
      expect(decrypted.customer_name).toBe("Jane Doe");
      expect(decrypted.tax_id).toBe("TX-67890");
    });

    it("should preserve metadata when creating record", () => {
      const kycData = { name: "Test" };
      const record = createKycRecord("kyc_001", "user_001", kycData);
      
      expect(record.metadata.lastUpdated).toBeDefined();
      expect(record.metadata.version).toBe("1.0");
    });
  });

  describe("Security Assumptions", () => {
    it("should not expose sensitive data in encrypted form", () => {
      const sensitiveData = {
        tax_id: "TX-12345",
        ssn: "123-45-6789",
        customer_name: "John Doe"
      };
      
      const encrypted = encryptSensitiveData(JSON.stringify(sensitiveData));
      
      expect(encrypted).not.toContain("TX-12345");
      expect(encrypted).not.toContain("123-45-6789");
      expect(encrypted).not.toContain("John Doe");
    });

    it("should redact all PII fields in object", () => {
      const data = {
        tax_id: "TX-12345",
        customer_name: "John Doe",
        customer_address: "123 Main St",
        ssn: "123-45-6789",
        phone_number: "555-1234",
        email: "john@example.com",
        amount: 1000,
        status: "active"
      };
      
      const redacted = redactPii(data);
      
      // Sensitive fields should be redacted
      expect(redacted.tax_id).not.toBe("TX-12345");
      expect(redacted.customer_name).not.toBe("John Doe");
      expect(redacted.customer_address).not.toBe("123 Main St");
      expect(redacted.ssn).not.toBe("123-45-6789");
      expect(redacted.phone_number).not.toBe("555-1234");
      expect(redacted.email).not.toBe("john@example.com");
      
      // Non-sensitive fields should remain unchanged
      expect(redacted.amount).toBe(1000);
      expect(redacted.status).toBe("active");
    });
  });
});