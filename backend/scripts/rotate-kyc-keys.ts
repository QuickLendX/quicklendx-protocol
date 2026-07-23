#!/usr/bin/env ts-node

import * as crypto from "crypto";
import {
  initializeEncryption,
  encryptSensitiveDataV2,
  decryptSensitiveDataAny,
  KeyRing
} from "../src/services/kycService";

// Mock database connection (replace with actual DB setup in production)
// For this example, we'll simulate the DB
interface KycRecord {
  id: string;
  userId: string;
  status: string;
  encryptedData: string;
  submittedAt: number;
  verifiedAt?: number;
  metadata: { version: string; lastUpdated: number; [key: string]: any };
}

const mockDatabase: KycRecord[] = [
  // Example legacy v1 record
  {
    id: "kyc_123",
    userId: "user_456",
    status: "verified",
    encryptedData: "", // Will be populated with test data
    submittedAt: Date.now() - 86400000,
    metadata: { version: "1.0", lastUpdated: Date.now() - 86400000 }
  },
  // Example v2 record
  {
    id: "kyc_789",
    userId: "user_012",
    status: "submitted",
    encryptedData: "", // Will be populated with test data
    submittedAt: Date.now(),
    metadata: { version: "2.0", lastUpdated: Date.now() }
  }
];

// Configuration
const BATCH_SIZE = 100;
const DRY_RUN = process.env.DRY_RUN !== "false"; // Default to dry run
const OLD_KEY = process.env.KYC_OLD_KEY || "old-test-master-key-for-encryption-12345678901234567890";
const NEW_KEY = process.env.KYC_NEW_KEY || "new-test-master-key-for-encryption-09876543210987654321";

async function rotateKycKeys() {
  console.log("=== KYC Key Rotation ===");
  console.log(`Dry run: ${DRY_RUN ? "enabled" : "disabled"}`);
  console.log(`Batch size: ${BATCH_SIZE}`);

  // Initialize encryption with both keys
  initializeEncryption({
    activeKeyId: "v2",
    keys: {
      "v1": OLD_KEY,
      "v2": NEW_KEY
    }
  });

  console.log("\nInitializing test data...");
  // Populate mock database with test data
  initializeEncryption(OLD_KEY);
  mockDatabase[0].encryptedData = encryptSensitiveDataV2(JSON.stringify({
    customer_name: "John Doe",
    tax_id: "TX-12345"
  }));
  initializeEncryption(NEW_KEY);
  mockDatabase[1].encryptedData = encryptSensitiveDataV2(JSON.stringify({
    customer_name: "Jane Smith",
    tax_id: "TX-67890"
  }));
  // Also add a legacy v1 record for testing
  initializeEncryption(OLD_KEY);
  mockDatabase.push({
    id: "kyc_legacy",
    userId: "user_legacy",
    status: "verified",
    encryptedData: encryptSensitiveData(JSON.stringify({
      customer_name: "Legacy User",
      tax_id: "TX-00000"
    })),
    submittedAt: Date.now() - 172800000,
    metadata: { version: "1.0", lastUpdated: Date.now() - 172800000 }
  });

  console.log("\nStarting key rotation...");

  let totalRecords = mockDatabase.length;
  let processedRecords = 0;
  let updatedRecords = 0;

  // Process records in batches
  for (let i = 0; i < mockDatabase.length; i += BATCH_SIZE) {
    const batch = mockDatabase.slice(i, i + BATCH_SIZE);

    console.log(`\nProcessing batch ${Math.floor(i / BATCH_SIZE) + 1} (${i + 1} - ${Math.min(i + BATCH_SIZE, totalRecords)})`);

    for (const record of batch) {
      processedRecords++;
      console.log(`Processing record ${record.id} (${processedRecords}/${totalRecords})`);

      try {
        // Decrypt with old key
        const decryptedData = decryptSensitiveDataAny(record.encryptedData);
        // Re-encrypt with new key
        const reencryptedData = encryptSensitiveDataV2(decryptedData);

        if (record.encryptedData !== reencryptedData) {
          updatedRecords++;
          console.log(`  - Record ${record.id} will be updated`);

          if (!DRY_RUN) {
            // Update record in mock DB (replace with actual DB update in production)
            record.encryptedData = reencryptedData;
            record.metadata.version = "2.0";
            record.metadata.lastUpdated = Date.now();
          }
        } else {
          console.log(`  - Record ${record.id} already up to date`);
        }
      } catch (error) {
        console.error(`  - ERROR processing record ${record.id}:`, error);
      }
    }
  }

  console.log("\n=== Key Rotation Summary ===");
  console.log(`Total records: ${totalRecords}`);
  console.log(`Processed records: ${processedRecords}`);
  console.log(`Updated records: ${updatedRecords}`);
  console.log(`Dry run: ${DRY_RUN ? "no changes made" : "changes made"}`);

  if (DRY_RUN) {
    console.log("\nTo apply changes, run with DRY_RUN=false");
  }
}

// Execute rotation
rotateKycKeys().catch(error => {
  console.error("Key rotation failed:", error);
  process.exit(1);
});
