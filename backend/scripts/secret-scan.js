#!/usr/bin/env node
"use strict";

const path = require("node:path");
const { assertNoSecretsPrinted, runSecretScan } = require("./lib/secret-scan-utils");

/**
 * Exit codes used by the CI entry point. Keeping them explicit makes the
 * failure boundaries deterministic and documented for the callers.
 */
const EXIT_CODES = Object.freeze({
  OK: 0,
  SECRETS_FOUND: 1,
  SCAN_FAILURE: 2,
});

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

/**
 * Resolve the backend root from an explicit argument or the current working
 * directory. The root is always normalized to an absolute path so the
 * scanner is deterministic regardless of where it is invoked from.
 */
function resolveBackendRoot(argv) {
  const rawRoot = argv ? argv : process.cwd();
  return path.resolve(rawRoot);
}

/**
 * Resolve the optional allowlist path. Returns `undefined` when no allowlist
 * is supplied, so the scanner can apply its own default behavior.
 */
function resolveAllowlistPath(argv, backendRoot) {
  if (!argv) {
    return undefined;
  }
  return path.resolve(backendRoot, argv);
}

/**
 * Run the scanner and map its result to a deterministic process exit code.
 *
 * Invariants:
 * - A failure to run the scanner must never be silently treated as a pass.
 * - Secret findings must never be echoed to the console; only the sanitized
 *   message is printed.
 * - The process exit code is always one of EXIT_CODES.
 */
function main(argv = process.argv.slice(2)) {
  const backendRoot = resolveBackendRoot(argv[0]);
  const allowlistPath = resolveAllowlistPath(argv[1], backendRoot);

  let result;
  try {
    result = runSecretScan({
      backendRoot,
      allowlistPath,
    });
  } catch (error) {
    // The scanner threw instead of returning a result: treat as an
    // infrastructure failure so CI fails closed and the cause is visible.
    const message = error && error.message ? error.message : String(error);
    console.error(`Secret scan failed: ${message}`);
    return EXIT_CODES.SCAN_FAILURE;
  }

  if (!result || typeof result !== "object") {
    console.error("Secret scan failed: scanner returned an invalid result");
    return EXIT_CODES.SCAN_FAILURE;
  }

  if (result.ok) {
    console.log(result.message);
    return EXIT_CODES.OK;
  }

  // Guard before emitting any failure output so secrets cannot leak.
  assertNoSecretsPrinted(result.message, result.findings);
  console.error(result.message);

  const exitCode = Number.isInteger(result.exitCode)
    ? result.exitCode
    : EXIT_CODES.SECRETS_FOUND;
  return exitCode;
}

if (require.main === module) {
  try {
    const exitCode = main();
    process.exitCode = exitCode;
    process.exit(exitCode);
  } catch (error) {
    const message = error && typeof error.message === "string" ? error.message : "unknown error";
    console.error(`Secret scan failed: ${message}`);
    process.exit(1);
  }
}
}

module.exports = { main, EXIT_CODES };
