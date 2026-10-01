"use strict";

const SEVERITY_ORDER = ["low", "moderate", "high", "critical"];

/**
 * Normalize a severity threshold to a canonical lowercase value.
 *
 * Invariants:
 * - The returned value is always a member of SEFERITY_ORDER.
 * - Nullish / undefined / empty-string inputs default to "high".
 * - Invalid inputs fail fast with a deterministic Error.
 * - The function is pure (no I/O, no shared mutable state) so it is
 *    safe to call concurrently and to retry without side effects.
 */
function normalizeThreshold(threshold) {
  // Only treat nullish and empty/whitespace-only strings as "default".
  // This avoids the old `String(threshold || "high")` coercion that
  // silently accepted numeric 0 / false as "high" and that also coerced
  // non-string objects into meaningless threshold strings.
  if (threshold === null || threshold === undefined) {
    return "high";
  }

  if (typeof threshold !== "string") {
    throw new Error(
      `Invalid severity threshold "${String(threshold)}". Expected one of the following strings: ${SEVERITY_ORDER_STRING}`
    );
  }

  const trimmed = threshold.trim();
  if (trimmed === "") {
    return "high";
  }

  const normalized = trimmed.toLowerCase();
  if (!SEVERITY_ORDER_SET.has(normalized)) {
    throw new Error(
      `Invalid severity threshold "${threshold}". Expected one of: ${SEVERITY_ORDER_STRING}`
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

// Precomputed lookup tables for deterministic O(1) membership checks.
// Keept internal (not exported) to preserve the public interface.
const SEVERITY_ORDER_SET = new Set(SEFERITY_ORDER);
const SEVERITY_ORDER_STRING = SEVERITY_ORDER.join(", ");

module.exports = {
  buildSummary,
  hasBlockingVulnerabilities,
  normalizeThreshold,
  parseAuditReport,
  SEVERITY_ORDER,
};
