"use strict";

const SEVERITY_ORDER = ["low", "moderate", "high", "critical"];

/**
 * Normalize a severity threshold to a canonical lowercase value.
 *
 * Invariants:
 *  - The return value is always one of SEFERITY_ORDER.
  - The function is deterministic for a given input: no global state is read or mutated.
 *  - Nullish / undefined / empty-string inputs fall back to the default "high".
 *  - Any other invalid input throws a TypeError with a stable message.
 *  - Non-string inputs are coerced via String() but never silently accepted
 *    if they do not map to a valid severity level.
 */
function normalizeThreshold(threshold) {
  // Only nullish or empty-string inputs fall back to the default. This
  // keeps the default behavior for missing config while ensuring that
  // explicit invalid values (e.g. "urgent") are rejected instead of silently
  // becoming "high".
  if (threshold === null || threshold === undefined) {
    return "high";
  }

  const asString = typeof threshold === "string" ? threshold : String(threshold);
  const trimmed = asString.trim();

  if (trimmed === "") {
    return "high";
  }

  const normalized = trimmed.toLowerCase();
  if (!SEVERITY_ORDER.includes(normalized)) {
    throw new TypeError(
      `Invalid severity threshold "${threshold}". Expected one of: ${SEVERITY_ORDER.join(", ")}`
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

/**
 * Determine whether any vulnerability at or above the threshold is present.
 *
 * Invariants:
 *  - Returns a boolean and never throws for a valid threshold.
  *  - Threshold normalization is delegated to normalizeThreshold, so invalid
  *    thresholds surface as TypeError with a stable message.
 *  - Non-numeric or negative counts are treated as 0 (no blocking vulnerability)
 *    so that malformed input cannot trigger a false positive.
 */
function hasBlockingVulnerabilities(vulnerabilities, threshold) {
  const thresholdIndex = SEVERITY_ORDER.indexOf(normalizeThreshold(threshold));

  return SEVERITY_ORDER.slice(thresholdIndex).some((level) => {
    const raw = vulnerabilities == null ? 0 : vulnerabilities[level];
    const count = Number(raw ?? 0);
    return Number.isFinite(count) && count > 0;
  });
}

function buildSummary(vulnerabilities) {
  const safe = vulnerabilities == null ? {} : vulnerabilities;
  return SEVERITY_ORDER
    .map((level) => {
      const raw = Number(safe[level] ?? 0);
      const count = Number.isFinite(raw) ? raw : 0;
      return `${level}=${count}`;
    })
    .join(", ");
}

module.exports = {
  buildSummary,
  hasBlockingVulnerabilities,
  normalizeThreshold,
  parseAuditReport,
  SEVERITY_ORDER,
};
