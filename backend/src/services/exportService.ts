/**
 * exportService.ts
 *
 * Manages the creation, signing, and retrieval of file exports.
 *
 * ## HMAC signing (MITM-body-swap defence)
 *
 * When a file is written via `createExport`, the raw bytes are fed through
 * Node's `crypto.createHmac` in a streaming fashion — the HMAC is updated
 * chunk-by-chunk as the data is produced, exactly as it would be if the data
 * were being piped to disk or a blob store.  The finalised hex digest is stored
 * alongside the file metadata and later served in the `X-Body-Signature`
 * response header.  Downstream consumers can reproduce the HMAC over the
 * received bytes with the shared secret to verify integrity end-to-end.
 *
 * ## Timing-safe verification
 *
 * Any comparison between a candidate signature and the stored signature uses
 * `crypto.timingSafeEqual` to prevent timing side-channel attacks.  Callers
 * should use `verifySignature(token, candidateHex)` rather than comparing
 * strings directly.
 *
 * ## Key management
 *
 * The HMAC secret is read from the `EXPORT_HMAC_SECRET` environment variable.
 * A hard-coded fallback is provided for development/test environments only —
 * production deployments MUST set this variable to a securely generated value.
 */

import * as crypto from "crypto";
import { Readable } from "stream";
import {
  ExportRecord,
  ExportStatus,
  CreateExportResult,
} from "../types/export";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

export const HMAC_ALGORITHM = "sha256";

/**
 * Default secret used when `EXPORT_HMAC_SECRET` is not set.
 * ⚠️  NEVER use this value in production.
 */
const FALLBACK_DEV_SECRET = "quicklendx-dev-secret-do-not-use-in-prod";

/** How long (ms) an export record stays valid before it is considered expired. */
const EXPORT_TTL_MS = 15 * 60 * 1000; // 15 minutes

// ---------------------------------------------------------------------------
// ExportService (singleton)
// ---------------------------------------------------------------------------

export class ExportService {
  private static instance: ExportService;

  /** In-memory store keyed by token.  Replace with DB/blob-store in production. */
  private readonly records: Map<string, ExportRecord> = new Map();

  private constructor() {}

  // --------------------------------------------------------------------------
  // Singleton access
  // --------------------------------------------------------------------------

  public static getInstance(): ExportService {
    if (!ExportService.instance) {
      ExportService.instance = new ExportService();
    }
    return ExportService.instance;
  }

  /** Replaces the singleton.  Used only in tests. */
  public static resetInstance(): void {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (ExportService as any).instance = undefined;
  }

  // --------------------------------------------------------------------------
  // HMAC secret resolution
  // --------------------------------------------------------------------------

  /** Returns the HMAC secret, preferring the environment variable. */
  public getHmacSecret(): string {
    return process.env.EXPORT_HMAC_SECRET ?? FALLBACK_DEV_SECRET;
  }

  // --------------------------------------------------------------------------
  // Streaming HMAC computation
  // --------------------------------------------------------------------------

  /**
   * Computes an HMAC-SHA256 digest over a stream of bytes.
   *
   * The function processes each chunk from the source stream and feeds it into
   * the HMAC context incrementally (no buffering of the full file).  Resolves
   * with the hex-encoded digest once the stream ends.
   *
   * @param source  A Readable stream (or async iterable) supplying the bytes.
   * @param secret  The HMAC signing key.
   * @param algorithm  The hash algorithm to use (default: "sha256").
   */
  public async computeStreamingHmac(
    source: Readable,
    secret: string,
    algorithm: string = HMAC_ALGORITHM
  ): Promise<string> {
    const hmac = crypto.createHmac(algorithm, secret);

    return new Promise<string>((resolve, reject) => {
      source.on("data", (chunk: Buffer | string) => {
        hmac.update(chunk);
      });
      source.on("end", () => {
        resolve(hmac.digest("hex"));
      });
      source.on("error", (err) => {
        reject(err);
      });
    });
  }

  // --------------------------------------------------------------------------
  // Export creation
  // --------------------------------------------------------------------------

  /**
   * Creates a new export record:
   *  1. Generates a random 32-byte hex download token.
   *  2. Wraps the file bytes in a Readable stream.
   *  3. Computes the HMAC while consuming the stream (simulating a streaming
   *     write to persistent storage).
   *  4. Stores the record with the computed signature.
   *
   * @param fileBytes    The raw bytes of the file to export.
   * @param filename     Suggested filename for Content-Disposition.
   * @param contentType  MIME type of the file.
   */
  public async createExport(
    fileBytes: Buffer,
    filename: string,
    contentType: string
  ): Promise<CreateExportResult> {
    const token = crypto.randomBytes(32).toString("hex");
    const secret = this.getHmacSecret();

    // Wrap the file bytes in a Readable stream so HMAC is computed
    // chunk-by-chunk, exactly mirroring a real streaming write pipeline.
    const readable = Readable.from(
      (async function* () {
        yield fileBytes;
      })()
    );

    const signature = await this.computeStreamingHmac(readable, secret, HMAC_ALGORITHM);

    const now = Date.now();
    const record: ExportRecord = {
      token,
      fileBuffer: fileBytes,
      filename,
      contentType,
      status: ExportStatus.Ready,
      createdAt: now,
      expiresAt: now + EXPORT_TTL_MS,
      signature,
      signatureAlgorithm: HMAC_ALGORITHM,
    };

    this.records.set(token, record);

    return {
      token,
      filename,
      signature,
      signatureAlgorithm: HMAC_ALGORITHM,
    };
  }

  // --------------------------------------------------------------------------
  // Record retrieval
  // --------------------------------------------------------------------------

  /**
   * Returns the export record for the given token, or undefined if not found
   * or expired.
   */
  public getExport(token: string): ExportRecord | undefined {
    const record = this.records.get(token);
    if (!record) return undefined;

    if (Date.now() > record.expiresAt) {
      record.status = ExportStatus.Expired;
      return record; // still return it so the controller can issue a 410
    }

    return record;
  }

  // --------------------------------------------------------------------------
  // Timing-safe signature verification
  // --------------------------------------------------------------------------

  /**
   * Compares a candidate hex signature against the stored signature for a
   * given token using `crypto.timingSafeEqual` to prevent timing attacks.
   *
   * Returns `false` (rather than throwing) when:
   *  - The record is not found.
   *  - The record has no stored signature.
   *  - The lengths differ (would panic `timingSafeEqual`).
   */
  public verifySignature(token: string, candidateHex: string): boolean {
    const record = this.records.get(token);
    if (!record?.signature) return false;

    const stored = Buffer.from(record.signature, "hex");
    const candidate = Buffer.from(candidateHex, "hex");

    // timingSafeEqual requires identical byte lengths
    if (stored.length !== candidate.length) return false;

    return crypto.timingSafeEqual(stored, candidate);
  }

  /**
   * Recomputes the HMAC over a buffer and compares it against the stored
   * signature for the given token using constant-time comparison.
   *
   * Useful for on-demand re-verification when the file bytes are available
   * (e.g. after fetching from a blob store before serving to the client).
   */
  public verifyFileIntegrity(token: string, fileBytes: Buffer): boolean {
    const record = this.records.get(token);
    if (!record?.signature || !record.signatureAlgorithm) return false;

    const secret = this.getHmacSecret();
    const recomputed = crypto
      .createHmac(record.signatureAlgorithm, secret)
      .update(fileBytes)
      .digest("hex");

    return this.verifySignature(token, recomputed);
  }

  // --------------------------------------------------------------------------
  // Test helpers
  // --------------------------------------------------------------------------

  /** Clears all records.  Use only in tests. */
  public clearRecords(): void {
    this.records.clear();
  }

  /**
   * Directly mutates the stored file bytes for a token.
   * Used ONLY in tests to simulate a body-swap / file-corruption attack.
   */
  public _testMutateFileBytes(token: string, tamperedBytes: Buffer): void {
    const record = this.records.get(token);
    if (!record) throw new Error(`No record for token: ${token}`);
    record.fileBuffer = tamperedBytes;
  }
}

export const exportService = ExportService.getInstance();
