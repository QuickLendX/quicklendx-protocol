import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";

function writeAuditReport(content: Record<string, unknown>): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "quicklendx-audit-"));
  const filePath = path.join(dir, "audit-report.json");
  fs.writeFileSync(filePath, JSON.stringify(content), "utf8");
  return filePath;
}

describe("Dependency scan gate script", () => {
  const repoRoot = path.resolve(__dirname, "..");
  const scriptPath = path.resolve(repoRoot, "scripts/dependency-scan.js");

  it("passes when no vulnerabilities meet the high threshold", () => {
    const report = writeAuditReport({
      metadata: {
        vulnerabilities: {
          info: 0,
          low: 2,
          moderate: 1,
          high: 0,
          critical: 0,
          total: 3,
        },
      },
    });

    expect(() => {
      execFileSync("node", [scriptPath, report, "high"], {
        cwd: repoRoot,
        stdio: "pipe",
      });
    }).not.toThrow();
  });

  it("fails when vulnerabilities meet the configured threshold", () => {
    const report = writeAuditReport({
      metadata: {
        vulnerabilities: {
          info: 0,
          low: 0,
          moderate: 0,
          high: 1,
          critical: 0,
          total: 1,
        },
      },
    });

    expect(() => {
      execFileSync("node", [scriptPath, report, "high"], {
        cwd: repoRoot,
        stdio: "pipe",
      });
    }).toThrow();
  });

  it("fails when threshold value is invalid", () => {
    const report = writeAuditReport({
      metadata: {
        vulnerabilities: {
          info: 0,
          low: 0,
          moderate: 0,
          high: 0,
          critical: 0,
          total: 0,
        },
      },
    });

    expect(() => {
      execFileSync("node", [scriptPath, report, "unknown-threshold"], {
        cwd: repoRoot,
        stdio: "pipe",
      });
    }).toThrow();
  });
});
