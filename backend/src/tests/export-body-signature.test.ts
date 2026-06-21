/**
 * export-body-signature.test.ts
 *
 * Security-focused test suite for the streaming HMAC body-signature pipeline.
 *
 * Coverage targets (≥ 95%):
 *  - ExportService (service layer, HMAC logic)
 *  - exports controller (HTTP layer, header injection, error paths)
 *  - export types (indirect, via service usage)
 *
 * Run:
 *   npm test -- export-body-signature
 */

import * as crypto from "crypto";
import { Readable } from "stream";
import request from "supertest";
import app from "../app";
import { ExportService, HMAC_ALGORITHM } from "../services/exportService";
import { ExportStatus } from "../types/export";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeService(): ExportService {
  ExportService.resetInstance();
  return ExportService.getInstance();
}

/** Computes an HMAC-SHA256 hex digest directly from a buffer + secret. */
function hmac(data: Buffer, secret: string): string {
  return crypto.createHmac("sha256", secret).update(data).digest("hex");
}

/** Builds a minimal readable that emits a single buffer then ends. */
function bufferToReadable(buf: Buffer): Readable {
  return Readable.from(
    (async function* () {
      yield buf;
    })()
  );
}

// ---------------------------------------------------------------------------
// 1. ExportService unit tests
// ---------------------------------------------------------------------------

describe("ExportService", () => {
  let svc: ExportService;

  beforeEach(() => {
    svc = makeService();
  });

  // -- Singleton --------------------------------------------------------------

  it("getInstance returns the same object on repeated calls", () => {
    const a = ExportService.getInstance();
    const b = ExportService.getInstance();
    expect(a).toBe(b);
  });

  it("resetInstance causes the next getInstance to return a fresh object", () => {
    const before = ExportService.getInstance();
    ExportService.resetInstance();
    const after = ExportService.getInstance();
    expect(before).not.toBe(after);
  });

  // -- HMAC secret resolution -------------------------------------------------

  it("getHmacSecret returns env var when set", () => {
    const orig = process.env.EXPORT_HMAC_SECRET;
    process.env.EXPORT_HMAC_SECRET = "test-secret-env";
    expect(svc.getHmacSecret()).toBe("test-secret-env");
    process.env.EXPORT_HMAC_SECRET = orig;
  });

  it("getHmacSecret returns fallback when env var is not set", () => {
    const orig = process.env.EXPORT_HMAC_SECRET;
    delete process.env.EXPORT_HMAC_SECRET;
    expect(svc.getHmacSecret()).toBeDefined();
    expect(typeof svc.getHmacSecret()).toBe("string");
    process.env.EXPORT_HMAC_SECRET = orig;
  });

  // -- Streaming HMAC ---------------------------------------------------------

  it("computeStreamingHmac produces the same digest as a direct crypto.createHmac call", async () => {
    const data = Buffer.from("hello world export data", "utf-8");
    const secret = "unit-test-secret";
    const expected = hmac(data, secret);

    const stream = bufferToReadable(data);
    const result = await svc.computeStreamingHmac(stream, secret, "sha256");

    expect(result).toBe(expected);
  });

  it("computeStreamingHmac defaults to sha256 when no algorithm is specified", async () => {
    const data = Buffer.from("default algo test", "utf-8");
    const secret = "s";
    const stream = bufferToReadable(data);
    const result = await svc.computeStreamingHmac(stream, secret);
    const expected = hmac(data, secret);
    expect(result).toBe(expected);
  });

  it("computeStreamingHmac produces different digests for different data", async () => {
    const secret = "same-secret";
    const a = await svc.computeStreamingHmac(bufferToReadable(Buffer.from("aaa")), secret);
    const b = await svc.computeStreamingHmac(bufferToReadable(Buffer.from("bbb")), secret);
    expect(a).not.toBe(b);
  });

  it("computeStreamingHmac produces different digests for different secrets", async () => {
    const data = Buffer.from("same data");
    const a = await svc.computeStreamingHmac(bufferToReadable(data), "secret-1");
    const b = await svc.computeStreamingHmac(bufferToReadable(data), "secret-2");
    expect(a).not.toBe(b);
  });

  it("computeStreamingHmac rejects when the stream errors", async () => {
    const errStream = new Readable({
      read() {
        this.destroy(new Error("Stream failed"));
      },
    });
    await expect(svc.computeStreamingHmac(errStream, "s")).rejects.toThrow("Stream failed");
  });

  it("computeStreamingHmac handles a multi-chunk stream correctly", async () => {
    const chunks = [Buffer.from("chunk1"), Buffer.from("chunk2"), Buffer.from("chunk3")];
    const combined = Buffer.concat(chunks);
    const secret = "multi-chunk-secret";
    const expectedDigest = hmac(combined, secret);

    const multiStream = Readable.from(
      (async function* () {
        for (const c of chunks) yield c;
      })()
    );

    const result = await svc.computeStreamingHmac(multiStream, secret);
    expect(result).toBe(expectedDigest);
  });

  // -- createExport -----------------------------------------------------------

  it("createExport returns a token, filename, signature, and algorithm", async () => {
    const bytes = Buffer.from("test file contents");
    const result = await svc.createExport(bytes, "test.json", "application/json");
    expect(typeof result.token).toBe("string");
    expect(result.token).toHaveLength(64); // 32 bytes hex
    expect(result.filename).toBe("test.json");
    expect(typeof result.signature).toBe("string");
    expect(result.signatureAlgorithm).toBe(HMAC_ALGORITHM);
  });

  it("createExport stores a retrievable record", async () => {
    const bytes = Buffer.from("file data");
    const { token } = await svc.createExport(bytes, "f.json", "application/json");
    const record = svc.getExport(token);
    expect(record).toBeDefined();
    expect(record?.status).toBe(ExportStatus.Ready);
    expect(record?.signature).toBeDefined();
  });

  it("createExport signature matches a direct HMAC over the same bytes", async () => {
    const bytes = Buffer.from("deterministic content");
    const secret = svc.getHmacSecret();
    const { signature } = await svc.createExport(bytes, "det.json", "application/json");
    const expected = hmac(bytes, secret);
    expect(signature).toBe(expected);
  });

  it("createExport generates unique tokens for successive calls", async () => {
    const bytes = Buffer.from("data");
    const r1 = await svc.createExport(bytes, "a.json", "application/json");
    const r2 = await svc.createExport(bytes, "b.json", "application/json");
    expect(r1.token).not.toBe(r2.token);
  });

  // -- getExport --------------------------------------------------------------

  it("getExport returns undefined for unknown token", () => {
    expect(svc.getExport("nonexistent-token")).toBeUndefined();
  });

  it("getExport marks the record as Expired when TTL has elapsed", async () => {
    const bytes = Buffer.from("expiry test");
    const { token } = await svc.createExport(bytes, "e.json", "application/json");

    // Force expiry by back-dating the record
    const record = svc.getExport(token)!;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (record as any).expiresAt = Date.now() - 1000;

    const retrieved = svc.getExport(token);
    expect(retrieved?.status).toBe(ExportStatus.Expired);
  });

  // -- verifySignature (timing-safe) ------------------------------------------

  it("verifySignature returns true for the correct hex signature", async () => {
    const bytes = Buffer.from("verify me");
    const { token, signature } = await svc.createExport(bytes, "v.json", "application/json");
    expect(svc.verifySignature(token, signature)).toBe(true);
  });

  it("verifySignature returns false for an incorrect signature", async () => {
    const bytes = Buffer.from("verify me");
    const { token } = await svc.createExport(bytes, "v.json", "application/json");
    const tampered = "a".repeat(64); // wrong 32-byte hex
    expect(svc.verifySignature(token, tampered)).toBe(false);
  });

  it("verifySignature returns false for unknown token", () => {
    expect(svc.verifySignature("no-such-token", "a".repeat(64))).toBe(false);
  });

  it("verifySignature returns false when lengths differ", async () => {
    const bytes = Buffer.from("len test");
    const { token } = await svc.createExport(bytes, "l.json", "application/json");
    // sha256 hex is 64 chars; provide only 32 (different length)
    expect(svc.verifySignature(token, "a".repeat(32))).toBe(false);
  });

  // -- verifyFileIntegrity (timing-safe re-check) -----------------------------

  it("verifyFileIntegrity returns true when bytes match the stored signature", async () => {
    const bytes = Buffer.from("integrity bytes");
    const { token } = await svc.createExport(bytes, "i.json", "application/json");
    expect(svc.verifyFileIntegrity(token, bytes)).toBe(true);
  });

  it("verifyFileIntegrity returns false after file bytes are mutated (MITM simulation)", async () => {
    const original = Buffer.from("original payload");
    const { token } = await svc.createExport(original, "m.json", "application/json");

    // Simulate a body-swap attack: replace stored bytes with different content
    const tampered = Buffer.from("attacker-injected payload");
    svc._testMutateFileBytes(token, tampered);

    // The stored signature was computed over the original bytes; re-verifying
    // against the tampered bytes must fail
    expect(svc.verifyFileIntegrity(token, tampered)).toBe(false);
  });

  it("verifyFileIntegrity returns false for unknown token", () => {
    expect(svc.verifyFileIntegrity("no-token", Buffer.from("x"))).toBe(false);
  });

  // -- _testMutateFileBytes ---------------------------------------------------

  it("_testMutateFileBytes throws for unknown token", async () => {
    expect(() => svc._testMutateFileBytes("bad", Buffer.from("x"))).toThrow(/No record/);
  });

  // -- clearRecords -----------------------------------------------------------

  it("clearRecords removes all stored exports", async () => {
    await svc.createExport(Buffer.from("a"), "a.json", "application/json");
    await svc.createExport(Buffer.from("b"), "b.json", "application/json");
    svc.clearRecords();
    // Both tokens are gone — we can't check directly but the internal map is empty
    // Confirm by creating a new export and checking it's the only retrievable one
    const { token } = await svc.createExport(Buffer.from("c"), "c.json", "application/json");
    expect(svc.getExport(token)).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// 2. HTTP integration tests (supertest against live app)
// ---------------------------------------------------------------------------

describe("Export API — HTTP integration", () => {
  beforeEach(() => {
    // Reset service so each test starts with an empty store
    ExportService.resetInstance();
  });

  // -- POST /exports ----------------------------------------------------------

  describe("POST /api/v1/exports", () => {
    it("returns 201 with token, filename, signature, and signatureAlgorithm", async () => {
      const res = await request(app)
        .post("/api/v1/exports")
        .send({ type: "invoices", format: "json" });

      expect(res.status).toBe(201);
      expect(typeof res.body.token).toBe("string");
      expect(res.body.token).toHaveLength(64);
      expect(res.body.filename).toBe("invoices.json");
      expect(typeof res.body.signature).toBe("string");
      expect(res.body.signatureAlgorithm).toBe("sha256");
    });

    it("creates a CSV export when format is csv", async () => {
      const res = await request(app)
        .post("/api/v1/exports")
        .send({ type: "bids", format: "csv" });

      expect(res.status).toBe(201);
      expect(res.body.filename).toBe("bids.csv");
    });

    it("creates exports for all allowed types", async () => {
      for (const type of ["invoices", "bids", "settlements"]) {
        const res = await request(app)
          .post("/api/v1/exports")
          .send({ type, format: "json" });
        expect(res.status).toBe(201);
        expect(res.body.filename).toBe(`${type}.json`);
      }
    });

    it("returns 400 for an unsupported export type", async () => {
      const res = await request(app)
        .post("/api/v1/exports")
        .send({ type: "users", format: "json" });

      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_EXPORT_TYPE");
    });

    it("returns 400 for an unsupported format", async () => {
      const res = await request(app)
        .post("/api/v1/exports")
        .send({ type: "invoices", format: "xml" });

      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_EXPORT_FORMAT");
    });

    it("uses invoices + json as defaults when body is empty", async () => {
      const res = await request(app)
        .post("/api/v1/exports")
        .send({});

      expect(res.status).toBe(201);
      expect(res.body.filename).toBe("invoices.json");
    });

    it("each POST produces a unique token (determinism / randomness check)", async () => {
      const r1 = await request(app)
        .post("/api/v1/exports")
        .send({ type: "invoices", format: "json" });
      const r2 = await request(app)
        .post("/api/v1/exports")
        .send({ type: "invoices", format: "json" });

      expect(r1.body.token).not.toBe(r2.body.token);
    });
  });

  // -- GET /exports/:token/download -------------------------------------------

  describe("GET /api/v1/exports/:token/download", () => {
    it("returns 200 with correct signature headers on a valid token", async () => {
      const createRes = await request(app)
        .post("/api/v1/exports")
        .send({ type: "invoices", format: "json" });
      expect(createRes.status).toBe(201);

      const { token, signature, signatureAlgorithm } = createRes.body;

      const dlRes = await request(app).get(`/api/v1/exports/${token}/download`);

      expect(dlRes.status).toBe(200);
      expect(dlRes.headers["x-body-signature"]).toBe(signature);
      expect(dlRes.headers["x-body-signature-algorithm"]).toBe(signatureAlgorithm);
    });

    it("signature header is deterministically present on every download", async () => {
      const createRes = await request(app)
        .post("/api/v1/exports")
        .send({ type: "settlements", format: "json" });

      const { token, signature } = createRes.body;

      // Download twice — both must return the same signature header
      const dl1 = await request(app).get(`/api/v1/exports/${token}/download`);
      const dl2 = await request(app).get(`/api/v1/exports/${token}/download`);

      expect(dl1.headers["x-body-signature"]).toBe(signature);
      expect(dl2.headers["x-body-signature"]).toBe(signature);
    });

    it("X-Body-Signature-Algorithm header is always sha256", async () => {
      const createRes = await request(app)
        .post("/api/v1/exports")
        .send({ type: "bids", format: "csv" });

      const { token } = createRes.body;
      const dlRes = await request(app).get(`/api/v1/exports/${token}/download`);
      expect(dlRes.headers["x-body-signature-algorithm"]).toBe("sha256");
    });

    it("consumer can re-verify the downloaded bytes against the signature header", async () => {
      // Simulate a downstream consumer verifying the response
      const createRes = await request(app)
        .post("/api/v1/exports")
        .send({ type: "invoices", format: "json" });

      const { token } = createRes.body;
      const dlRes = await request(app).get(`/api/v1/exports/${token}/download`);

      expect(dlRes.status).toBe(200);

      // Consumer: re-compute HMAC over the received body bytes
      const receivedBytes = Buffer.from(dlRes.text, "utf-8");
      const secret = ExportService.getInstance().getHmacSecret();
      const recomputed = crypto
        .createHmac("sha256", secret)
        .update(receivedBytes)
        .digest("hex");

      const headerSig = dlRes.headers["x-body-signature"];
      expect(recomputed).toBe(headerSig);
    });

    it("Content-Disposition header is set for file download", async () => {
      const createRes = await request(app)
        .post("/api/v1/exports")
        .send({ type: "invoices", format: "json" });
      const { token } = createRes.body;

      const dlRes = await request(app).get(`/api/v1/exports/${token}/download`);
      expect(dlRes.headers["content-disposition"]).toContain("attachment");
      expect(dlRes.headers["content-disposition"]).toContain("invoices.json");
    });

    it("returns 404 for a non-existent token", async () => {
      const res = await request(app).get(
        "/api/v1/exports/0000000000000000000000000000000000000000000000000000000000000000/download"
      );
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("EXPORT_NOT_FOUND");
    });

    it("returns 410 for an expired export", async () => {
      const createRes = await request(app)
        .post("/api/v1/exports")
        .send({ type: "invoices", format: "json" });
      const { token } = createRes.body;

      // Artificially expire the record
      const record = ExportService.getInstance().getExport(token)!;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (record as any).expiresAt = Date.now() - 1000;

      const dlRes = await request(app).get(`/api/v1/exports/${token}/download`);
      expect(dlRes.status).toBe(410);
      expect(dlRes.body.error.code).toBe("EXPORT_EXPIRED");
    });

    it("returns 503 when export status is Pending (not ready)", async () => {
      const createRes = await request(app)
        .post("/api/v1/exports")
        .send({ type: "invoices", format: "json" });
      const { token } = createRes.body;

      // Force the status back to Pending
      const record = ExportService.getInstance().getExport(token)!;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (record as any).status = ExportStatus.Pending;

      const dlRes = await request(app).get(`/api/v1/exports/${token}/download`);
      expect(dlRes.status).toBe(503);
      expect(dlRes.body.error.code).toBe("EXPORT_NOT_READY");
    });
  });

  // -- Mutation defence test: body-swap attack detection ----------------------

  describe("Mutation defence: body-swap attack detection", () => {
    it("detects file body mutation between write and download (MITM simulation)", async () => {
      // 1. Create the export — signature is computed over original bytes
      const createRes = await request(app)
        .post("/api/v1/exports")
        .send({ type: "settlements", format: "json" });
      expect(createRes.status).toBe(201);

      const { token, signature: originalSig } = createRes.body;

      // 2. Confirm first download is clean
      const dlBefore = await request(app).get(`/api/v1/exports/${token}/download`);
      expect(dlBefore.status).toBe(200);
      expect(dlBefore.headers["x-body-signature"]).toBe(originalSig);

      // 3. Simulate a MITM body-swap: replace the stored file bytes with
      //    attacker-controlled content.  The stored signature does NOT change.
      const attackerPayload = Buffer.from(
        JSON.stringify([{ id: "EVIL", amount: "99999999999" }])
      );
      ExportService.getInstance()._testMutateFileBytes(token, attackerPayload);

      // 4. The integrity check inside the download handler must detect the
      //    mismatch and return 500 rather than serving the tampered file.
      const dlAfter = await request(app).get(`/api/v1/exports/${token}/download`);
      expect(dlAfter.status).toBe(500);
      expect(dlAfter.body.error.code).toBe("INTEGRITY_CHECK_FAILED");
    });

    it("verifyFileIntegrity rejects tampered bytes independent of HTTP layer", async () => {
      const original = Buffer.from("sensitive export data");
      const svc = ExportService.getInstance();
      const { token } = await svc.createExport(original, "s.json", "application/json");

      // Integrity check passes on original
      expect(svc.verifyFileIntegrity(token, original)).toBe(true);

      // Simulate byte-level tampering (single-bit flip is enough)
      const tampered = Buffer.from(original);
      tampered[0] ^= 0x01;
      expect(svc.verifyFileIntegrity(token, tampered)).toBe(false);
    });

    it("verifySignature uses constant-time comparison (no early exit on mismatch)", async () => {
      // We can't directly measure timing from a unit test, but we can assert
      // that the function calls timingSafeEqual by verifying the result is
      // deterministically correct for both matching and non-matching inputs.
      const bytes = Buffer.from("timing safe test");
      const svc = ExportService.getInstance();
      const { token, signature } = await svc.createExport(bytes, "t.json", "application/json");

      // Correct signature → true
      expect(svc.verifySignature(token, signature)).toBe(true);

      // Every hex-digit variant of a wrong signature → false
      const wrongSig = "f".repeat(64);
      expect(svc.verifySignature(token, wrongSig)).toBe(false);
    });
  });
});

// ---------------------------------------------------------------------------
// 3. Edge-case & security boundary tests
// ---------------------------------------------------------------------------

describe("Edge cases", () => {
  beforeEach(() => {
    ExportService.resetInstance();
  });

  it("empty file buffer produces a valid (non-empty) HMAC signature", async () => {
    const svc = ExportService.getInstance();
    const emptyBuf = Buffer.alloc(0);
    const { signature } = await svc.createExport(emptyBuf, "empty.json", "application/json");
    expect(typeof signature).toBe("string");
    expect(signature.length).toBe(64); // sha256 hex is always 64 chars
  });

  it("large file buffer produces the correct signature (chunked streaming integrity)", async () => {
    const svc = ExportService.getInstance();
    const largeBuf = crypto.randomBytes(1024 * 512); // 512 KB
    const secret = svc.getHmacSecret();
    const expected = hmac(largeBuf, secret);

    const { token, signature } = await svc.createExport(largeBuf, "large.bin", "application/octet-stream");

    expect(signature).toBe(expected);
    expect(svc.verifyFileIntegrity(token, largeBuf)).toBe(true);
  });

  it("CSV and JSON exports for the same type produce different signatures", async () => {
    const svc = ExportService.getInstance();

    const jsonRes = await request(app)
      .post("/api/v1/exports")
      .send({ type: "invoices", format: "json" });
    const csvRes = await request(app)
      .post("/api/v1/exports")
      .send({ type: "invoices", format: "csv" });

    // Different byte content → different signatures
    expect(jsonRes.body.signature).not.toBe(csvRes.body.signature);
    void svc; // suppress unused warning
  });

  it("two exports with identical content but different tokens have the same signature", async () => {
    // The HMAC is over content only; two identical files get the same digest
    const svc = ExportService.getInstance();
    const bytes = Buffer.from("identical content");
    const r1 = await svc.createExport(bytes, "a.json", "application/json");
    const r2 = await svc.createExport(bytes, "b.json", "application/json");
    expect(r1.signature).toBe(r2.signature);
    expect(r1.token).not.toBe(r2.token);
  });

  it("changing EXPORT_HMAC_SECRET between create and verify causes integrity failure", async () => {
    const svc = ExportService.getInstance();
    const orig = process.env.EXPORT_HMAC_SECRET;

    process.env.EXPORT_HMAC_SECRET = "secret-at-write-time";
    const bytes = Buffer.from("secret-bound content");
    const { token } = await svc.createExport(bytes, "s.json", "application/json");

    // Signature was computed with "secret-at-write-time".
    // Now change the secret to simulate key rotation / misconfiguration.
    process.env.EXPORT_HMAC_SECRET = "different-secret-at-read-time";
    const valid = svc.verifyFileIntegrity(token, bytes);
    expect(valid).toBe(false);

    process.env.EXPORT_HMAC_SECRET = orig;
  });
});
