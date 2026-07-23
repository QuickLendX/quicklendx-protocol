#!/usr/bin/env node
"use strict";

const path = require("node:path");
const { assertNoSecretsPrinted, runSecretScan } = require("./lib/secret-scan-utils");

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
  console.error(`Secret scan failed: ${error.message}`);
  process.exit(1);
}
