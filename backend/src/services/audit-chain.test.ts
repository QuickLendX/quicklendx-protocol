import * as fs from "fs";
import * as path from "path";
import { auditService } from "../services/auditService";
import { AuditEntry, AUDIT_CHAIN_GENESIS_HASH, redactSensitiveFields } from "../types/audit";

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

    // Tamper with the second entry
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

    // Delete the second line
    lines.splice(1, 1);
    fs.writeFileSync(filePath, lines.join("\n"));

    const result = auditService.verifyChain(today);
    expect(result.ok).toBe(false);
    expect(result.brokenAt).toBe(2); // The 3rd entry (now 2nd) has wrong prevHash
  });

  it("should detect reordered lines", () => {
    auditService.append(createDummyEntry("MAINTENANCE_MODE"));
    auditService.append(createDummyEntry("CONFIG_CHANGE"));
    auditService.append(createDummyEntry("ADMIN_API_KEY_ADD"));

    const today = new Date().toISOString().slice(0, 10);
    const filePath = path.join(TEST_AUDIT_DIR, `audit-${today}.jsonl`);
    let lines = fs.readFileSync(filePath, "utf8").split("\n").filter(Boolean);

    // Swap lines 1 and 2
    const temp = lines[0];
    lines[0] = lines[1];
    lines[1] = temp;
    fs.writeFileSync(filePath, lines.join("\n") + "\n");

    const result = auditService.verifyChain(today);
    expect(result.ok).toBe(false);
    expect(result.brokenAt).toBe(1); // First line's prevHash should be genesis
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
    // Mock date to control file naming
    const today = "2026-04-25";
    const tomorrow = "2026-04-26";

    const entryToday: AuditEntry = { ...createDummyEntry("MAINTENANCE_MODE"), id: "1", timestamp: `${today}T10:00:00.000Z`, prevHash: "a", entryHash: "b" };
    const entryTomorrow: AuditEntry = { ...createDummyEntry("CONFIG_CHANGE"), id: "2", timestamp: `${tomorrow}T10:00:00.000Z`, prevHash: "c", entryHash: "d" };

    // Manually create files to simulate different days
    const filePathToday = path.join(TEST_AUDIT_DIR, `audit-${today}.jsonl`);
    const filePathTomorrow = path.join(TEST_AUDIT_DIR, `audit-${tomorrow}.jsonl`);

    fs.writeFileSync(filePathToday, JSON.stringify(entryToday) + "\n");

    const appendResult = auditService.append({ ...createDummyEntry("WEBHOOK_SECRET_ROTATE"), timestamp: `${tomorrow}T11:00:00.000Z` } as any);

    expect(appendResult.prevHash).toBe(AUDIT_CHAIN_GENESIS_HASH);
  });
});