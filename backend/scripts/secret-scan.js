#!/usr/bin/env node
"use strict";

const path = require("node:path");
const { assertNoSecretsPrinted, runSecretScan } = require("./lib/secret-scan-utils");

/**
 * CLI entry point for the secret scanner.
 *
 * Invariants:
 * - Exit code 0 only when the scan reports ok=true.
 * - On failure, exit code is derived from the result and never 0.
 * - Secret findings must never be printed to stdout/stderr; only sanitized
 *   messages are emitted. The assertion guard is applied before any failure
 *   output is written.
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

  // Guard before emitting any failure output so secrets cannot leak.
  assertNoSecretsPrinted(result.message, result.findings);
  console.error(result.message);
  process.exit(result.exitCode);
}

try {
  main();
} catch (error) {
  // Never echo raw error details that could contain secret material.
  console.error("Secret scan failed: unexpected error during execution.");
  process.exit(1);
}
