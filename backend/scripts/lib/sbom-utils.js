"use strict";

function isNonEmptyString(value) {
  return typeof value === "string" && value.trim().length > 0;
}

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

/**
 * Validates the structural fields consumed by the SBOM scripts.
 *
 * This function is intentionally pure: every invocation returns a new, ordered
 * error list and never mutates the supplied document. That makes retries and
 * repeated validation of the same parsed artifact deterministic.
 */
function validateSbomDocument(document) {
  const errors = [];

  if (document?.bomFormat !== "CycloneDX") {
    errors.push("bomFormat must be CycloneDX");
  }

  if (!isNonEmptyString(document?.specVersion)) {
    errors.push("specVersion must be a non-empty string");
  }

  if (!isRecord(document?.metadata)) {
    errors.push("metadata section is required");
  }

  const component = document?.metadata?.component;
  if (!isRecord(component)) {
    errors.push("metadata.component section is required");
  } else {
    if (!isNonEmptyString(component.type)) {
      errors.push("metadata.component.type is required");
    }
    if (!isNonEmptyString(component.name)) {
      errors.push("metadata.component.name is required");
    }
  }

  if (!Array.isArray(document?.components)) {
    errors.push("components must be an array");
  } else {
    const invalidIndex = document.components.findIndex(
      (entry) =>
        !isRecord(entry) ||
        !isNonEmptyString(entry.name) ||
        !isNonEmptyString(entry.type),
    );
    if (invalidIndex >= 0) {
      errors.push(`components[${invalidIndex}] must include type and name`);
    }
  }

  return errors;
}

module.exports = {
  validateSbomDocument,
};
