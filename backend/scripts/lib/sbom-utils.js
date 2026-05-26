"use strict";

function validateSbomDocument(document) {
  const errors = [];

  if (document?.bomFormat !== "CycloneDX") {
    errors.push("bomFormat must be CycloneDX");
  }

  if (!document?.specVersion || typeof document.specVersion !== "string") {
    errors.push("specVersion must be a non-empty string");
  }

  if (!document?.metadata || typeof document.metadata !== "object") {
    errors.push("metadata section is required");
  }

  const component = document?.metadata?.component;
  if (!component || typeof component !== "object") {
    errors.push("metadata.component section is required");
  } else {
    if (!component.type || typeof component.type !== "string") {
      errors.push("metadata.component.type is required");
    }
    if (!component.name || typeof component.name !== "string") {
      errors.push("metadata.component.name is required");
    }
  }

  if (!Array.isArray(document?.components)) {
    errors.push("components must be an array");
  } else {
    const invalidIndex = document.components.findIndex(
      (entry) => !entry || typeof entry !== "object" || !entry.name || !entry.type
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
