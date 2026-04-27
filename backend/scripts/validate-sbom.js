#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { validateSbomDocument } = require("./lib/sbom-utils");

function main() {
  const sbomPath = process.argv[2] || "sbom/backend-sbom.cdx.json";
  const absolutePath = path.resolve(process.cwd(), sbomPath);

  if (!fs.existsSync(absolutePath)) {
    console.error(`SBOM check failed: File not found at ${absolutePath}`);
    process.exit(1);
  }

  const text = fs.readFileSync(absolutePath, "utf8");

  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    console.error(`SBOM check failed: Invalid JSON (${error.message})`);
    process.exit(1);
  }

  const errors = validateSbomDocument(parsed);
  if (errors.length > 0) {
    console.error("SBOM check failed with the following issues:");
    errors.forEach((entry) => console.error(`- ${entry}`));
    process.exit(1);
  }

  console.log(
    `SBOM check passed: ${parsed.components.length} components documented in ${sbomPath}.`
  );
}

main();
