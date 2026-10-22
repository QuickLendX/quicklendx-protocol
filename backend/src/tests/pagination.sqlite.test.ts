import crypto from "crypto";
import fs from "fs";
import path from "path";
import express from "express";
import request from "supertest";
import { getDatabase, closeDatabase } from "../lib/database";
import { getInvoices } from "../controllers/v1/invoices";
import { getSettlements } from "../controllers/v1/settlements";
import { CC_SHORT } from "../middleware/cache-headers";
import { invoiceStore } from "../services/invoiceStore";
import { settlementOrchestrator } from "../services/settlementOrchestrator";
import type { MigrationContext } from "../lib/migrations/types";
import paginationIndexes from "../migrations/v013_add_list_pagination_indexes";

const TEST_DB_DIR = path.resolve(__dirname, "../../.data");
const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-pagination-${crypto.randomUUID()}.db`);
const ORIGINAL_DATABASE_PATH = process.env.DATABASE_PATH;
const BUSINESS_A = `G${"A".repeat(55)}`;
const BUSINESS_B = `G${"B".repeat(55)}`;

const app = express();
app.get("/invoices", getInvoices);
app.get("/settlements", getSettlements);
app.use((err: Error, req: express.Request, res: express.Response, next: express.NextFunction) => {
  res.status(500).json({ error: err.message });
});

beforeAll(async () => {
  fs.mkdirSync(TEST_DB_DIR, { recursive: true });
  process.env.DATABASE_PATH = TEST_DB_PATH;
  closeDatabase();

  const db = getDatabase();
  db.exec(`
    CREATE TABLE invoices (
      id TEXT PRIMARY KEY, business TEXT NOT NULL, amount TEXT NOT NULL,
      currency TEXT NOT NULL, due_date INTEGER NOT NULL, status TEXT NOT NULL,
      description TEXT NOT NULL, category TEXT NOT NULL, tags TEXT NOT NULL,
      metadata TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
      contract_version INTEGER NOT NULL, event_schema_version INTEGER NOT NULL,
      indexed_at TEXT
    );
    CREATE TABLE settlements (
      id TEXT PRIMARY KEY, invoice_id TEXT NOT NULL, amount TEXT NOT NULL,
      payer TEXT NOT NULL, recipient TEXT NOT NULL, timestamp INTEGER NOT NULL,
      status TEXT NOT NULL, contract_version INTEGER NOT NULL DEFAULT 1,
      event_schema_version INTEGER NOT NULL DEFAULT 1, indexed_at TEXT NOT NULL,
      created_at TEXT NOT NULL, updated_at TEXT NOT NULL, event_id TEXT
    );
  `);

  let context: MigrationContext;
  context = {
    db: {
      exec: async (sql) => {
        db.exec(sql);
        return [];
      },
      get: async () => undefined,
      run: async () => ({ lastInsertRowId: 0, changes: 0 }),
      transaction: (fn) => fn(context.db),
    },
    env: process.env,
    isProduction: false,
    isTest: true,
  };
  await paginationIndexes.up(context);

  const insertInvoice = db.prepare(`
    INSERT INTO invoices (
      id, business, amount, currency, due_date, status, description, category,
      tags, metadata, created_at, updated_at, contract_version,
      event_schema_version, indexed_at
    ) VALUES (?, ?, '100', 'USDC', 1, 'Pending', 'invoice', 'Services',
      '[]', '{}', ?, ?, 1, 1, NULL)
  `);
  const seedInvoices = db.transaction(() => {
    for (let i = 0; i < 1000; i++) {
      insertInvoice.run(
        `invoice-${String(i).padStart(4, "0")}`,
        i % 2 === 0 ? BUSINESS_A : BUSINESS_B,
        Math.floor(i / 5),
        Math.floor(i / 5)
      );
    }
  });
  seedInvoices();

  const insertSettlement = db.prepare(`
    INSERT INTO settlements (
      id, invoice_id, amount, payer, recipient, timestamp, status,
      indexed_at, created_at, updated_at
    ) VALUES (?, ?, '100', 'payer', 'recipient', ?, 'Pending', 'now', 'now', 'now')
  `);
  const seedSettlements = db.transaction(() => {
    for (let i = 0; i < 1000; i++) {
      insertSettlement.run(
        `settlement-${String(i).padStart(4, "0")}`,
        `invoice-${i}`,
        Math.floor(i / 5)
      );
    }
  });
  seedSettlements();
});

afterAll(() => {
  closeDatabase();
  if (ORIGINAL_DATABASE_PATH === undefined) {
    delete process.env.DATABASE_PATH;
  } else {
    process.env.DATABASE_PATH = ORIGINAL_DATABASE_PATH;
  }
  try {
    fs.unlinkSync(TEST_DB_PATH);
  } catch {
    // The database may already have been removed by the test runner.
  }
  for (const suffix of ["-wal", "-shm"]) {
    try {
      fs.unlinkSync(TEST_DB_PATH + suffix);
    } catch {
      // SQLite sidecar files are absent when the connection is closed cleanly.
    }
  }
});

function readEveryPage<T extends { id: string }>(
  listPage: (cursor: ReturnType<typeof parseCursor>) => {
    data: T[];
    next_cursor: string | null;
    has_more: boolean;
  },
): T[] {
  const rows: T[] = [];
  let cursor: ReturnType<typeof parseCursor> = null;
  while (true) {
    const page = listPage(cursor);
    rows.push(...page.data);
    if (!page.has_more) {
      expect(page.next_cursor).toBeNull();
      return rows;
    }
    expect(page.next_cursor).not.toBeNull();
    cursor = parseCursor(page.next_cursor!);
  }
}

function parseCursor(raw: string | null) {
  if (!raw) return null;
  const parsed = JSON.parse(Buffer.from(raw, "base64url").toString("utf8"));
  return parsed as { id: string; sort_val: number; scope?: string };
}

describe("SQLite keyset pagination", () => {
  it("pages through invoices exactly once in created_at DESC, id ASC order", () => {
    const rows = readEveryPage(
      (cursor) => invoiceStore.listPage({ filters: {}, limit: 37, cursor })
    );
    const expected = getDatabase()
      .prepare("SELECT id, created_at FROM invoices ORDER BY created_at DESC, id ASC")
      .all() as { id: string; created_at: number }[];

    expect(rows.map((row) => row.id)).toEqual(expected.map((row) => row.id));
    expect(new Set(rows.map((row) => row.id)).size).toBe(1000);
  });

  it("pages through settlements exactly once in timestamp DESC, id ASC order", () => {
    const rows = readEveryPage(
      (cursor) =>
        settlementOrchestrator.listPage({
          filters: {},
          limit: 37,
          cursor,
        })
    );
    const expected = getDatabase()
      .prepare("SELECT id FROM settlements ORDER BY timestamp DESC, id ASC")
      .all() as { id: string }[];

    expect(rows.map((row) => row.id)).toEqual(expected.map((row) => row.id));
    expect(new Set(rows.map((row) => row.id)).size).toBe(1000);
  });

  it("uses the business composite index without a temporary order-by tree", () => {
    const plan = getDatabase()
      .prepare(`
        EXPLAIN QUERY PLAN
        SELECT * FROM invoices
        WHERE business = ?
          AND (created_at < ? OR (created_at = ? AND id > ?))
        ORDER BY created_at DESC, id ASC
        LIMIT ?
      `)
      .all(BUSINESS_A, 100, 100, "invoice-0100", 38) as { detail: string }[];
    const details = plan.map((entry) => entry.detail).join(" ");

    expect(details).toContain("idx_invoices_business_created_at_id");
    expect(details).not.toContain("USE TEMP B-TREE FOR ORDER BY");
  });

  it("uses the invoice ordering index when there are no filters", () => {
    const plan = getDatabase()
      .prepare(`
        EXPLAIN QUERY PLAN
        SELECT * FROM invoices
        ORDER BY created_at DESC, id ASC
        LIMIT ?
      `)
      .all(38) as { detail: string }[];
    const details = plan.map((entry) => entry.detail).join(" ");

    expect(details).toContain("idx_invoices_created_at_id");
    expect(details).not.toContain("USE TEMP B-TREE FOR ORDER BY");
  });

  it("reads no more than limit + 1 invoice rows for one page", () => {
    const sql = "SELECT * FROM invoices ORDER BY created_at DESC, id ASC LIMIT ?";
    const db = getDatabase();
    const prepare = db.prepare.bind(db);
    const rowsRead = jest.fn();
    const prepareSpy = jest.spyOn(db, "prepare").mockImplementation((query) => {
      const statement = prepare(query);
      if (query === sql) {
        const all = statement.all.bind(statement);
        statement.all = (...params) => {
          const rows = all(...params);
          rowsRead(rows.length);
          return rows;
        };
      }
      return statement;
    });

    try {
      invoiceStore.listPage({ filters: {}, limit: 9, cursor: null });
      expect(prepareSpy).toHaveBeenCalledWith(sql);
      expect(rowsRead).toHaveBeenCalledWith(10);
    } finally {
      prepareSpy.mockRestore();
    }
  });

  it("rejects an invoice cursor replayed under a different business filter", async () => {
    const firstPage = await request(app).get(`/invoices?business=${BUSINESS_A}&limit=1`);
    expect(firstPage.status).toBe(200);
    expect(firstPage.body.next_cursor).toBeTruthy();

    const replay = await request(app)
      .get(`/invoices?business=${BUSINESS_B}&limit=1`)
      .query({ cursor: firstPage.body.next_cursor });

    expect(replay.status).toBe(400);
    expect(replay.body.error.code).toBe("INVALID_PAGINATION");
    expect(replay.body.error.message).toBe("cursor scope mismatch");
  });

  it("returns settlement pages with short cache headers and working cursors", async () => {
    getDatabase().exec("DELETE FROM settlements");
    const insert = getDatabase().prepare(`
      INSERT INTO settlements (
        id, invoice_id, amount, payer, recipient, timestamp, status,
        indexed_at, created_at, updated_at
      ) VALUES (?, 'invoice-page', '100', 'payer', 'recipient', ?, 'Pending', 'now', 'now', 'now')
    `);
    insert.run("settlement-page-a", 3);
    insert.run("settlement-page-b", 2);
    insert.run("settlement-page-c", 1);

    const ids: string[] = [];
    let cursor: string | null = null;
    for (let pageNo = 0; pageNo < 3; pageNo++) {
      const response = await request(app)
        .get("/settlements")
        .query({ limit: 1, ...(cursor ? { cursor } : {}) });
      expect(response.status).toBe(200);
      expect(response.headers["cache-control"]).toBe(CC_SHORT);
      expect(response.body.data).toHaveLength(1);
      ids.push(response.body.data[0].id);
      if (pageNo < 2) {
        expect(response.body.has_more).toBe(true);
        expect(response.body.next_cursor).toBeTruthy();
        cursor = response.body.next_cursor;
      } else {
        expect(response.body.has_more).toBe(false);
        expect(response.body.next_cursor).toBeNull();
      }
    }

    expect(ids).toEqual([
      "settlement-page-a",
      "settlement-page-b",
      "settlement-page-c",
    ]);
  });
});
