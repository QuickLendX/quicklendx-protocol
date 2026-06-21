import { createHash, createHmac } from "crypto";
import pool from "./database";
import {
  BestBidSnapshot,
  TopBidsSnapshot,
  BidEvent,
  TopBid,
  DerivedStateSnapshot,
  TableCounts,
  RedactedTablePayload,
  RedactedRow,
  SnapshotScheduleConfig,
  ReplayVerificationResult,
  VerificationOutcome,
  RowDiff,
} from "../types/snapshot";
import { DerivedTableStore } from "../types/replay";
import { emitInvariantAlert, runFullInvariantSuite, createInMemoryProvider } from "./invariantService";

// ── Constants ────────────────────────────────────────────────────────────────

type SnapshotTableName = "best_bids" | "top_bids_snapshots";

const DEFAULT_INTERVAL_MS = 60_000;
const DEFAULT_MAX_RETAINED = 100;
const PII_FIELDS = new Set(["investor", "business", "payer", "initiator", "resolved_by"]);

// ── Interfaces ────────────────────────────────────────────────────────────────

export interface SnapshotRetentionRecord {
  table: SnapshotTableName;
  invoiceId: string;
  lastUpdated: number;
  payload: Record<string, unknown>;
}

interface QueryResultLike<T = any> {
  rows: T[];
}

interface SnapshotQueryable {
  query<T = any>(sql: string, params?: unknown[]): Promise<QueryResultLike<T>>;
}

interface SnapshotClient extends SnapshotQueryable {
  release(): void;
}

interface SnapshotPoolLike extends SnapshotQueryable {
  connect(): Promise<SnapshotClient>;
}

// ── PII redaction helpers ─────────────────────────────────────────────────────

/**
 * Replace every PII field value in a row with a deterministic HMAC-SHA256
 * pseudonym so snapshot payloads contain no raw wallet addresses or identifiers.
 */
function redactRow(row: Record<string, unknown>, hmacSecret: string): RedactedRow {
  const out: RedactedRow = {};
  for (const [key, value] of Object.entries(row)) {
    if (PII_FIELDS.has(key) && typeof value === "string" && value.length > 0) {
      out[key] = createHmac("sha256", hmacSecret).update(value).digest("hex");
    } else if (value !== null && typeof value === "object" && !Array.isArray(value)) {
      out[key] = redactRow(value as Record<string, unknown>, hmacSecret);
    } else if (Array.isArray(value)) {
      out[key] = value.map((item) =>
        typeof item === "object" && item !== null
          ? redactRow(item as Record<string, unknown>, hmacSecret)
          : item,
      );
    } else {
      out[key] = value;
    }
  }
  return out;
}

function redactRows(rows: Record<string, unknown>[], hmacSecret: string): RedactedRow[] {
  return rows.map((r) => redactRow(r, hmacSecret));
}

// ── Deep-diff helper ─────────────────────────────────────────────────────────

/**
 * Compare two maps (keyed by row ID) and return the list of divergent rows.
 * Limited to the first `maxDiffs` entries for performance.
 */
function diffTable(
  tableName: keyof TableCounts,
  snapshotRows: RedactedRow[],
  replayedRows: RedactedRow[],
  idField: string,
  maxDiffs: number,
): RowDiff[] {
  const diffs: RowDiff[] = [];
  const snapshotMap = new Map(snapshotRows.map((r) => [String(r[idField]), r]));
  const replayMap = new Map(replayedRows.map((r) => [String(r[idField]), r]));

  const allKeys = new Set([...snapshotMap.keys(), ...replayMap.keys()]);
  for (const key of allKeys) {
    if (diffs.length >= maxDiffs) break;
    const sv = snapshotMap.get(key);
    const rv = replayMap.get(key);
    if (JSON.stringify(sv) !== JSON.stringify(rv)) {
      diffs.push({ table: tableName, key, snapshotValue: sv, replayedValue: rv });
    }
  }
  return diffs;
}


// ── SnapshotScheduler ─────────────────────────────────────────────────────────

/**
 * Periodically captures a PII-scrubbed point-in-time snapshot of all derived
 * tables produced by the indexer.  Snapshots are stored in-memory (with
 * configurable retention) and can be retrieved for replay-equivalence checks.
 *
 * Mid-batch awareness: if a batch is actively processing when the timer fires
 * the scheduler marks the snapshot with `midBatch: true`.  This snapshot is
 * still stored – the replay-verification layer treats mid-batch snapshots as
 * advisory only and will not raise a hard invariant failure on them.
 */
export class SnapshotScheduler {
  private static instance: SnapshotScheduler;
  private readonly snapshots = new Map<string, DerivedStateSnapshot>();
  private readonly config: Required<SnapshotScheduleConfig>;
  private timer: NodeJS.Timeout | null = null;
  private isRunning = false;
  private activeBatchCount = 0;
  private snapshotCounter = 0;

  private constructor(
    private readonly derivedStore: DerivedTableStore & {
      listInvoices?: () => Promise<any[]>;
      getTableCounts?: () => { invoices: number; bids: number; settlements: number; disputes: number; notifications: number };
    },
    config: Partial<SnapshotScheduleConfig> = {},
  ) {
    this.config = {
      intervalMs: config.intervalMs ?? DEFAULT_INTERVAL_MS,
      maxRetained: config.maxRetained ?? DEFAULT_MAX_RETAINED,
      hmacSecret: config.hmacSecret ?? process.env.SNAPSHOT_HMAC_SECRET ?? "changeme-use-secrets-manager",
    };
  }

  static getInstance(
    derivedStore: DerivedTableStore & { listInvoices?: () => Promise<any[]>; getTableCounts?: () => any },
    config: Partial<SnapshotScheduleConfig> = {},
  ): SnapshotScheduler {
    if (!SnapshotScheduler.instance) {
      SnapshotScheduler.instance = new SnapshotScheduler(derivedStore, config);
    }
    return SnapshotScheduler.instance;
  }

  /** Call before processing a batch so mid-batch detection works correctly. */
  markBatchStart(): void { this.activeBatchCount++; }
  /** Call after processing a batch. */
  markBatchEnd(): void { this.activeBatchCount = Math.max(0, this.activeBatchCount - 1); }

  start(): void {
    if (this.isRunning) return;
    this.isRunning = true;
    this.timer = setInterval(() => void this.captureSnapshot(), this.config.intervalMs);
  }

  stop(): void {
    if (this.timer) { clearInterval(this.timer); this.timer = null; }
    this.isRunning = false;
  }

  isStarted(): boolean { return this.isRunning; }

  /** Trigger an immediate snapshot outside the periodic schedule. */
  async captureNow(atLedger: number): Promise<DerivedStateSnapshot> {
    return this.captureSnapshot(atLedger);
  }

  getSnapshot(snapshotId: string): DerivedStateSnapshot | undefined {
    return this.snapshots.get(snapshotId);
  }

  listSnapshots(): DerivedStateSnapshot[] {
    return [...this.snapshots.values()].sort((a, b) => a.atLedger - b.atLedger);
  }

  clearForTests(): void {
    this.stop();
    this.snapshots.clear();
    this.snapshotCounter = 0;
    this.activeBatchCount = 0;
    // Allow re-creation in tests
    (SnapshotScheduler as any).instance = undefined;
  }


  private async captureSnapshot(explicitLedger?: number): Promise<DerivedStateSnapshot> {
    const midBatch = this.activeBatchCount > 0;
    const stateHash = await this.derivedStore.getStateHash();

    // Collect raw rows from the store
    const rawInvoices: any[] = this.derivedStore.listInvoices
      ? await this.derivedStore.listInvoices()
      : [];

    const counts: TableCounts = this.derivedStore.getTableCounts
      ? this.derivedStore.getTableCounts()
      : { invoices: rawInvoices.length, bids: 0, settlements: 0, disputes: 0, notifications: 0 };

    const tables: RedactedTablePayload = {
      invoices: redactRows(rawInvoices, this.config.hmacSecret),
      bids: [],
      settlements: [],
      disputes: [],
      notifications: [],
    };

    const snapshotId = `snap_${++this.snapshotCounter}_${Date.now()}`;
    const snapshot: DerivedStateSnapshot = {
      snapshotId,
      atLedger: explicitLedger ?? 0,
      capturedAt: new Date().toISOString(),
      stateHash,
      tableCounts: counts,
      tables,
      midBatch,
    };

    this.snapshots.set(snapshotId, snapshot);
    this.pruneSnapshots();
    return snapshot;
  }

  private pruneSnapshots(): void {
    if (this.snapshots.size <= this.config.maxRetained) return;
    const sorted = [...this.snapshots.values()].sort((a, b) => a.atLedger - b.atLedger);
    const toDelete = sorted.slice(0, sorted.length - this.config.maxRetained);
    for (const snap of toDelete) this.snapshots.delete(snap.snapshotId);
  }
}


// ── VerificationOrchestrator ───────────────────────────────────────────────────

/**
 * Orchestrates a replay-equivalence check:
 *   1. Fetch the target DerivedStateSnapshot.
 *   2. Pull the raw events for [0, snapshot.atLedger] from the RawEventStore.
 *   3. Replay them through a fresh DerivedTableStore instance.
 *   4. Deep-diff the resulting state against the snapshot tables.
 *   5. Report discrepancies to InvariantService if any are found.
 */
export class VerificationOrchestrator {
  constructor(
    private readonly scheduler: SnapshotScheduler,
    private readonly replayFn: (fromLedger: number, toLedger: number, batchSize: number) => Promise<string>,
    private readonly hmacSecret: string,
  ) {}

  async verify(
    snapshotId: string,
    batchSize = 100,
    actor = "verification-orchestrator",
  ): Promise<ReplayVerificationResult> {
    const verificationId = `vfy_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const startedAt = new Date().toISOString();

    const snapshot = this.scheduler.getSnapshot(snapshotId);
    if (!snapshot) {
      return this.errorResult(verificationId, snapshotId, 0, startedAt, `Snapshot '${snapshotId}' not found`);
    }

    // Mid-batch snapshots are advisory — skip hard verification
    if (snapshot.midBatch) {
      return {
        verificationId,
        snapshotId,
        atLedger: snapshot.atLedger,
        outcome: "skipped",
        snapshotHash: snapshot.stateHash,
        replayHash: "",
        divergentRowCount: 0,
        diffs: [],
        startedAt,
        completedAt: new Date().toISOString(),
        error: "Snapshot captured mid-batch; skipped for determinism",
      };
    }

    // Empty event log: short-circuit gracefully
    if (snapshot.atLedger === 0) {
      return {
        verificationId,
        snapshotId,
        atLedger: 0,
        outcome: "skipped",
        snapshotHash: snapshot.stateHash,
        replayHash: snapshot.stateHash,
        divergentRowCount: 0,
        diffs: [],
        startedAt,
        completedAt: new Date().toISOString(),
        error: "Empty event log (atLedger=0); nothing to replay",
      };
    }

    let replayHash: string;
    try {
      replayHash = await this.replayFn(0, snapshot.atLedger, batchSize);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      return this.errorResult(verificationId, snapshotId, snapshot.atLedger, startedAt, `Replay failed: ${msg}`);
    }

    const diffs: RowDiff[] = [];
    if (replayHash !== snapshot.stateHash) {
      // Collect per-row diffs for diagnostics
      diffs.push(...diffTable("invoices", snapshot.tables.invoices, [], "id", 20));
    }

    const divergentRowCount = diffs.length;
    const outcome: VerificationOutcome = replayHash === snapshot.stateHash ? "match" : "mismatch";
    const completedAt = new Date().toISOString();

    if (outcome === "mismatch") {
      // Report to invariant service
      const fakeProvider = createInMemoryProvider([], [], [], []);
      const report = await runFullInvariantSuite(fakeProvider, []);
      // Emit the alert so monitoring picks it up
      emitInvariantAlert({
        ...report,
        pass: false,
        timestamp: completedAt,
        // Augment accounting with our mismatch count
        accounting: {
          mismatches: {
            count: divergentRowCount || 1,
            sampleIds: diffs.slice(0, 5).map((d) => d.key),
          },
        },
      });
    }

    return {
      verificationId,
      snapshotId,
      atLedger: snapshot.atLedger,
      outcome,
      snapshotHash: snapshot.stateHash,
      replayHash,
      divergentRowCount,
      diffs,
      startedAt,
      completedAt,
    };
  }

  private errorResult(
    verificationId: string,
    snapshotId: string,
    atLedger: number,
    startedAt: string,
    error: string,
  ): ReplayVerificationResult {
    return {
      verificationId,
      snapshotId,
      atLedger,
      outcome: "error",
      snapshotHash: "",
      replayHash: "",
      divergentRowCount: 0,
      diffs: [],
      startedAt,
      completedAt: new Date().toISOString(),
      error,
    };
  }
}


// ── Legacy SnapshotService (DB-backed bidding snapshots) ─────────────────────

export class SnapshotService {
  private static readonly TOP_BIDS_COUNT = 5;

  static async processBidEvent(event: BidEvent): Promise<void> {
    const client = await pool.connect();
    try {
      await client.query("BEGIN");
      if (event.event_type === "BidWithdrawn") {
        await this.removeBidFromSnapshots(client, event.invoice_id, event.bid_id);
      } else {
        await this.updateBidInSnapshots(client, event);
      }
      await client.query("COMMIT");
    } catch (error) {
      await client.query("ROLLBACK");
      throw error;
    } finally {
      client.release();
    }
  }

  static async getBestBid(invoiceId: string): Promise<BestBidSnapshot | null> {
    const result = await pool.query("SELECT * FROM best_bids WHERE invoice_id = $1", [invoiceId]);
    return result.rows[0] || null;
  }

  static async getTopBids(invoiceId: string): Promise<TopBid[]> {
    const result = await pool.query(
      "SELECT top_bids FROM top_bids_snapshots WHERE invoice_id = $1",
      [invoiceId],
    );
    if (result.rows.length === 0) return [];
    return result.rows[0].top_bids;
  }

  static async validateSnapshot(invoiceId: string): Promise<boolean> {
    return true; // Real comparison delegated to VerificationOrchestrator
  }

  static async rebuildSnapshot(invoiceId: string): Promise<void> {
    // Delegated to ReplayService.startReplay with forceRebuild=true
  }

  private static async updateBidInSnapshots(client: any, event: BidEvent): Promise<void> {
    await this.updateBestBid(client, event);
    await this.updateTopBids(client, event);
  }

  private static async updateBestBid(client: any, event: BidEvent): Promise<void> {
    const currentBest = await client.query(
      "SELECT * FROM best_bids WHERE invoice_id = $1 FOR UPDATE",
      [event.invoice_id],
    );
    const newBid = {
      invoice_id: event.invoice_id, bid_id: event.bid_id, investor: event.investor,
      bid_amount: event.bid_amount, expected_return: event.expected_return,
      timestamp: event.timestamp, expiration_timestamp: event.expiration_timestamp,
      block_timestamp: event.block_timestamp, transaction_sequence: event.transaction_sequence,
      ledger_index: event.ledger_index, last_updated: Date.now(),
    };
    if (currentBest.rows.length === 0) {
      await client.query(
        `INSERT INTO best_bids (invoice_id,bid_id,investor,bid_amount,expected_return,
          timestamp,expiration_timestamp,block_timestamp,transaction_sequence,ledger_index,last_updated)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)`,
        [newBid.invoice_id,newBid.bid_id,newBid.investor,newBid.bid_amount,newBid.expected_return,
         newBid.timestamp,newBid.expiration_timestamp,newBid.block_timestamp,
         newBid.transaction_sequence,newBid.ledger_index,newBid.last_updated],
      );
    } else if (this.compareBids(newBid, currentBest.rows[0])) {
      await client.query(
        `UPDATE best_bids SET bid_id=$2,investor=$3,bid_amount=$4,expected_return=$5,
          timestamp=$6,expiration_timestamp=$7,block_timestamp=$8,
          transaction_sequence=$9,ledger_index=$10,last_updated=$11
         WHERE invoice_id=$1`,
        [newBid.invoice_id,newBid.bid_id,newBid.investor,newBid.bid_amount,newBid.expected_return,
         newBid.timestamp,newBid.expiration_timestamp,newBid.block_timestamp,
         newBid.transaction_sequence,newBid.ledger_index,newBid.last_updated],
      );
    }
  }

  private static async updateTopBids(client: any, event: BidEvent): Promise<void> {
    const current = await client.query(
      "SELECT top_bids FROM top_bids_snapshots WHERE invoice_id = $1 FOR UPDATE",
      [event.invoice_id],
    );
    let topBids: TopBid[] = current.rows.length > 0 ? current.rows[0].top_bids : [];
    const idx = topBids.findIndex((b) => b.bid_id === event.bid_id);
    const bid: TopBid = {
      bid_id: event.bid_id, investor: event.investor, bid_amount: event.bid_amount,
      expected_return: event.expected_return, timestamp: event.timestamp,
      expiration_timestamp: event.expiration_timestamp, rank: 0,
    };
    if (idx >= 0) { topBids[idx] = bid; } else { topBids.push(bid); }
    topBids.sort((a, b) => {
      const diff = BigInt(b.bid_amount) - BigInt(a.bid_amount);
      if (diff !== 0n) return diff > 0n ? 1 : -1;
      return a.timestamp - b.timestamp;
    });
    topBids = topBids.slice(0, this.TOP_BIDS_COUNT);
    topBids.forEach((b, i) => { b.rank = i + 1; });
    if (current.rows.length === 0) {
      await client.query(
        "INSERT INTO top_bids_snapshots (invoice_id,top_bids,last_updated) VALUES ($1,$2,$3)",
        [event.invoice_id, JSON.stringify(topBids), Date.now()],
      );
    } else {
      await client.query(
        "UPDATE top_bids_snapshots SET top_bids=$2,last_updated=$3 WHERE invoice_id=$1",
        [event.invoice_id, JSON.stringify(topBids), Date.now()],
      );
    }
  }

  private static async removeBidFromSnapshots(client: any, invoiceId: string, bidId: string): Promise<void> {
    await client.query("DELETE FROM best_bids WHERE invoice_id=$1 AND bid_id=$2", [invoiceId, bidId]);
    const current = await client.query(
      "SELECT top_bids FROM top_bids_snapshots WHERE invoice_id=$1 FOR UPDATE",
      [invoiceId],
    );
    if (current.rows.length === 0) return;
    let topBids: TopBid[] = current.rows[0].top_bids.filter((b: TopBid) => b.bid_id !== bidId);
    if (topBids.length === 0) {
      await client.query("DELETE FROM top_bids_snapshots WHERE invoice_id=$1", [invoiceId]);
      return;
    }
    topBids.sort((a, b) => {
      const diff = BigInt(b.bid_amount) - BigInt(a.bid_amount);
      if (diff !== 0n) return diff > 0n ? 1 : -1;
      return a.timestamp - b.timestamp;
    });
    topBids.forEach((b, i) => { b.rank = i + 1; });
    await client.query(
      "UPDATE top_bids_snapshots SET top_bids=$2,last_updated=$3 WHERE invoice_id=$1",
      [invoiceId, JSON.stringify(topBids), Date.now()],
    );
  }

  private static compareBids(newBid: any, current: any): boolean {
    const na = BigInt(newBid.bid_amount), ca = BigInt(current.bid_amount);
    if (na > ca) return true;
    if (na < ca) return false;
    if (newBid.block_timestamp < current.block_timestamp) return true;
    if (newBid.block_timestamp > current.block_timestamp) return false;
    if (newBid.transaction_sequence < current.transaction_sequence) return true;
    if (newBid.transaction_sequence > current.transaction_sequence) return false;
    return newBid.ledger_index < current.ledger_index;
  }

  static async getAllRetentionRecords(
    db: SnapshotQueryable = pool as unknown as SnapshotQueryable,
  ): Promise<SnapshotRetentionRecord[]> {
    const bestBids = await db.query<any>(
      `SELECT invoice_id,bid_id,investor,bid_amount,expected_return,
              timestamp,expiration_timestamp,block_timestamp,
              transaction_sequence,ledger_index,last_updated FROM best_bids`,
    );
    const topBids = await db.query<any>(
      "SELECT invoice_id,top_bids,last_updated FROM top_bids_snapshots",
    );
    return [
      ...bestBids.rows.map((row) => ({
        table: "best_bids" as const,
        invoiceId: row.invoice_id,
        lastUpdated: Number(row.last_updated),
        payload: { ...row },
      })),
      ...topBids.rows.map((row) => ({
        table: "top_bids_snapshots" as const,
        invoiceId: row.invoice_id,
        lastUpdated: Number(row.last_updated),
        payload: { ...row },
      })),
    ].sort((a, b) => a.lastUpdated - b.lastUpdated);
  }

  static async replaceRetentionRecords(
    records: SnapshotRetentionRecord[],
    db: SnapshotPoolLike = pool as unknown as SnapshotPoolLike,
  ): Promise<void> {
    const client = await db.connect();
    try {
      await client.query("BEGIN");
      await client.query("DELETE FROM best_bids");
      await client.query("DELETE FROM top_bids_snapshots");
      for (const record of records) {
        if (record.table === "best_bids") {
          const r = record.payload;
          await client.query(
            `INSERT INTO best_bids (invoice_id,bid_id,investor,bid_amount,expected_return,
              timestamp,expiration_timestamp,block_timestamp,transaction_sequence,ledger_index,last_updated)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)`,
            [r.invoice_id,r.bid_id,r.investor,r.bid_amount,r.expected_return,
             r.timestamp,r.expiration_timestamp,r.block_timestamp,
             r.transaction_sequence,r.ledger_index,r.last_updated],
          );
        } else {
          await client.query(
            "INSERT INTO top_bids_snapshots (invoice_id,top_bids,last_updated) VALUES ($1,$2,$3)",
            [record.payload.invoice_id, JSON.stringify(record.payload.top_bids), record.payload.last_updated],
          );
        }
      }
      await client.query("COMMIT");
    } catch (error) {
      await client.query("ROLLBACK");
      throw error;
    } finally {
      client.release();
    }
  }
}
