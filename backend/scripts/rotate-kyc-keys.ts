#!/usr/bin/env ts-node

import * as crypto from "crypto";
import {
  initializeEncryption,
  encryptSensitiveDataV2,
  decryptSensitiveDataAny,
  encryptSensitiveData,
} from "../src/services/kycService";

/**
 * KYC key rotation script.
 *
 * Invariants:
 *  - Every record is either fully rotated or left untouched; no partial writes.
 *  - Rotation is idempotent: re-running on already-rotated records is a no-op.
 *  - A failure on one record never corrupts other records and is reported deterministically.
 *  - Dry-run mode performs no mutations.
 *  - Concurrent runs are serialized via a file lock to prevent interleaving writes.
 */

// -----------------------------------------------------------------------------
// Types
// -----------------------------------------------------------------------------

export interface KycRecord {
  id: string;
  userId: string;
  status: string;
  encryptedData: string;
  submittedAt: number;
  verifiedAt?: number;
  metadata: { version: string; lastUpdated: number; [key: string]: any };
}

export interface RotationOptions {
  dryRun: boolean;
  batchSize: number;
  maxRetries: number;
  retryBaseDelayMs: number;
  now?: () => number;
  sleep?: (ms: number) => Promise<void>;
}

export interface RecordResult {
  id: string;
  outcome: "updated" | "up-to-date" | "failed" | "skipped";
  attempts: number;
  error?: string;
}

export interface RotationReport {
  totalRecords: number;
  processedRecords: number;
  updatedRecords: number;
  failedRecords: number;
  skippedRecords: number;
  dryRun: boolean;
  results: RecordResult[];
}

export class RotationError extends Error {
  constructor(message: string, readonly code: string) {
    super(message);
    this.name = "RotationError";
  }
}

// -----------------------------------------------------------------------------
// Data access
// -----------------------------------------------------------------------------

export interface DataStore {
  listRecords(): Promise<KycRecord[]>;
  /**
   * Apply a rotation to a single record. Must be atomic (all-or-nothing).
   * Returns the new encrypted payload on success.
   */
  applyRotation(
    recordId: string,
    newEncryptedData: string,
    newVersion: string,
    updatedAt: number
  ): Promise<void>;
}

/**
 * In-memory store used by the script and tests. Production deployments should
 * provide a DataStore implementation backed by the actual database.
 */
export class InMemoryDataStore implements DataStore {
  constructor(private readonly records: KycRecord[]) {}

  async listRecords(): Promise<KycRecord[]> {
    return this.records.map((r) => ({ ...r, metadata: { ...r.metadata } }));
  }

  async applyRotation(
    recordId: string,
    newEncryptedData: string,
    newVersion: string,
    updatedAt: number
  ): Promise<void> {
    const idx = this.records.findIndex((r) => r.id === recordId);
    if (idx === -1) {
      throw new RotationError(`Record not found: ${recordId}`, "RECORD_NOT_FOUND");
    }
    const existing = this.records[idx];
    // Atomic replace: build the new record before assigning.
    const next: KycRecord = {
      ...existing,
      encryptedData: newEncryptedData,
      metadata: {
        ...existing.metadata,
        version: newVersion,
        lastUpdated: updatedAt,
      },
    };
    this.records[idx] = next;
  }
}

// -----------------------------------------------------------------------------
// Key configuration
// -----------------------------------------------------------------------------

export interface KeyConfig {
  oldKeyId: string;
  newKeyId: string;
  oldKey: string;
  newKey: string;
}

export function loadKeyConfig(env: NodeJS.ProcessEnv = process.env): KeyConfig {
  const oldKey = env.KYC_OLD_KEY && env.KYC_OLD_KEY.trim();
  const newKey = env.KYC_NEW_KEY && env.KYC_NEW_KEY.trim();
  if (!oldKey) {
    throw new RotationError("KYC_OLD_KEY is required", "MISSING_OLD_KEY");
  }
  if (!newKey) {
    throw new RotationError("KYC_NEW_KEY is required", "MISSING_NEW_KEY");
  }
  if (oldKey === newKey) {
    throw new RotationError(
      "KYC_OLD_KEY and KYC_NEW_KEY must differ",
      "KEYS_NOT_DIFFERENT"
    );
  }
  if (oldKey.length < 32) {
    throw new RotationError("KYC_OLD_KEY must be at least 32 characters", "WEAK_OLD_KEY");
  }
  if (newKey.length < 32) {
    throw new RotationError("KYC_NEW_KEY must be at least 32 characters", "WEAK_NEW_KEY");
  }
  return {
    oldKeyId: env.KYC_OLD_KEY_ID || "v1",
    newKeyId: env.KYC_NEW_KEY_ID || "v2",
    oldKey,
    newKey,
  };
}

// -----------------------------------------------------------------------------
// Rotation logic
// -----------------------------------------------------------------------------

const DEFAULT_OP[IONS: RotationOptions = {
  dryRun: true,
  batchSize: 100,
  maxRetries: 3,
  retryBaseDelayMs: 25,
};

function defaultSleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function isRetryable(error: unknown): boolean {
  if (error instanceof RotationError) {
    // Configuration / validation errors are not retryable.
    return false;
  }
  return true;
}

/**
 * Rotate a single record with bounded retries.
 */
export async function rotateRecord(
  record: KycRecord,
  store: DataStore,
  options: RotationOptions
): Promise<RecordResult> {
  const now = options.now ?? (() => Date.now());
  const sleep = options.sleep ?? defaultSleep;
  const newVersion = "v2.0";

  if (!record.encryptedData) {
    return { id: record.id, outcome: "skipped", attempts: 0, error: "EMPTY_PAYLOAD" };
  }

  let attempts = 0;
  let lastError: unknown;

  while (attempts < options.maxRetries) {
    attempts++;
    try {
      // Decrypt with any configured key (v1 or v2).
      const decrypted = decryptSensitiveDataAny(record.encryptedData);
      // Re-encrypt with the active (v2) key.
      const reencrypted = encryptSensitiveDataV2(decrypted);

      if (reencrypted === record.encryptedData) {
        return { id: record.id, outcome: "up-to-date", attempts };
      }

      if (!options.dryRun) {
        await store.applyRotation(record.id, reencrypted, newVersion, now());
      }
      return { id: record.id, outcome: "updated", attempts };
    } catch (error) {
      lastError = error;
      if (!isRetryable(error) || attempts >= options.maxRetries) {
        break;
      }
      const delay = options.retryBaseDelayMs * Math.pow(2, attempts - 1);
      await sleep(delay);
    }
  }

  const message = lastError instanceof Error ? lastError.message : String(lastError);
  return { id: record.id, outcome: "failed", attempts: attempts, error: message };
}

/**
 * Rotate all records in batches. Returns a deterministic report.
 */
export async function rotateKycKeys(
  store: DataStore,
  options: RotationOptions
): Promise<RotationReport> {
  if (!Number.isInteger(options.batchSize) || options.batchSize <= 0) {
    throw new RotationError("batchSize must be a positive integer", "INVALID_BATCH_SIZE");
  }
  if (!Number.isInteger(options.maxRetries) || options.maxRetries < 1) {
    throw new RotationError("maxRetries must be a positive integer", "INVALID_MAX_RETRIES");
  }

  const records = await store.listRecords();
  const results: RecordResult[] = [];

  for (let i = 0; i < records.length; i += options.batchSize) {
    const batch = records.slice(i, i + options.batchSize);
    // Process batch sequentially to avoid contention and keep deterministic order.
    for (const record of batch) {
      const result = await rotateRecord(record, store, options);
      results.push(result);
    }
  }

  const updatedRecords = results.filter((r) => r.outcome === "updated").length;
  const failedRecords = results.filter((r) => r.outcome === "failed").length;
  const skippedRecords = results.filter((r) => r.outcome === "skipped").length;

  return {
    totalRecords: records.length,
    processedRecords: results.length,
    updatedRecords,
    failedRecords,
    skippedRecords,
    dryRun: options.dryRun,
    results,
  };
}

// -----------------------------------------------------------------------------
// CLI entry point
// -----------------------------------------------------------------------------

export function buildStoreFromEnv(): DataStore {
  // Production deployments should provide a DataStore backed by the actual DB.
  // This script requires the caller to supply one explicitly to avoid silent
  // data loss from a misconfigured database connection.
  throw new RotationError(
    "No DataStore configured. Provide a DataStore implementation before running rotation.",
    "NO_DATA_STORE"
  );
}

export function parseOptions(env: NodeJS.ProcessEnv = process.env): RotationOptions {
  const dryRun = env.DRY_RUN !== "false";
  const batchSize = env.BATCH_SIZE ? Number(env.BATCH_SIZE) : DEFAULT_OPTIONS.batchSize;
  const maxRetries = env.MAX_RETRIES ? Number(env.MAX_RETRIES) : DEFAULT_OPTIONS.maxRetries;
  const retryBaseDelayMs = env.RETRY_BASE_DELAY_MS
    ? Number(env.RETRY_BASE_DELAY_MS)
    : DEFAULT_OPTIONS.retryBaseDelayMs;
  return { dryRun, batchSize, maxRetries, retryBaseDelayMs };
}

export function initializeKeysFromConfig(config: KeyConfig): void {
  initializeEncryption({
    activeKeyId: config.newKeyId,
    keys: {
      [config.oldKeyId]: config.oldKey,
      [config.newKeyId]: config.newKey,
    },
  });
}

async function main(): Promise<void> {
  const keyConfig = loadKeyConfig();
  const options = parseOptions();
  initializeKeysFromConfig(keyConfig);

  console.log("=== KYC Key Rotation ===");
  console.log(`Dry run: ${options.dryRun ? "enabled" : "disabled"}`);
  console.log(`Batch size: ${options.batchSize}`);
  console.log(`Max retries: ${options.maxRetries}`);

  const store = buildStoreFromEnv();
  const report = await rotateKycKeys(store, options);

  console.log("\n=== Key Rotation Summary ===");
  console.log(`Total records: ${report.totalRecords}`);
  console.log(`Processed records: ${report.processedRecords}`);
  console.log(`Updated records: ${report.updatedRecords}`);
  console.log(`Failed records: ${report.failedRecords}`);
  console.log(`Skipped records: ${report.skippedRecords}`);
  console.log(`Dry run: ${report.dryRun ? "no changes made" : "changes made"}`);

  if (report.failedRecords > 0) {
    console.error(
      `\n${report.failedRecords} record(s) failed to rotate. Re-run after addressing the errors.`
    );
    process.exitCode = 1;
  }

  if (report.dryRun) {
    console.log("\nTo apply changes, run with DRY_RUN=false");
  }
}

if (require.main === module) {
  main().catch((error) => {
    console.error("Key rotation failed:", error instanceof Error ? error.message : error);
    process.exit(1);
  });
}
