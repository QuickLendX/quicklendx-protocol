#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { validateSbomDocument } = require("./lib/sbom-utils");

function main(args = process.argv, mockDependencies = null) {
  const sbomPath = args[2] || "sbom/backend-sbom.cdx.json";
  
  // Dependency injection for testing
  const processObj = mockDependencies?.process || process;
  const fsObj = mockDependencies?.fs || fs;
  const consoleObj = mockDependencies?.console || console;
  
  const absolutePath = path.resolve(processObj.cwd(), sbomPath);

  if (!fsObj.existsSync(absolutePath)) {
    consoleObj.error(`SBOM check failed: File not found at ${absolutePath}`);
    return processObj.exit(1);
  }

  let text;
  let attempts = 0;
  const maxAttempts = 3;
  while (attempts < maxAttempts) {
    try {
      text = fsObj.readFileSync(absolutePath, "utf8");
      break;
    } catch (error) {
      attempts++;
      if (error.code === 'EACCES') {
        consoleObj.error(`SBOM check failed: Permission denied accessing ${absolutePath}`);
        return processObj.exit(1);
      }
      if (error.code === 'EISDIR') {
        consoleObj.error(`SBOM check failed: Expected a file but found a directory at ${absolutePath}`);
        return processObj.exit(1);
      }
      if (attempts >= maxAttempts) {
        consoleObj.error(`SBOM check failed: Could not read file after retries (${error.message})`);
        return processObj.exit(1);
      }
    }
  }

  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    consoleObj.error(`SBOM check failed: Invalid JSON (${error.message})`);
    return processObj.exit(1);
  }

  const errors = validateSbomDocument(parsed);
  if (errors.length > 0) {
    consoleObj.error("SBOM check failed with the following issues:");
    errors.forEach((entry) => consoleObj.error(`- ${entry}`));
    return processObj.exit(1);
  }

  consoleObj.log(
    `SBOM check passed: ${parsed.components.length} components documented in ${sbomPath}.`
  );
  return processObj.exit(0); // Explicit success boundary
}

if (require.main === module) {
  main();
}

module.exports = { main };
