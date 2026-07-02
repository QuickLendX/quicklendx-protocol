import Database from "better-sqlite3";
import { InMemoryDerivedTableStore } from "../services/derivedTableStore";
import { SnapshotScheduler } from "../services/snapshotService";
import type { RedactedRow } from "../types/snapshot";

type LedgerTable = "invoices" | "bids" | "settlements";
type InvoiceRow = { id: string; amount: number };
type BidRow = { id: string; invoice_id: string; bid_amount: number };
type SettlementRow = { id: string; invoice_id: string; amount: number };
type ScalarRow = { value: number };

function createLedgerDb(): Database.Database {
  const db = new Database(":memory:");
  db.exec(`
    CREATE TABLE invoices (
      id TEXT PRIMARY KEY,
      amount INTEGER NOT NULL
    );
    CREATE TABLE bids (
      id TEXT PRIMARY KEY,
      invoice_id TEXT NOT NULL,
      bid_amount INTEGER NOT NULL
    );
    CREATE TABLE settlements (
      id TEXT PRIMARY KEY,
      invoice_id TEXT NOT NULL,
      amount INTEGER NOT NULL
    );
  `);
  return db;
}

function queryCount(db: Database.Database, table: LedgerTable): number {
  const row = db.prepare(`SELECT COUNT(*) AS value FROM ${table}`).get() as ScalarRow;
  return Number(row.value);
}

function querySum(db: Database.Database, table: "bids", column: "bid_amount"): number;
function querySum(db: Database.Database, table: "settlements", column: "amount"): number;
function querySum(db: Database.Database, table: "bids" | "settlements", column: "bid_amount" | "amount"): number {
  const row = db.prepare(`SELECT COALESCE(SUM(${column}), 0) AS value FROM ${table}`).get() as ScalarRow;
  return Number(row.value);
}

function sumRows(rows: RedactedRow[], column: string): number {
  return rows.reduce((total, row) => total + Number(row[column] ?? 0), 0);
}

async function hydrateStoreFromDb(
  db: Database.Database,
  store: InMemoryDerivedTableStore,
): Promise<void> {
  const invoices = db.prepare("SELECT id, amount FROM invoices ORDER BY id").all() as InvoiceRow[];
  const bids = db.prepare("SELECT id, invoice_id, bid_amount FROM bids ORDER BY id").all() as BidRow[];
  const settlements = db.prepare("SELECT id, invoice_id, amount FROM settlements ORDER BY id").all() as SettlementRow[];

  for (const invoice of invoices) {
    await store.upsertInvoice(invoice);
  }
  for (const bid of bids) {
    await store.upsertBid({
      bid_id: bid.id,
      invoice_id: bid.invoice_id,
      bid_amount: bid.bid_amount,
    });
  }
  for (const settlement of settlements) {
    await store.upsertSettlement(settlement);
  }
}

describe("SnapshotScheduler integrity", () => {
  let db: Database.Database;
  let store: InMemoryDerivedTableStore;
  let scheduler: SnapshotScheduler;

  beforeEach(() => {
    db = createLedgerDb();
    store = new InMemoryDerivedTableStore();
    scheduler = SnapshotScheduler.getInstance(store, {
      intervalMs: 60_000,
      maxRetained: 10,
      hmacSecret: "snapshot-integrity-test-secret",
    });
  });

  afterEach(() => {
    scheduler.clearForTests();
    db.close();
  });

  it("captures empty derived tables with valid zero counts and totals", async () => {
    const snapshot = await scheduler.captureNow(0);

    expect(snapshot.tableCounts).toEqual({
      invoices: 0,
      bids: 0,
      settlements: 0,
      disputes: 0,
      notifications: 0,
    });
    expect(sumRows(snapshot.tables.bids, "bid_amount")).toBe(0);
    expect(sumRows(snapshot.tables.settlements, "amount")).toBe(0);
  });

  it("reconciles snapshot counts and totals with derived-table sums", async () => {
    db.prepare("INSERT INTO invoices (id, amount) VALUES (?, ?)").run("inv_1", 1000);
    db.prepare("INSERT INTO invoices (id, amount) VALUES (?, ?)").run("inv_2", 2000);
    db.prepare("INSERT INTO bids (id, invoice_id, bid_amount) VALUES (?, ?, ?)").run("bid_1", "inv_1", 400);
    db.prepare("INSERT INTO bids (id, invoice_id, bid_amount) VALUES (?, ?, ?)").run("bid_2", "inv_1", 250);
    db.prepare("INSERT INTO bids (id, invoice_id, bid_amount) VALUES (?, ?, ?)").run("bid_3", "inv_2", 900);
    db.prepare("INSERT INTO settlements (id, invoice_id, amount) VALUES (?, ?, ?)").run("set_1", "inv_1", 300);
    db.prepare("INSERT INTO settlements (id, invoice_id, amount) VALUES (?, ?, ?)").run("set_2", "inv_2", 700);
    await hydrateStoreFromDb(db, store);

    const snapshot = await scheduler.captureNow(42);

    expect(snapshot.tableCounts.invoices).toBe(queryCount(db, "invoices"));
    expect(snapshot.tableCounts.bids).toBe(queryCount(db, "bids"));
    expect(snapshot.tableCounts.settlements).toBe(queryCount(db, "settlements"));
    expect(sumRows(snapshot.tables.bids, "bid_amount")).toBe(querySum(db, "bids", "bid_amount"));
    expect(sumRows(snapshot.tables.settlements, "amount")).toBe(querySum(db, "settlements", "amount"));
  });

  it("redacts PII while preserving aggregate reconciliation fields", async () => {
    await store.upsertInvoice({ id: "inv_pii", amount: 1000, business: "GRAWBUSINESSWALLET" });
    await store.upsertBid({
      bid_id: "bid_pii",
      invoice_id: "inv_pii",
      investor: "GINVESTORWALLET",
      bid_amount: 1000,
    });

    const first = await scheduler.captureNow(7);
    const second = await scheduler.captureNow(8);

    expect(first.tables.invoices[0].business).not.toBe("GRAWBUSINESSWALLET");
    expect(first.tables.bids[0].investor).not.toBe("GINVESTORWALLET");
    expect(first.tables.bids[0].bid_amount).toBe(1000);
    expect(first.tables.bids[0].investor).toBe(second.tables.bids[0].investor);
  });

  it("keeps snapshots point-in-time when derived tables mutate", async () => {
    db.prepare("INSERT INTO invoices (id, amount) VALUES (?, ?)").run("inv_1", 1000);
    db.prepare("INSERT INTO bids (id, invoice_id, bid_amount) VALUES (?, ?, ?)").run("bid_1", "inv_1", 500);
    await hydrateStoreFromDb(db, store);
    const before = await scheduler.captureNow(100);

    db.prepare("INSERT INTO settlements (id, invoice_id, amount) VALUES (?, ?, ?)").run("set_1", "inv_1", 500);
    await store.upsertSettlement({ id: "set_1", invoice_id: "inv_1", amount: 500 });
    const after = await scheduler.captureNow(101);

    expect(after.snapshotId).not.toBe(before.snapshotId);
    expect(after.stateHash).not.toBe(before.stateHash);
    expect(before.tableCounts.settlements).toBe(0);
    expect(after.tableCounts.settlements).toBe(1);
    expect(sumRows(before.tables.settlements, "amount")).toBe(0);
    expect(sumRows(after.tables.settlements, "amount")).toBe(500);
  });

  it("marks mid-batch snapshots and prunes retained snapshots deterministically", async () => {
    scheduler.clearForTests();
    scheduler = SnapshotScheduler.getInstance(store, {
      intervalMs: 60_000,
      maxRetained: 2,
      hmacSecret: "snapshot-integrity-test-secret",
    });

    scheduler.markBatchStart();
    const midBatch = await scheduler.captureNow(1);
    scheduler.markBatchEnd();
    const second = await scheduler.captureNow(2);
    const third = await scheduler.captureNow(3);

    expect(midBatch.midBatch).toBe(true);
    expect(second.midBatch).toBe(false);
    expect(scheduler.getSnapshot(midBatch.snapshotId)).toBeUndefined();
    expect(scheduler.listSnapshots().map((snapshot) => snapshot.snapshotId)).toEqual([
      second.snapshotId,
      third.snapshotId,
    ]);
  });
});
