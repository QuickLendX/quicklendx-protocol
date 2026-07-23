import * as fs from "fs";
import * as path from "path";
import { auditService } from "../services/auditService";
import { AuditEntry, AUDIT_CHAIN_GENESIS_HASH, redactSensitiveFields, computeEntryHash } from "../types/audit";

const TEST_AUDIT_DIR = path.join(__dirname, "test_audit_logs");

describe("Audit Log Hash Chaining", () => {
  beforeAll(() => {
    if (fs.existsSync(TEST_AUDIT_DIR)) {
      fs.rmSync(TEST_AUDIT_DIR, { recursive: true, force: true });
    }
    fs.mkdirSync(TEST_AUDIT_DIR, { recursive: true });
    auditService.setAuditDir(TEST_AUDIT_DIR);
  });

  beforeEach(() => {
    auditService.clearAll();
  });

  afterAll(() => {
    fs.rmSync(TEST_AUDIT_DIR, { recursive: true, force: true });
    auditService.setAuditDir(process.env.AUDIT_DIR || "audit_logs");
  });

  const createDummyEntry = (op: any, params: Record<string, unknown> = {}) => ({
    actor: "test-actor",
    operation: op,
    params,
    redactedParams: redactSensitiveFields(params),
    ip: "127.0.0.1",
    userAgent: "jest",
    effect: `Performed ${op}`,
    success: true,
  });

  it("should start a new chain with the genesis hash", () => {
    const entry = auditService.append(createDummyEntry("MAINTENANCE_MODE"));
    expect(entry.prevHash).toBe(AUDIT_CHAIN_GENESIS_HASH);
    expect(entry.entryHash).not.toBe(AUDIT_CHAIN_GENESIS_HASH);
  });

  it("should correctly chain subsequent entries", () => {
    const entry1 = auditService.append(createDummyEntry("MAINTENANCE_MODE"));
    const entry2 = auditService.append(createDummyEntry("CONFIG_CHANGE"));

    expect(entry2.prevHash).toBe(entry1.entryHash);
  });

  it("should pass verification for a valid chain", () => {
    auditService.append(createDummyEntry("MAINTENANCE_MODE"));
    auditService.append(createDummyEntry("CONFIG_CHANGE"));
    auditService.append(createDummyEntry("ADMIN_API_KEY_ADD"));

    const today = new Date().toISOString().slice(0, 10);
    const result = auditService.verifyChain(today);

    expect(result.ok).toBe(true);
    expect(result.brokenAt).toBeUndefined();
  });

  it("should detect a tampered line", () => {
    auditService.append(createDummyEntry("MAINTENANCE_MODE"));
    const entry2 = auditService.append(createDummyEntry("CONFIG_CHANGE"));
    auditService.append(createDummyEntry("ADMIN_API_KEY_ADD"));

    const today = new Date().toISOString().slice(0, 10);
    const filePath = path.join(TEST_AUDIT_DIR, `audit-${today}.jsonl`);
    const lines = fs.readFileSync(filePath, "utf8").split("\n");

    const tamperedEntry = { ...entry2, effect: "Something malicious" };
    lines[1] = JSON.stringify(tamperedEntry);
    fs.writeFileSync(filePath, lines.join("\n"));

    const result = auditService.verifyChain(today);
    expect(result.ok).toBe(false);
    expect(result.brokenAt).toBe(2);
  });

  it("should detect a deleted line", () => {
    auditService.append(createDummyEntry("MAINTENANCE_MODE"));
    auditService.append(createDummyEntry("CONFIG_CHANGE"));
    auditService.append(createDummyEntry("ADMIN_API_KEY_ADD"));

    const today = new Date().toISOString().slice(0, 10);
    const filePath = path.join(TEST_AUDIT_DIR, `audit-${today}.jsonl`);
    let lines = fs.readFileSync(filePath, "utf8").split("\n");

    lines.splice(1, 1);
    fs.writeFileSync(filePath, lines.join("\n"));

    const result = auditService.verifyChain(today);
    expect(result.ok).toBe(false);
    expect(result.brokenAt).toBe(2);
  });

  it("should detect reordered lines", () => {
    auditService.append(createDummyEntry("MAINTENANCE_MODE"));
    auditService.append(createDummyEntry("CONFIG_CHANGE"));
    auditService.append(createDummyEntry("ADMIN_API_KEY_ADD"));

    const today = new Date().toISOString().slice(0, 10);
    const filePath = path.join(TEST_AUDIT_DIR, `audit-${today}.jsonl`);
    let lines = fs.readFileSync(filePath, "utf8").split("\n").filter(Boolean);

    const temp = lines[0];
    lines[0] = lines[1];
    lines[1] = temp;
    fs.writeFileSync(filePath, lines.join("\n") + "\n");

    const result = auditService.verifyChain(today);
    expect(result.ok).toBe(false);
    expect(result.brokenAt).toBe(1);
  });

  it("should handle chain verification for a non-existent file", () => {
    const result = auditService.verifyChain("1999-12-31");
    expect(result.ok).toBe(true);
  });

  it("should handle chain verification for an empty file", () => {
    const date = "2026-01-01";
    const filePath = path.join(TEST_AUDIT_DIR, `audit-${date}.jsonl`);
    fs.writeFileSync(filePath, "");

    const result = auditService.verifyChain(date);
    expect(result.ok).toBe(true);
  });

  it("should start a new chain on a new day", () => {
    const RealDate = global.Date;

    // Day 1 Mocking Setup
    const mockDate1 = new Date("2026-04-25T10:00:00.000Z");
    const MockDateClass1 = function (this: any, ...args: any[]) {
      if (args.length === 0) return mockDate1;
      return new (RealDate as any)(...args);
    };
    MockDateClass1.prototype = RealDate.prototype;
    global.Date = MockDateClass1 as any;
    global.Date.now = () => mockDate1.getTime();

    const entry1 = auditService.append(createDummyEntry("MAINTENANCE_MODE"));
    expect(entry1.prevHash).toBe(AUDIT_CHAIN_GENESIS_HASH);

    // Day 2 Mocking Setup
    const mockDate2 = new Date("2026-04-26T10:00:00.000Z");
    const MockDateClass2 = function (this: any, ...args: any[]) {
      if (args.length === 0) return mockDate2;
      return new (RealDate as any)(...args);
    };
    MockDateClass2.prototype = RealDate.prototype;
    global.Date = MockDateClass2 as any;
    global.Date.now = () => mockDate2.getTime();

    const entry2 = auditService.append(createDummyEntry("CONFIG_CHANGE"));
    expect(entry2.prevHash).toBe(AUDIT_CHAIN_GENESIS_HASH);

    // Restore Global Environment State
    global.Date = RealDate;
  });

  it("should produce a stable hash regardless of property order", () => {
    const entry1: Omit<AuditEntry, "entryHash"> = {
      id: "01H8XGJWBWBAQ0JDBQWEXXXXXX",
      timestamp: "2026-04-25T10:00:00.000Z",
      actor: "test-actor",
      operation: "CONFIG_CHANGE",
      params: { key: "value" },
      redactedParams: { key: "value" },
      ip: "127.0.0.1",
      userAgent: "jest",
      effect: "Changed config",
      success: true,
      prevHash: AUDIT_CHAIN_GENESIS_HASH,
    };

    const entry2: Omit<AuditEntry, "entryHash"> = {
      actor: "test-actor",
      timestamp: "2026-04-25T10:00:00.000Z",
      id: "01H8XGJWBWBAQ0JDBQWEXXXXXX",
      operation: "CONFIG_CHANGE",
      params: { key: "value" },
      redactedParams: { key: "value" },
      ip: "127.0.0.1",
      userAgent: "jest",
      effect: "Changed config",
      success: true,
      prevHash: AUDIT_CHAIN_GENESIS_HASH,
    };

    expect(computeEntryHash(entry1 as AuditEntry)).toBe(computeEntryHash(entry2 as AuditEntry));
  });
});