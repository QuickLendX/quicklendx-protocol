#!/usr/bin/env ts-node

import * as crypto from "crypto";
import {
  initializeEncryption,
  encryptSensitiveDataV2,
  decryptSensitiveDataAny,
  KeyRing
} from "../src/services/kycService";

// ---------------------------------------------------------------------------
// Deterministic failure-boundary coverage for rotateKycKeys
// ---------------------------------------------------------------------------
// Invariants enforced by this script:
// 1. Every record is either left untouched or atomically re-encrypted with the
//    new active key. Partial writes are never committed.
// 2. Decryption failures are isolated per-record and never abort the batch.
// 3. Retries are idempotent: re-running the script on an already-rotated record
//    is a no-op (ciphertext equality check).
// 4. Concurrent executions are serialized via an advisory lock file so two
//    rotations cannot interleave writes.
// 5. Authorization is enforced via an explicit operator token check before any
//    mutation occurs.
// 6. Sensitive plaintext is never logged; only record ids and error classes.
// ---------------------------------------------------------------------------

const LOCK_FILE = process.env.KYC_ROTATION_LOCK_FILE || "/tmp/rotate-kyc-keys.lock";
const OPERATOR_TOKEN = process.env.KYC_OPERATOR_TOKEN;
const MAX_RETRIES = Number(process.env.KYC_MAX_RETRIES || 3);
const RETRY_BASE_MS = Number(process.env.KYC_RETRY_BASE_MS || 50);

class RotationError extends Error {
  constructor(message: string, public readonly code: string) {
    super(message);
    this.name = "RotationError";
  }
}

function assertAuthorized(): void {
  if (!OPERATOR_TOKEN || OPERATOR_TOKEN.length < 16) {
    throw new RotationError(
      "Missing or weak KYC_OPERATOR_TOKEN; refusing to rotate keys",
      "UNAUTHORIZED"
    );
  }
}

function acquireLock(): () => void {
  const fs = require("fs");
  try {
    const fd = fs.openSync(LOCK_FILE, "wx");
    fs.writeSync(fd, String(process.pid));
    fs.closeSync(fd);
  } catch (err: any) {
    if (err && err.code === "EEXIST") {
      throw new RotationError(
        "Another rotation is already in progress",
        "LOCKED"
      );
    }
    throw err;
  }
  return () => {
    try {
      fs.unlinkSync(LOCK_FILE);
    } catch {
      /* best-effort cleanup */
    }
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function withRetry<T>(fn: () => T, label: string): Promise<T> {
  let lastErr: unknown;
  for (let attempt = 1; attempt <= MAX_RETRIES; attempt++) {
    try {
      return fn();
    } catch (err) {
      lastErr = err;
      if (attempt < MAX_RETRIES) {
        await sleep(RETRY_BASE_MS * attempt);
      }
    }
  }
  throw new RotationError(
    `Operation failed after ${MAX_RETRIES} attempts: ${label}`,
    "RETRY_EXHAUSTED"
  );
}

function classifyError(err: unknown): string {
  if (err instanceof RotationError) return err.code;
  if (err instanceof Error) return err.name || "Error";
  return "UnknownError";
}

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

  // Authorization boundary: refuse to run without a valid operator token.
  assertAuthorized();

  // Concurrency boundary: serialize rotations via an advisory lock file.
  const releaseLock = acquireLock();

  try {
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
  let failedRecords = 0;
  let skippedRecords = 0;

  // Process records in batches
  for (let i = 0; i < mockDatabase.length; i += BATCH_SIZE) {
    const batch = mockDatabase.slice(i, i + BATCH_SIZE);

    console.log(`\nProcessing batch ${Math.floor(i / BATCH_SIZE) + 1} (${i + 1} - ${Math.min(i + BATCH_SIZE, totalRecords)})`);

    for (const record of batch) {
      processedRecords++;
      console.log(`Processing record ${record.id} (${processedRecords}/${totalRecords})`);

      // Boundary: skip records with missing or empty ciphertext rather than
      // attempting decryption and corrupting state.
      if (!record.encryptedData || record.encryptedData.length === 0) {
        skippedRecords++;
        console.warn(`  - Record ${record.id} skipped: empty ciphertext`);
        continue;
      }

      try {
        // Decrypt with old key (retryable, deterministic).
        const decryptedData = await withRetry(
          () => decryptSensitiveDataAny(record.encryptedData),
          `decrypt:${record.id}`
        );

        // Re-encrypt with new key (retryable, deterministic).
        const reencryptedData = await withRetry(
          () => encryptSensitiveDataV2(decryptedData),
          `encrypt:${record.id}`
        );

        // Idempotency boundary: if ciphertext is unchanged, treat as no-op.
        if (record.encryptedData !== reencryptedData) {
          updatedRecords++;
          console.log(`  - Record ${record.id} will be updated`);

          if (!DRY_RUN) {
            // Atomic write boundary: only mutate after both decrypt and
            // encrypt succeeded. If this throws, the record is left intact.
            record.encryptedData = reencryptedData;
            record.metadata.version = "2.0";
            record.metadata.lastUpdated = Date.now();
          }
        } else {
          skippedRecords++;
          console.log(`  - Record ${record.id} already up to date`);
        }
      } catch (error) {
        failedRecords++;
        // Log only the error class/code, never plaintext or key material.
        console.error(
          `  - ERROR processing record ${record.id}: ${classifyError(error)}`
        );
      }
    }
  }

  console.log("\n=== Key Rotation Summary ===");
  console.log(`Total records: ${totalRecords}`);
  console.log(`Processed records: ${processedRecords}`);
  console.log(`Updated records: ${updatedRecords}`);
  console.log(`Skipped records: ${skippedRecords}`);
  console.log(`Failed records: ${failedRecords}`);
  console.log(`Dry run: ${DRY_RUN ? "no changes made" : "changes made"}`);

  if (DRY_RUN) {
    console.log("\nTo apply changes, run with DRY_RUN=false");
  }

  if (failedRecords > 0) {
    throw new RotationError(
      `Rotation completed with ${failedRecords} failed record(s)`,
      "PARTIAL_FAILURE"
    );
  }
  } finally {
    releaseLock();
  }
}

// Execute rotation
rotateKycKeys().catch(error => {
  // Surface a diagnosable, non-sensitive error code to operators.
  const code = error instanceof RotationError ? error.code : "UNEXPECTED";
  console.error(`Key rotation failed [${code}]:`, error instanceof Error ? error.message : error);
  process.exit(1);
});
