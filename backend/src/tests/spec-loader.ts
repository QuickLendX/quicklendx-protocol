/**
 * spec-loader.ts
 *
 * Lightweight helper that reads backend/openapi.yaml from disk, parses it
 * with js-yaml, and exposes typed accessors used by the contract-test suite.
 *
 * Design goals
 * ─────────────
 * • Parsed once per Jest worker run (module-level cache).
 * • Zero runtime dependencies beyond js-yaml (already a dev-dep).
 * • Validates that every schema $ref can be resolved before the tests run,
 *   so a broken YAML causes a descriptive error rather than a cryptic
 *   "Cannot read property of undefined" deep in the test.
 *
 * Contract-test approach
 * ──────────────────────
 * The contract tests in openapi-contract.test.ts follow this pattern:
 *
 *   1. Start the Express app in a test environment via supertest.
 *   2. Send an HTTP request to each endpoint documented in openapi.yaml.
 *   3. Assert the response status code is one of the codes defined in the spec.
 *   4. Assert the response body matches the schema documented for that status.
 *
 * Schema validation is performed with a hand-rolled, spec-minimal JSON Schema
 * checker (validateAgainstSchema) so the suite has zero production-facing
 * runtime overhead and no binary dependencies.
 *
 * Supported OpenAPI 3.0 schema keywords
 * ──────────────────────────────────────
 *   • type, nullable      — scalar type checks with null support
 *   • properties          — object property sub-schemas
 *   • required            — required property list
 *   • enum                — value must be one of the listed values
 *   • items               — array element sub-schema
 *   • $ref                — local #/components/... reference
 *   • oneOf, anyOf, allOf — combinator keywords
 *   • additionalProperties allowed by default (open-world assumption)
 */

import fs from "fs";
import path from "path";
import yaml from "js-yaml";

// ─── Types ──────────────────────────────────────────────────────────────────

export interface OpenApiSchema {
  type?: string;
  nullable?: boolean;         // OpenAPI 3.0: allows null alongside the declared type
  properties?: Record<string, OpenApiSchema>;
  items?: OpenApiSchema;
  required?: string[];
  enum?: unknown[];
  $ref?: string;
  oneOf?: OpenApiSchema[];
  anyOf?: OpenApiSchema[];
  allOf?: OpenApiSchema[];
  format?: string;
  description?: string;
  example?: unknown;
  minimum?: number;
  maximum?: number;
  additionalProperties?: boolean | OpenApiSchema;
}

export interface OpenApiResponse {
  description: string;
  content?: Record<string, { schema?: OpenApiSchema }>;
}

export interface OpenApiOperation {
  summary?: string;
  description?: string;
  parameters?: Array<{
    name: string;
    in: string;
    required?: boolean;
    schema?: OpenApiSchema;
  }>;
  requestBody?: {
    required?: boolean;
    content?: Record<string, { schema?: OpenApiSchema }>;
  };
  security?: Array<Record<string, string[]>>;
  responses: Record<string, OpenApiResponse>;
  tags?: string[];
}

export interface OpenApiSpec {
  openapi: string;
  info: { title: string; version: string };
  servers?: Array<{ url: string; description?: string }>;
  paths: Record<string, Record<string, OpenApiOperation>>;
  components?: {
    schemas?: Record<string, OpenApiSchema>;
    securitySchemes?: Record<string, unknown>;
    parameters?: Record<string, unknown>;
    responses?: Record<string, OpenApiResponse>;
  };
}

// ─── Module-level cache ──────────────────────────────────────────────────────

let _spec: OpenApiSpec | null = null;

/**
 * Returns the parsed OpenAPI spec, loading it from disk on first call.
 */
export function loadSpec(): OpenApiSpec {
  if (_spec) return _spec;

  const specPath = path.resolve(__dirname, "../../openapi.yaml");
  if (!fs.existsSync(specPath)) {
    throw new Error(
      `openapi.yaml not found at ${specPath}. ` +
        "Ensure you are running tests from the backend/ directory."
    );
  }

  const raw = fs.readFileSync(specPath, "utf8");
  _spec = yaml.load(raw) as OpenApiSpec;

  // Sanity-check: make sure $refs in the spec can be resolved
  validateRefs(_spec);

  return _spec;
}

/**
 * Resolve a $ref string like "#/components/schemas/Invoice" to the
 * referenced schema object within the spec.
 */
export function resolveRef(spec: OpenApiSpec, ref: string): OpenApiSchema {
  if (!ref.startsWith("#/")) {
    throw new Error(`Only local $refs are supported; got: ${ref}`);
  }
  const parts = ref.slice(2).split("/");
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let current: any = spec;
  for (const part of parts) {
    if (current === undefined || current === null) {
      throw new Error(`$ref "${ref}" could not be resolved: missing segment "${part}"`);
    }
    current = current[part];
  }
  if (current === undefined) {
    throw new Error(`$ref "${ref}" resolved to undefined`);
  }
  return current as OpenApiSchema;
}

/**
 * Walk the entire spec and assert that every $ref can be resolved.
 * Called once when the spec is first loaded.
 */
function validateRefs(spec: OpenApiSpec): void {
  const walk = (node: unknown): void => {
    if (Array.isArray(node)) {
      node.forEach(walk);
      return;
    }
    if (node !== null && typeof node === "object") {
      const obj = node as Record<string, unknown>;
      if (typeof obj["$ref"] === "string") {
        resolveRef(spec, obj["$ref"]); // throws if broken
      }
      Object.values(obj).forEach(walk);
    }
  };
  walk(spec.paths);
  walk(spec.components ?? {});
}

// ─── Minimal JSON Schema validator ──────────────────────────────────────────

export interface ValidationResult {
  valid: boolean;
  errors: string[];
}

/**
 * Validate `data` against `schema` using the component schemas from `spec`
 * to resolve $refs.  Returns { valid, errors }.
 *
 * Supported keywords: type, nullable, properties, required, enum, items,
 *                     $ref, oneOf, anyOf, allOf.
 *
 * additionalProperties are allowed by default (open-world assumption matching
 * the spec's intent of not rejecting extra _system metadata fields injected
 * by the statusInjector middleware).
 */
export function validateAgainstSchema(
  data: unknown,
  schema: OpenApiSchema,
  spec: OpenApiSpec,
  path = "$"
): ValidationResult {
  const errors: string[] = [];

  // Resolve $ref first — the referenced schema is the authoritative definition.
  if (schema.$ref) {
    const resolved = resolveRef(spec, schema.$ref);
    return validateAgainstSchema(data, resolved, spec, path);
  }

  // OpenAPI 3.0 nullable: true means the value may also be null.
  // Check this before the type check so null is accepted where expected.
  if (data === null && schema.nullable === true) {
    // null is explicitly allowed — skip all further validation.
    return { valid: true, errors: [] };
  }

  // oneOf: exactly one sub-schema must match
  if (schema.oneOf) {
    const passing = schema.oneOf.filter(
      (s) => validateAgainstSchema(data, s, spec, path).valid
    );
    if (passing.length !== 1) {
      errors.push(
        `${path}: expected exactly one of ${schema.oneOf.length} sub-schemas to match, got ${passing.length}`
      );
    }
    return { valid: errors.length === 0, errors };
  }

  // anyOf: at least one sub-schema must match
  if (schema.anyOf) {
    const passing = schema.anyOf.filter(
      (s) => validateAgainstSchema(data, s, spec, path).valid
    );
    if (passing.length === 0) {
      errors.push(`${path}: did not match any of the anyOf sub-schemas`);
    }
    return { valid: errors.length === 0, errors };
  }

  // allOf: all sub-schemas must match
  if (schema.allOf) {
    for (const sub of schema.allOf) {
      const result = validateAgainstSchema(data, sub, spec, path);
      errors.push(...result.errors);
    }
    return { valid: errors.length === 0, errors };
  }

  // enum check
  if (schema.enum !== undefined) {
    if (!schema.enum.includes(data)) {
      errors.push(
        `${path}: expected one of [${schema.enum.join(", ")}], got ${JSON.stringify(data)}`
      );
    }
    return { valid: errors.length === 0, errors };
  }

  // type check
  if (schema.type) {
    const actualType = Array.isArray(data) ? "array" : typeof data;
    const typeMap: Record<string, string[]> = {
      integer: ["number"],
      number:  ["number"],
      string:  ["string"],
      boolean: ["boolean"],
      object:  ["object"],
      array:   ["array"],
      null:    ["object"], // JSON null has typeof === "object"
    };
    const expectedTypes = typeMap[schema.type] ?? [schema.type];

    if (data === null) {
      // null is only valid when explicitly declared (already handled above for
      // nullable:true; this branch handles type:"null" schemas).
      if (schema.type !== "null") {
        errors.push(`${path}: expected ${schema.type}, got null`);
      }
    } else if (!expectedTypes.includes(actualType)) {
      errors.push(
        `${path}: expected type "${schema.type}", got "${actualType}" (value: ${JSON.stringify(data)})`
      );
    }

    if (schema.type === "integer" && typeof data === "number") {
      if (!Number.isInteger(data)) {
        errors.push(`${path}: expected integer, got float ${data}`);
      }
    }
  }

  // properties + required validation (applies to object-typed values)
  if (schema.properties && typeof data === "object" && data !== null && !Array.isArray(data)) {
    const obj = data as Record<string, unknown>;

    for (const [key, propSchema] of Object.entries(schema.properties)) {
      if (key in obj) {
        const childResult = validateAgainstSchema(
          obj[key],
          propSchema,
          spec,
          `${path}.${key}`
        );
        errors.push(...childResult.errors);
      }
    }
  }

  if (schema.required && typeof data === "object" && data !== null && !Array.isArray(data)) {
    const obj = data as Record<string, unknown>;
    for (const req of schema.required) {
      if (!(req in obj) || obj[req] === undefined) {
        errors.push(`${path}: missing required property "${req}"`);
      }
    }
  }

  // items validation (applies to array-typed values)
  if (schema.items && Array.isArray(data)) {
    data.forEach((item, idx) => {
      const childResult = validateAgainstSchema(
        item,
        schema.items!,
        spec,
        `${path}[${idx}]`
      );
      errors.push(...childResult.errors);
    });
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Convenience: validate `body` against the schema documented for `statusCode`
 * at `method` `pathKey` in the spec.
 * Returns an array of error strings; empty array means the body is valid.
 */
export function validateResponseBody(
  spec: OpenApiSpec,
  method: string,
  pathKey: string,
  statusCode: number,
  body: unknown
): string[] {
  const operation = spec.paths[pathKey]?.[method.toLowerCase()];
  if (!operation) {
    return [`No operation found for ${method.toUpperCase()} ${pathKey}`];
  }

  const statusStr = String(statusCode);
  const responseSpec = operation.responses[statusStr] ?? operation.responses["default"];
  if (!responseSpec) {
    return [
      `No response spec for status ${statusCode} at ${method.toUpperCase()} ${pathKey}`,
    ];
  }

  const jsonContent = responseSpec.content?.["application/json"];
  if (!jsonContent || !jsonContent.schema) {
    // Spec allows an empty body for this status — nothing to validate.
    return [];
  }

  const result = validateAgainstSchema(body, jsonContent.schema, spec);
  return result.errors;
}

/** Reset the module cache (useful between test files if needed). */
export function resetSpecCache(): void {
  _spec = null;
}
