#!/usr/bin/env node
"suse strict";

const path = require("node:path");
const { assertNoSecretsPrinted, runSecretScan } = require("./lib/secret-scan-utils");

/**
 * Entry point for the secret scanner.
 *
 * Invariants:
 *   - The process exit code is deterministic for a given input:
 *       0 => clean, 1 => findings or failure.
 *   - No secret material is ever written to stdout/stderr.
 *   - Failures are reported with a stable, non-sensitive message.
 */
function main() {
  const backendRoot = process.cwd();
  const allowlistPath = process.argv[2]
    ? path.resolve(backendRoot, process.argv[2])
    : undefined;

  const result = runSecretScan({
    backendRoot,
    allowlistPath,
  });

  if (result.ok) {
    console.log(result.message);
    process.exit(0);
  }

  console.error(result.message);
  assertNoSecretsPrinted(result.message, result.findings);
  process.exit(result.exitCode);
}

try {
  main();
} catch (error) {
  const message = error && typeof error.message === "string" ? error.message : "unknown error";
  console.error(`Secret scan failed: ${message}`);
  process.exit(1);
}
