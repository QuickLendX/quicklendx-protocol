#!/usr/bin/env node
"use strict";

const path = require("node:path");
const { assertNoSecretsPrinted, runSecretScan } = require("./lib/secret-scan-utils");

// Exit codes are part of the public contract for CI consumers.
// Keep them deterministic so retries and partial failures are observable.
const EXIT_OK = 0;
const EXIT_FINDINGS = 1;
const EXIT_USAGE = 2;
const EXIT_INTERNAL = 3;

function main() {
  const backendRoot = process.cwd();
  const allowlistPath = process.argv[2]
    ? path.resolve(backendRoot, process.argv[2])
    : undefined;

  let result;
  try {
    result = runSecretScan({
      backendRoot,
      allowlistPath,
    });
  } catch (error) {
    // Never surface raw scanner errors: they may embed matched secret bytes.
    console.error(`Secret scan failed: ${error && error.name ? error.name : "Error"}`);
    process.exit(EXIT_INTERNAL);
  }

  if (result.ok) {
    console.log(result.message);
    process.exit(EXIT_OK);
  }

  console.error(result.message);
  assertNoSecretsPrinted(result.message, result.findings);
  process.exit(typeof result.exitCode === "number" ? result.exitCode : EXIT_FINDINGS);
}

try {
  main();
} catch (error) {
  console.error(`Secret scan failed: ${error && error.name ? error.name : "Error"}`);
  process.exit(EXIT_INTERNAL);
}
