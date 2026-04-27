"use strict";

const SEVERITY_ORDER = ["low", "moderate", "high", "critical"];

function normalizeThreshold(threshold) {
  const normalized = String(threshold || "high").toLowerCase();
  if (!SEVERITY_ORDER.includes(normalized)) {
    throw new Error(
      `Invalid severity threshold \"${threshold}\". Expected one of: ${SEVERITY_ORDER.join(", ")}`
    );
  }

  return normalized;
}

function parseAuditReport(jsonText) {
  let parsed;
  try {
    parsed = JSON.parse(jsonText);
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

  return SEVERITY_ORDER.slice(thresholdIndex).some((level) => {
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
