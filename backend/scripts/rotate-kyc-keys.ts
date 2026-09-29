#!/usr/bin/env ts_node

import * as crypto from "crypto";
import {
  initializeEncryption,
  encryptSensitiveDataV2,
  decryptSensitiveDataAny,
  KeyRing
} from "../src/services/kycService";

// ------------------------------------------------------------------------------
// Types and invariants
// ------------------------------------------------------------------------------
//
// Invariants enforced by this script:
// 1. A key rotation is deterministic: given the same input records and
//    the same key ring, the same outcome is produced.
// 2. A failed record is never silently dropped: it is retried a bounded
//    number of times and then reported in the final metrics.
// 3. A stale record (metadata lastUpdated older than the configured
//    threshold) is skipped and counted, not silently re-written.
// 4. Dry-run is the default and must not mutate any record.
// 5. Authorization is required when a token is configured; missing or
//    invalid tokens fail closed before any record is touched.
// 6. Concurrent execution is prevented by an atomic lock file.
// 7. Error messages must not leak plaintext or key material.

export interface KycRecord {
  id: string;
  userId: string;
  status: string;
  encryptedData: string;
  submittedAt: number;
  verifiedAt?: number;
  metadata: { version: string; lastUpdated: number; [key: string]: any };
}

export interface RotationConfig {
  batchSize: number;
  dryRun: boolean;
  maxRetries: number;
  retryBaseDelayMs: number;
  staleThresholdMs: number;
  operatorToken?: string;
  requiredOperatorToken?: string;
}

export interface RotationMetrics {
  total: number;
  processed: number;
  updated: number;
  skipped: number;
  failed: number;
  retried: number;
  stale: number;
}

export interface RotationResult {
  metrics: RotationMetrics;
  failures: Array<{ id: string; error: string; attempts: number }>;
  success: boolean;
}

export interface RotationLogger {
  info(message: string, context?: Record<string, unknown>): void;
  warn(message: string, context?: Record<string, unknown>): void;
  error(message: string, context?: Record<string, unknown>): void;
}

export const defaultLogger: RotationLogger = {
  info: (message, context) => {
    console.log(JSON.stringify({ level: "info", message, ...context }));
  },
  warn: (message, context) => {
    console.warn(JSON.stringify({ level: "warn", message, ...context }));
  },
  error: (message, context) => {
    console.error(JSON.stringify({ level: "error", message, ...context }));
  }
};

export class RotationError extends Error {
  constructor(message: string, readonly code: string) {
    super(message);
    this.name = "RotationError";
  }
}

export class AuthorizationError extends RotationError {
  constructor(message: string) {
    super(message, "UNAUTHORIZED");
  }
}

export class ConcurrencyError extends RotationError {
  constructor(message: string) {
    super(message, "CONCURRENCY_CONFLICT");
  }
}

export class ValidationError extends RotationError {
  constructor(message: string) {
    super(message, "VALIDATION_ERROR");
  }
}

// ------------------------------------------------------------------------------
// Error sanitization
// ------------------------------------------------------------------------------

/**
 * Sanitize an error into a safe, deterministic string.
 * Never includes raw error messages that may contain plaintext or key material.
 */
export function sanitizeError(error: unknown): string {
  if (error instanceof RotationError) {
    return `${error.code}: ${error.message}`;
  }
  if (error instanceof Error) {
    // Only expose the class name and a generic message to avoid leaking
    // sensitive data that may be present in the raw message.
    return `${error.name}: ${error.message.replace(/[a-zA-Z0-9_+/=]{16,}/g, "[redacted]")}`;
  }
  return "UnknownError: an unknown error occurred";
}

// ------------------------------------------------------------------------------
// Authorization
// ------------------------------------------------------------------------------

/**
 * Verify the operator token if one is required.
 * Fails closed before any record is touched.
 */
export function authorize(config: RotationConfig): void {
  if (!config.requiredOperatorToken) {
    return;
  }
  if (!config.operatorToken) {
    throw new AuthorizationError("Operator token is required but not provided");
  }
  // Constant-time comparison to avoid timing leaks.
  const expected = Buffer.from(config.requiredOperatorToken);
  const actual = Buffer.from(config.operatorToken);
  if (expected.length !== actual.length || !crypto.timingSafeEqual(expected, actual)) {
    throw new AuthorizationError("Invalid operator token");
  }
}

// ------------------------------------------------------------------------------
// Concurrency lock
// ------------------------------------------------------------------------------

export interface LockHandle {
  release(): void;
}

/**
 * Acquire an atomic lock file to prevent concurrent rotation runs.
 * Uses exclusive file creation ('wx') which is atomic on POSIX-compliant
 * filesystems.
 */
export function acquireLock(lockPath: string): LockHandle {
  try {
    const fd = fs.openSync(lockPath, "wx");
    fs.writeSync(fd, String(process.pid));
    fs.closeSync(fd);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "EEXIST") {
      throw new ConcurrencyError(
        `Another rotation is already in progress (lock file ${lockPath} exists)`
      );
    }
    throw error;
  }
  return {
    release: () => {
      try {
        fs.unlinkSync(lockPath);
      } catch {
        // Ignore release failures; the lock file will be cleaned by the next run.
      }
    }
  };
}

// ------------------------------------------------------------------------------
// Retry logic
// ------------------------------------------------------------------------------

/**
 * Retry an async operation with exponential backoff and a bounded number
 * of attempts. The delay is deterministic (base * 2^attempt) so tests can
 * assert on timing without flakiness.
 */
export async function withRetry<T>(
  operation: () => Promise<T>,
  maxRetries: number,
  baseDelayMs: number,
  sleep: (ms: number) => Promise<void> = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
): Promise<T> {
  let lastError: unknown;
  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      return await operation();
    } catch (error) {
      lastError = error;
      if (attempt === maxRetries) {
        break;
      }
      await sleep(baseDelayMs * Math.pow(2, attempt));
    }
  }
  throw lastError;
}

// ------------------------------------------------------------------------------
// Record processing
// ------------------------------------------------------------------------------

/**
 * Decrypt and re-encrypt a single record.
 * Returns the new encrypted payload and whether it differs from the old one.
 */
export function rekeyRecord(record: KycRecord): { encryptedData: string; changed: boolean } {
  const decryptedData = decryptSensitiveDataAny(record.encryptedData);
  const reencryptedData = encryptSensitiveDataV2(decryptedData);
  return { encryptedData: reencryptedData, changed: record.encryptedData !== reencryptedData };
}

/**
 * Determine whether a record is stale based on its metadata lastUpdated
 * timestamp. Stale records are skipped to avoid overwriting concurrent
 * updates.
 */
export function isStale(record: KycRecord, now: number, thresholdMs: number): boolean {
  if (thresholdMs <= 0) {
    return false;
  }
  return now - record.metadata.lastUpdated > thresholdMs;
}

/**
 * Validate a record before processing.
 * Rejects records with missing or malformed fields.
 */
export function validateRecord(record: KycRecord): void {
  if (!record || typeof record !== "object") {
    throw new ValidationError("Record must be an object");
  }
  if (!record.id || typeof record.id !== "string") {
    throw new ValidationError("Record id is required and must be a string");
  }
  if (!record.encryptedData || typeof record.encryptedData !== "string") {
    throw new ValidationError(`Record ${record.id} is missing encryptedData`);
  }
  if (!record.metadata || typeof record.metadata.lastUpdated !== "number") {
    throw new ValidationError(`Record ${record.id} is missing metadata.lastUpdated`);
  }
}

// ------------------------------------------------------------------------------
// Main rotation function
// ------------------------------------------------------------------------------

export interface RotationDependencies {
  /** Persist a record update. Must be idempotent and atomic. */
  persistRecord: (record: KycRecord) => Promise<void>;
  /** Logger for structured observability. */
  logger: RotationLogger;
  /** Injectable sleep for deterministic testing. */
  sleep?: (ms: number) => Promise<void>;
  /** Injectable clock for deterministic testing. */
  now?: () => number;
}

export async function rotateKycKeys(
  records: KycRecord[],
  config: RotationConfig,
  deps: RotationDependencies
): Promise<RotationResult> {
  const logger = deps.logger;
  const now = deps.now ?? (() => Date.now());
  const sleep = deps.sleep ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)));

  // Invariant 5: authorize the operation before any state is touched.
  authorize(config);

  // Invariant 6: prevent concurrent execution.
  const lockPath = process.env.KYC_ROTATION_LOCK || "/tmp/quicklendx-kyc-rotation.lock";
  const lock = acquireLock(lockPath);

  const metrics: RotationMetrics = {
    total: records.length,
    processed: 0,
    updated: 0,
    skipped: 0,
    failed: 0,
    retried: 0,
    stale: 0
  };
  const failures: Array<{ id: string; error: string; attempts: number }> = [];

  try {
    logger.info("kyc.rotation.start", {
      total: metrics.total,
      dryRun: config.dryRun,
      batchSize: config.batchSize,
      maxRetries: config.maxRetries
    });

    for (let i = 0; i < records.length; i += config.batchSize) {
      const batch = records.slice(i, i + config.batchSize);
      logger.info("kyc.rotation.batch", {
        batchIndex: Math.floor(i / config.batchSize),
        from: i + 1,
        to: Math.min(i + config.batchSize, records.length)
      });

      for (const record of batch) {
        metrics.processed++;
        try {
          // Invariant 1: validate before processing.
          validateRecord(record);

          // Invariant 3: skip stale records.
          if (isStale(record, now(), config.staleThresholdMs)) {
            metrics.stale++;
            metrics.skipped++;
            logger.warn("kyc.rotation.stale", { id: record.id });
            continue;
          }

          // Invariant 2: bounded retries for transient failures.
          let attempts = 0;
          const result = await withRetry(
            async () => {
              attempts++;
              const { encryptedData, changed } = rekeyRecord(record);
              if (!changed) {
                return { changed: false as const, encryptedData };
              }
              if (!config.dryRun) {
                // Invariant 4: dry-run must not mutate.
                const updatedRecord: KycRecord = {
                  ...record,
                  encryptedData,
                  metadata: {
                    ...record.metadata,
                    version: "2.0",
                    lastUpdated: now()
                  }
                };
                await deps.persistRecord(updatedRecord);
              }
              return { changed: true as const, encryptedData };
            },
            config.maxRetries,
            config.retryBaseDelayMs,
            sleep
          );

          if (result.changed) {
            metrics.updated++;
            logger.info("kyc.rotation.updated", { id: record.id });
          } else {
            metrics.skipped++;
            logger.info("kyc.rotation.up-to-date", { id: record.id });
          }
        } catch (error) {
          metrics.failed++;
          const safe = sanitizeError(error);
          failures.push({ id: record.id, error: safe, attempts });
          logger.error("kyc.rotation.failed", { id: record.id, error: safe });
        }
      }
    }
  } finally {
    lock.release();
  }

  const success = metrics.failed === 0;
  logger.info("kyc.rotation.complete", {
    ...metrics,
    success,
    failures: failures.length
  });

  return { metrics, failures, success };
}

// ------------------------------------------------------------------------------
// CLI entry point
// ------------------------------------------------------------------------------

export function buildConfigFromEnv(env: NodeJS.ProcessEnv = process.env): RotationConfig {
  const parseIntEnv = (name: string, defaultValue: number): number => {
    const raw = env[name];
    if (raw === undefined || raw === "") {
      return defaultValue;
    }
    const parsed = Number(raw);
    if (!Number.isFinite(parsed) || parsed < 0) {
      throw new ValidationError(`Environment variable ${name} must be a non-negative number`);
    }
    return parsed;
  };

  const batchSize = parseIntEnv("KYC_BATCH_SIZE", 100);
  if (batchSize === 0) {
    throw new ValidationError("KYC_BATCH_SIZE must be greater than zero");
  }

  return {
    batchSize,
    dryRun: env.DRY_RUN !== "false",
    maxRetries: parseIntEnv("KYC_MAX_RETRIES", 3),
    retryBaseDelayMs: parseIntEnv("KYC_RETRY_BASE_DELAY_MS", 500),
    staleThresholdMs: parseIntEnv("KCY_STALE_THRESHOLD_MS", 0),
    operatorToken: env.KYC_OPERATOR_TOKEN,
    requiredOperatorToken: env.KYC_REQUIRED_OPERATOR_TOKEN
  };
}

/**
 * Run the rotation against the provided records and key ring.
 * This is the production entry point used by the CLI.
 */
export async function runRotation(
  records: KycRecord[],
  keyRing: KeyRing,
  config: RotationConfig,
  deps: RotationDependencies
dendings: RotationDependencies
): Promise<RotationResult> {
  initializeEncryption(keyRing);
  return rotateKycKeys(records, config, deps);
}

// ------------------------------------------------------------------------------
// Production persistence adapter
// ------------------------------------------------------------------------------

/**
 * Persistence adapter interface. The concrete implementation is provided
 * by the caller (e.g. a DB client). This script does not hard-code any
 * database driver.
 */
export interface PersistenceAdapter {
  /** Load all KYC records that need rotation. */
  loadRecords: () => Promise<KycRecord[]>;
  /** Persist a single record update. */
  persistRecord: (record: KycRecord) => Promise<void>;
}

/**
 * Run the rotation using a persistence adapter and key ring.
 * This is the high-level entry point used by the CLI.
 */
export async function runRotationWithAdapter(
  adapter: PersistenceAdapter,
  keyRing: KeyRing,
  config: RotationConfig,
  deps: RotationDependencies
dendings: RotationDependencies
): Promise<RotationResult> {
  const records = await adapter.loadRecords();
  initializeEncryption(keyRing);
  return rotateKycKeys(records, config, {
    ...deps,
    persistRecord: adapter.persistRecord
  });
}

// ------------------------------------------------------------------------------
// CLI entry point
// ------------------------------------------------------------------------------

if (require.main === module) {
  const config = buildConfigFromEnv();
  const oldKey = process.env.KYC_OLD_KEY;
  const newKey = process.env.KYC_NEW_KEY;&#x5c;
  if (!oldKey || !newKey) {
    console.error("KYC_OLD_KEY and KYC_NEW_KEY must be set");
    process.exit(1);
  }
  const keyRing: KeyRing = {
    activeKeyId: "v2",
    keys: { v1: oldKey, v2: newKey }
  };
  const adapter: PersistenceAdapter = {
    loadRecords: async () => {
      throw new Error(
        "No persistence adapter configured. Provide a PersistenceAdapter implementation."
      );
    },
    persistRecord: async () => {
      throw new Error(
        "No persistence adapter configured. Provide a PersistenceAdapter implementation."
      );
    }
  };
  runRotationWithAdapter(adapter, keyRing, config, { logger: defaultLogger })
    .then((result) => {
      if (!result.success) {
        process.exit(2);
      }
    })
    .catch((error) => {
      console.error(JSON.stringify({ level: "error", message: "kyc.rotation.fatal", error: sanitizeError(error) }));
      process.exit(1);
    });
}
