import { describe, expect, it, beforeEach } from "@jest/globals";
import request from "supertest";
import app from "../src/app";
import {
  labelRecord,
  resolveEventSchemaVersion,
  CURRENT_CONTRACT_VERSION,
  CURRENT_EVENT_SCHEMA_VERSION,
  EVENT_TOPIC_SCHEMA_VERSIONS,
} from "../src/services/versioningService";
import type { VersionedRecord } from "../src/types/contract";

// ─── helpers ────────────────────────────────────────────────────────────────

/** Asserts that every field of VersionedRecord is present and well-formed. */
function expectVersionLabels(record: object): void {
  const r = record as Record<string, unknown>;
  expect(typeof r.contract_version).toBe("number");
  expect(typeof r.event_schema_version).toBe("number");
  expect(typeof r.indexed_at).toBe("string");
  expect(r.contract_version).toBeGreaterThanOrEqual(1);
  expect(r.event_schema_version).toBeGreaterThanOrEqual(1);
  expect(new Date(r.indexed_at as string).toISOString()).toBe(r.indexed_at);
}

// ─── API response version labels ─────────────────────────────────────────────

describe("API response — version labels present on all record types", () => {
  it("should include version labels on invoice list records", async () => {
    const res = await request(app).get("/api/v1/invoices");
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
    res.body.forEach((invoice: Record<string, unknown>) => {
      expectVersionLabels(invoice);
    });
  });

  it("should include version labels on a single invoice", async () => {
    const id =
      "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const res = await request(app).get(`/api/v1/invoices/${id}`);
    expect(res.status).toBe(200);
    expectVersionLabels(res.body);
  });

  it("should include version labels on bid list records", async () => {
    const res = await request(app).get("/api/v1/bids");
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
    res.body.forEach((bid: Record<string, unknown>) => {
      expectVersionLabels(bid);
    });
  });

  it("should include version labels on settlement list records", async () => {
    const res = await request(app).get("/api/v1/settlements");
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
    res.body.forEach((settlement: Record<string, unknown>) => {
      expectVersionLabels(settlement);
    });
  });

  it("should include version labels on a single settlement", async () => {
    const res = await request(app).get("/api/v1/settlements/0xsettle123");
    expect(res.status).toBe(200);
    expectVersionLabels(res.body);
  });

  it("should include version labels on dispute list records", async () => {
    const id =
      "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const res = await request(app).get(`/api/v1/invoices/${id}/disputes`);
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
    res.body.forEach((dispute: Record<string, unknown>) => {
      expectVersionLabels(dispute);
    });
  });

  it("should expose current contract version (1) on all records", async () => {
    const res = await request(app).get("/api/v1/invoices");
    expect(res.status).toBe(200);
    res.body.forEach((invoice: Record<string, unknown>) => {
      expect(invoice.contract_version).toBe(CURRENT_CONTRACT_VERSION);
    });
  });

  it("should expose current event schema version (1) on all records", async () => {
    const res = await request(app).get("/api/v1/bids");
    expect(res.status).toBe(200);
    res.body.forEach((bid: Record<string, unknown>) => {
      expect(bid.event_schema_version).toBe(CURRENT_EVENT_SCHEMA_VERSION);
    });
  });
});

// ─── labelRecord utility ─────────────────────────────────────────────────────

describe("labelRecord utility", () => {
  const RAW = { id: "abc", amount: "1000" };

  it("should apply current versions by default", () => {
    const labeled = labelRecord(RAW);
    expect(labeled.contract_version).toBe(CURRENT_CONTRACT_VERSION);
    expect(labeled.event_schema_version).toBe(CURRENT_EVENT_SCHEMA_VERSION);
  });

  it("should preserve all original fields", () => {
    const labeled = labelRecord(RAW);
    expect(labeled.id).toBe("abc");
    expect(labeled.amount).toBe("1000");
  });

  it("should set indexed_at to a valid ISO 8601 string", () => {
    const before = new Date().toISOString();
    const labeled = labelRecord(RAW);
    const after = new Date().toISOString();
    expect(labeled.indexed_at >= before).toBe(true);
    expect(labeled.indexed_at <= after).toBe(true);
  });

  it("should not mutate the original record", () => {
    const original = { id: "xyz", value: "500" };
    labelRecord(original);
    expect((original as Record<string, unknown>).contract_version).toBeUndefined();
  });

  it("should allow overriding contract_version for historical records", () => {
    const labeled = labelRecord(RAW, 2, 1);
    expect(labeled.contract_version).toBe(2);
    expect(labeled.event_schema_version).toBe(1);
  });

  it("should allow overriding event_schema_version for historical records", () => {
    const labeled = labelRecord(RAW, 1, 3);
    expect(labeled.event_schema_version).toBe(3);
  });

  it("should throw RangeError for contractVersion < 1", () => {
    expect(() => labelRecord(RAW, 0, 1)).toThrow(RangeError);
  });

  it("should throw RangeError for non-integer contractVersion", () => {
    expect(() => labelRecord(RAW, 1.5, 1)).toThrow(RangeError);
  });

  it("should throw RangeError for eventSchemaVersion < 1", () => {
    expect(() => labelRecord(RAW, 1, 0)).toThrow(RangeError);
  });

  it("should throw RangeError for non-integer eventSchemaVersion", () => {
    expect(() => labelRecord(RAW, 1, 2.9)).toThrow(RangeError);
  });
});

// ─── Mixed-version fixture ingestion ─────────────────────────────────────────

describe("Mixed-version fixture ingestion", () => {
  /** Simulates records indexed from two different contract deployments. */
  function buildMixedFixtures(): Array<{ id: string } & VersionedRecord> {
    const base = { id: "record-1", amount: "1000" };
    const v1Record = labelRecord(base, 1, 1);
    const v2Record = labelRecord({ ...base, id: "record-2" }, 2, 1);
    const v2v2Record = labelRecord({ ...base, id: "record-3" }, 2, 2);
    return [v1Record, v2Record, v2v2Record];
  }

  it("should label v1 contract / v1 schema records correctly", () => {
    const fixtures = buildMixedFixtures();
    const v1 = fixtures.find((r) => r.id === "record-1")!;
    expect(v1.contract_version).toBe(1);
    expect(v1.event_schema_version).toBe(1);
  });

  it("should label v2 contract / v1 schema records correctly", () => {
    const fixtures = buildMixedFixtures();
    const v2 = fixtures.find((r) => r.id === "record-2")!;
    expect(v2.contract_version).toBe(2);
    expect(v2.event_schema_version).toBe(1);
  });

  it("should label v2 contract / v2 schema records correctly", () => {
    const fixtures = buildMixedFixtures();
    const v2v2 = fixtures.find((r) => r.id === "record-3")!;
    expect(v2v2.contract_version).toBe(2);
    expect(v2v2.event_schema_version).toBe(2);
  });

  it("should distinguish records from different contract versions in a mixed set", () => {
    const fixtures = buildMixedFixtures();
    const v1only = fixtures.filter((r) => r.contract_version === 1);
    const v2only = fixtures.filter((r) => r.contract_version === 2);
    expect(v1only).toHaveLength(1);
    expect(v2only).toHaveLength(2);
  });

  it("should distinguish records from different event schema versions in a mixed set", () => {
    const fixtures = buildMixedFixtures();
    const schema1 = fixtures.filter((r) => r.event_schema_version === 1);
    const schema2 = fixtures.filter((r) => r.event_schema_version === 2);
    expect(schema1).toHaveLength(2);
    expect(schema2).toHaveLength(1);
  });

  it("should preserve all version labels on each record independently", () => {
    const fixtures = buildMixedFixtures();
    fixtures.forEach((r) => expectVersionLabels(r));
  });

  it("should keep all original fields after labeling in a mixed set", () => {
    const fixtures = buildMixedFixtures();
    fixtures.forEach((r) => {
      expect(typeof r.id).toBe("string");
    });
  });

  it("should allow filtering for records compatible with a specific contract version", () => {
    const fixtures = buildMixedFixtures();
    // A consumer that only understands contract v1 filters out newer records.
    const compatible = fixtures.filter((r) => r.contract_version <= 1);
    expect(compatible).toHaveLength(1);
  });

  it("should allow filtering for records indexed from the current version", () => {
    const fixtures = buildMixedFixtures();
    const current = fixtures.filter(
      (r) =>
        r.contract_version === CURRENT_CONTRACT_VERSION &&
        r.event_schema_version === CURRENT_EVENT_SCHEMA_VERSION
    );
    expect(current).toHaveLength(1);
    expect(current[0].id).toBe("record-1");
  });
});

// ─── resolveEventSchemaVersion ────────────────────────────────────────────────

describe("resolveEventSchemaVersion", () => {
  it("should return 1 for all known event topics", () => {
    Object.keys(EVENT_TOPIC_SCHEMA_VERSIONS).forEach((topic) => {
      expect(resolveEventSchemaVersion(topic)).toBe(
        EVENT_TOPIC_SCHEMA_VERSIONS[topic]
      );
    });
  });

  it("should cover all invoice lifecycle event topics", () => {
    const invoiceTopics = [
      "invoice_created",
      "invoice_status_changed",
      "invoice_funded",
      "invoice_paid",
      "invoice_cancelled",
    ];
    invoiceTopics.forEach((topic) => {
      expect(resolveEventSchemaVersion(topic)).toBeGreaterThanOrEqual(1);
    });
  });

  it("should cover all bid event topics", () => {
    ["bid_placed", "bid_withdrawn", "bid_accepted", "bid_expired"].forEach(
      (topic) => {
        expect(resolveEventSchemaVersion(topic)).toBeGreaterThanOrEqual(1);
      }
    );
  });

  it("should cover all settlement event topics", () => {
    [
      "settlement_initiated",
      "settlement_paid",
      "settlement_defaulted",
    ].forEach((topic) => {
      expect(resolveEventSchemaVersion(topic)).toBeGreaterThanOrEqual(1);
    });
  });

  it("should cover all dispute event topics", () => {
    ["dispute_raised", "dispute_reviewed", "dispute_resolved"].forEach(
      (topic) => {
        expect(resolveEventSchemaVersion(topic)).toBeGreaterThanOrEqual(1);
      }
    );
  });

  it("should fall back to CURRENT_EVENT_SCHEMA_VERSION for unknown topics", () => {
    expect(resolveEventSchemaVersion("unknown_future_event")).toBe(
      CURRENT_EVENT_SCHEMA_VERSION
    );
  });
});

// ─── Security: version spoofing prevention ───────────────────────────────────

describe("Security: version spoofing prevention", () => {
  it("should not alter contract_version based on a query parameter", async () => {
    const res = await request(app).get(
      "/api/v1/invoices?contract_version=999"
    );
    expect(res.status).toBe(200);
    res.body.forEach((invoice: Record<string, unknown>) => {
      // Query param must be ignored; version is always derived from trusted source.
      expect(invoice.contract_version).toBe(CURRENT_CONTRACT_VERSION);
    });
  });

  it("should not alter event_schema_version based on a query parameter", async () => {
    const res = await request(app).get(
      "/api/v1/bids?event_schema_version=42"
    );
    expect(res.status).toBe(200);
    res.body.forEach((bid: Record<string, unknown>) => {
      expect(bid.event_schema_version).toBe(CURRENT_EVENT_SCHEMA_VERSION);
    });
  });

  it("should reject negative contract versions", () => {
    expect(() => labelRecord({}, -1, 1)).toThrow(RangeError);
  });

  it("should reject zero as a contract version", () => {
    expect(() => labelRecord({}, 0, 1)).toThrow(RangeError);
  });

  it("should reject zero as an event schema version", () => {
    expect(() => labelRecord({}, 1, 0)).toThrow(RangeError);
  });

  it("should reject fractional contract versions", () => {
    expect(() => labelRecord({}, 1.1, 1)).toThrow(RangeError);
  });

  it("should reject fractional event schema versions", () => {
    expect(() => labelRecord({}, 1, 1.5)).toThrow(RangeError);
  });
});
