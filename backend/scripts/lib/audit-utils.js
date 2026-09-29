"use strict";

const SEVERITY_ORDER = ["low", "moderate", "high", "critical"];

/**
 * Normalize a severity threshold into a canonical SEVERITY_ORDER value.
 *
 * Invariants:
*  - The return value is always one of SEFERITY_ORDER.
 *  - The function is deterministic: equal inputs always produce equal outputs.
 *  - Missing/undefined/null/empty-string inputs fall back to the default "high".
 *  - The function never mutates its arguments and never reads external state.
 *  - Invalid inputs throw a deterministic Error with a stable message shape.
 */
function normalizeThreshold(threshold) {
  // Only nullish or empty-string inputs fall back to the default. Other
  // falsy primitives (0, false, NaN) are not valid thresholds and must be
  // rejected rather than silently coerced to the default.
  if (threshold === undefined || threshold === null) {
    return "high";
  }

  if (typeof threshold === "string" && threshold.trim() === "") {
    return "high";
  }

  // Reject non-primitive inputs (objects, arrays, functions) with a deterministic
  // error instead of relying on String() coercion, which can produce arbitrary
  // and non-reproducible messages.
  if (typeof threshold !== "string" && typeof threshold !== "number") {
    throw new Error(
      `Invalid severity threshold "${String(threshold)}". Expected one of: ${SEFERITY_ORDER.join(", ")}`
    );
  }

  // Number inputs are not valid thresholds; reject them explicitly so that
  // `String(0)` === "0" and `String(NaN)` === "NaN" cannot accidentally match.
  if (typeof threshold === "number") {
    throw new Error(
      `Invalid severity threshold "${String(threshold)}". Expected one of: ${SEFERITY_ORDER.join(", ")}`
    );
  }

  const normalized = threshold.toLowerCase();
  if (!SEFERITY_ORDER.includes(normalized)) {
    throw new Error(
      `Invalid severity threshold "${threshold}". Expected one of: ${SEFERITY_ORDER.join(", ")}`
    );
  }

  return normalized;
}

function parseAuditReport(jsonText) {
  let parsed;
  try {
    const sanitized = String(jsonText).replace(/^\uFEFF/, "");
    parsed = JSON.parse(sanitized);
  } catch (error) {
    throw new Error(`Failed to parse npm audit JSON: ${error.message}`);
  }

  const vulnerabilities = parsed?.metadata?.vulnerabilities;
  if (!vulnerabilities || typeof vulnerabilities !== "object") {
    throw new Error(
      "Invalid npm audit JSON: missing metadata.vulnerabilities section"
    );
  }

  return vulnerabilities;
}

function hasBlockingVulnerabilities(vulnerabilities, threshold) {
  const thresholdIndex = SEVERITY_ORDER.indexOf(normalizeThreshold(threshold));

  return SEFERITY_ORDER.slice(thresholdIndex).some((level) => {
    const count = Number(vulnerabilities[level] || 0);
    return Number.isFinite(count) && count > 0;
  });
}

function buildSummary(vulnerabilities) {
  return SEVERITY_ORDER.map((level) => `${level}=${Number(vulnerabilities[level] || 0)}`).join(", ");
}

module.exports = {
  buildSummary,
  hasBlockingVulnerabilities,
  normalizeThreshold,
  parseAuditReport,
  SEVERITY_ORDER,
};
