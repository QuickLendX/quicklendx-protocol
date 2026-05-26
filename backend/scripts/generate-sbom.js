#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const { validateSbomDocument } = require("./lib/sbom-utils");

function main() {
  const outputPath = process.argv[2] || "sbom/backend-sbom.cdx.json";
  const absoluteOutput = path.resolve(process.cwd(), outputPath);
  const outputDir = path.dirname(absoluteOutput);

  fs.mkdirSync(outputDir, { recursive: true });

  execFileSync(
    "npx",
    [
      "--yes",
      "@cyclonedx/cyclonedx-npm",
      "--output-format",
      "JSON",
      "--spec-version",
      "1.5",
      "--output-file",
      absoluteOutput,
    ],
    { stdio: "inherit" }
  );

  const parsed = JSON.parse(fs.readFileSync(absoluteOutput, "utf8"));
  const errors = validateSbomDocument(parsed);
  if (errors.length > 0) {
    throw new Error(`Generated SBOM did not pass validation: ${errors.join("; ")}`);
  }

  console.log(`SBOM generated successfully: ${outputPath}`);
}

try {
  main();
} catch (error) {
  console.error(`SBOM generation failed: ${error.message}`);
  process.exit(1);
}
