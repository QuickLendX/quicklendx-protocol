#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");
const {
  buildSummary,
  hasBlockingVulnerabilities,
  normalizeThreshold,
  parseAuditReport,
} = require("./lib/audit-utils");

function main() {
  const reportPath = process.argv[2] || "audit-report.json";
  const threshold = normalizeThreshold(process.argv[3] || process.env.AUDIT_SEVERITY || "high");
  const absolutePath = path.resolve(process.cwd(), reportPath);

  if (!fs.existsSync(absolutePath)) {
    console.error(`Security gate failed: audit report not found at ${absolutePath}`);
    process.exit(1);
  }

  const reportText = fs.readFileSync(absolutePath, "utf8");
  const vulnerabilities = parseAuditReport(reportText);

  console.log(`Dependency audit summary: ${buildSummary(vulnerabilities)}`);
  console.log(`Blocking threshold: ${threshold}`);

  if (hasBlockingVulnerabilities(vulnerabilities, threshold)) {
    console.error(
      `Security gate failed: Found vulnerabilities at or above ${threshold}. ` +
        "Resolve issues or explicitly adjust threshold with AUDIT_SEVERITY."
    );
    process.exit(1);
  }

  console.log("Security gate passed: No blocking vulnerabilities were found.");
}

try {
  main();
} catch (error) {
  console.error(`Security gate failed: ${error.message}`);
  process.exit(1);
}
