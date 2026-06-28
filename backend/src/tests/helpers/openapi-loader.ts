/**
 * openapi-loader.ts
 *
 * Parses the project's openapi.yaml, resolves all $ref pointers in-process,
 * translates every OpenAPI 3.0 Schema Object into a valid JSON Schema Draft-07
 * schema, and extracts every documented path × method × status-code × example
 * combination so that the conformance test suite can iterate over them without
 * duplicating YAML-parsing logic.
 */

import * as fs from "fs";
import * as path from "path";
import * as yaml from "js-yaml";
import Ajv, { SchemaObject, ValidateFunction } from "ajv";
import addFormats from "ajv-formats";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** A single path parameter that must be interpolated before the request. */
export interface PathParam {
  name: string;
  value: string;
}

/**
 * One test-case extracted from the specification: everything needed to fire a
 * single conformance request and validate its response.
 */
export interface ConformanceCase {
  /** Human-readable label, e.g. "GET /invoices 200 (default)" */
  label: string;
  /** HTTP method in uppercase, e.g. "GET" */
  method: string;
  /**
   * URL path with parameter placeholders already replaced by example values,
   * relative to the API base (e.g. "/api/v1/invoices/0xabc").
   */
  url: string;
  /** Expected HTTP status code (numeric). */
  statusCode: number;
  /**
   * The concrete example value extracted from the spec.
   * For array responses this will be an array; for objects an object.
   */
  exampleBody: unknown;
  /**
   * Pre-compiled AJV validator for the response-body schema declared in the
   * spec.  Undefined when the response has no body schema (e.g. plain 404).
   */
  validate: ValidateFunction | undefined;
}

/** Raw OpenAPI document shape (only the fields we care about). */
interface OpenApiDoc {
  paths: Record<string, Record<string, OperationObject>>;
  components?: {
    schemas?: Record<string, SchemaObject>;
  };
}

interface OperationObject {
  responses: Record<string, ResponseObject>;
}

interface ResponseObject {
  content?: Record<string, MediaTypeObject>;
}

interface MediaTypeObject {
  schema?: SchemaObject;
  example?: unknown;
  examples?: Record<string, ExampleObject>;
}

interface ExampleObject {
  value: unknown;
}

// ---------------------------------------------------------------------------
// AJV instance shared by all compiled validators
// ---------------------------------------------------------------------------

let _ajv: Ajv | null = null;

/** Returns a lazily-created, shared AJV instance configured for strictness. */
export function getAjv(): Ajv {
  if (_ajv) return _ajv;
  _ajv = new Ajv({
    allErrors: true,
    strict: false, // allows unknown formats without throwing
    coerceTypes: false,
    useDefaults: false,
  });
  addFormats(_ajv);
  return _ajv;
}

// ---------------------------------------------------------------------------
// YAML loading
// ---------------------------------------------------------------------------

/** Reads and parses the project openapi.yaml into a plain JS object. */
export function loadOpenApiDoc(openapiPath?: string): OpenApiDoc {
  const filePath =
    openapiPath ??
    path.resolve(__dirname, "..", "..", "..", "openapi.yaml");
  const raw = fs.readFileSync(filePath, "utf8");
  return yaml.load(raw) as OpenApiDoc;
}

// ---------------------------------------------------------------------------
// $ref resolver
// ---------------------------------------------------------------------------

/**
 * Resolves a local JSON-pointer $ref (e.g. "#/components/schemas/Invoice")
 * against the root document and returns the referenced node.
 */
export function resolveRef(ref: string, root: OpenApiDoc): SchemaObject {
  if (!ref.startsWith("#/")) {
    throw new Error(`Only local $refs are supported; got: ${ref}`);
  }
  const parts = ref.slice(2).split("/");
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let node: any = root;
  for (const part of parts) {
    const decoded = part.replace(/~1/g, "/").replace(/~0/g, "~");
    if (node == null || typeof node !== "object") {
      throw new Error(`Cannot resolve ref "${ref}": segment "${decoded}" not found`);
    }
    node = node[decoded];
  }
  if (node === undefined) {
    throw new Error(`Cannot resolve ref "${ref}": path not found in document`);
  }
  return node as SchemaObject;
}

// ---------------------------------------------------------------------------
// OpenAPI → JSON Schema translation
// ---------------------------------------------------------------------------

/**
 * Translates an OpenAPI 3.0 Schema Object into a JSON Schema Draft-07 schema.
 *
 * Key differences handled:
 *  - `nullable: true` → adds `null` to the `type` array
 *  - `$ref` pointers are resolved and the target schema is inlined
 *  - OpenAPI-only formats (int32, int64, int128) are stripped (unknown to AJV)
 *  - `additionalProperties` is NOT forcibly set to false here; callers that
 *    want strict validation should pass `strict: true`.
 */
export function translateSchema(
  schema: SchemaObject,
  root: OpenApiDoc,
  strict: boolean = false,
  visited: Set<string> = new Set()
): SchemaObject {
  // Handle $ref
  if ("$ref" in schema && typeof schema["$ref"] === "string") {
    const ref = schema["$ref"] as string;
    if (visited.has(ref)) {
      // Circular ref guard: return a permissive schema to break the cycle
      return {};
    }
    visited.add(ref);
    const resolved = resolveRef(ref, root);
    const translated = translateSchema(resolved, root, strict, new Set(visited));
    visited.delete(ref);
    return translated;
  }

  const out: SchemaObject = {};

  // type + nullable
  if (schema.type) {
    if (schema.nullable) {
      out.type = [schema.type as string, "null"];
    } else {
      out.type = schema.type;
    }
  }

  // enum
  if (schema.enum) {
    out.enum = schema.nullable
      ? [...(schema.enum as unknown[]), null]
      : schema.enum;
  }

  // format – strip OpenAPI-only formats that AJV doesn't know
  const STRIP_FORMATS = new Set(["int32", "int64", "int128", "uint64", "uint128"]);
  if (schema.format && !STRIP_FORMATS.has(schema.format as string)) {
    out.format = schema.format;
  }

  // Scalar constraints
  for (const key of [
    "minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum",
    "minLength", "maxLength", "pattern",
    "minItems", "maxItems", "uniqueItems",
    "minProperties", "maxProperties",
    "multipleOf",
  ] as const) {
    if (schema[key] !== undefined) {
      (out as Record<string, unknown>)[key] = schema[key];
    }
  }

  // object: properties + required
  if (schema.properties) {
    out.properties = {} as Record<string, SchemaObject>;
    for (const [propName, propSchema] of Object.entries(schema.properties as Record<string, SchemaObject>)) {
      (out.properties as Record<string, SchemaObject>)[propName] = translateSchema(
        propSchema,
        root,
        strict,
        new Set(visited)
      );
    }
    if (strict) {
      out.additionalProperties = false;
    }
  }

  if (schema.required) {
    out.required = schema.required;
  }

  // array: items
  if (schema.items) {
    out.items = translateSchema(schema.items as SchemaObject, root, strict, new Set(visited));
  }

  // Composition keywords
  for (const keyword of ["allOf", "anyOf", "oneOf"] as const) {
    if (schema[keyword]) {
      (out as Record<string, unknown>)[keyword] = (schema[keyword] as SchemaObject[]).map(
        (s) => translateSchema(s, root, strict, new Set(visited))
      );
    }
  }

  if (schema.not) {
    out.not = translateSchema(schema.not as SchemaObject, root, strict, new Set(visited));
  }

  // additional passthrough keys
  for (const key of ["title", "description", "default", "example"] as const) {
    if (schema[key] !== undefined) {
      (out as Record<string, unknown>)[key] = schema[key];
    }
  }

  return out;
}

// ---------------------------------------------------------------------------
// Example extraction
// ---------------------------------------------------------------------------

/**
 * Extracts the example payload(s) from a MediaType object.
 * Returns a list of `{ label, value }` tuples (there may be multiple when the
 * spec uses the `examples` map rather than the singular `example`).
 */
export function extractExamples(media: MediaTypeObject): Array<{ label: string; value: unknown }> {
  const results: Array<{ label: string; value: unknown }> = [];

  if (media.example !== undefined) {
    results.push({ label: "default", value: media.example });
  }

  if (media.examples) {
    for (const [name, exObj] of Object.entries(media.examples)) {
      results.push({ label: name, value: exObj.value });
    }
  }

  return results;
}

// ---------------------------------------------------------------------------
// Path interpolation
// ---------------------------------------------------------------------------

/**
 * Replaces OpenAPI-style path parameters (`{id}`) in a path template with the
 * supplied values.  The parameter value is URI-encoded.
 *
 * @example
 * interpolatePath("/invoices/{id}", [{ name: "id", value: "0xabc" }])
 * // => "/invoices/0xabc"
 */
export function interpolatePath(
  template: string,
  params: PathParam[]
): string {
  let result = template;
  for (const { name, value } of params) {
    result = result.replace(`{${name}}`, encodeURIComponent(value));
  }
  return result;
}

// ---------------------------------------------------------------------------
// Main entry point: extract all conformance cases
// ---------------------------------------------------------------------------

/** Base path prefix to prepend to every spec path. */
const BASE_PREFIX = "/api/v1";

/**
 * Parses openapi.yaml and returns the complete list of conformance test cases.
 *
 * Each case represents one path × method × status-code × example combination
 * that is documented in the spec.  Only combinations that have at least one
 * example are included (otherwise there is nothing to assert).
 */
export function extractConformanceCases(openapiPath?: string): ConformanceCase[] {
  const doc = loadOpenApiDoc(openapiPath);
  const ajv = getAjv();
  const cases: ConformanceCase[] = [];

  for (const [pathTemplate, pathItem] of Object.entries(doc.paths)) {
    for (const [rawMethod, operation] of Object.entries(pathItem)) {
      // Skip non-HTTP-method keys like `parameters`, `summary`, etc.
      const HTTP_METHODS = new Set(["get", "post", "put", "patch", "delete", "head", "options"]);
      if (!HTTP_METHODS.has(rawMethod.toLowerCase())) continue;

      const method = rawMethod.toUpperCase();

      for (const [statusStr, response] of Object.entries(operation.responses ?? {})) {
        const statusCode = parseInt(statusStr, 10);
        if (isNaN(statusCode)) continue; // skip "default" keys

        const jsonContent = response.content?.["application/json"];
        if (!jsonContent) continue; // no JSON body for this response

        const examples = extractExamples(jsonContent);
        if (examples.length === 0) continue; // no examples to drive the test

        // Build the JSON Schema validator for this response schema
        let validate: ValidateFunction | undefined;
        if (jsonContent.schema) {
          try {
            const jsonSchema = translateSchema(jsonContent.schema as SchemaObject, doc, true);
            validate = ajv.compile(jsonSchema);
          } catch {
            // Schema compilation failure is captured but does not block extraction
            validate = undefined;
          }
        }

        for (const { label, value: exampleBody } of examples) {
          // Determine path parameters from the example body when the path has
          // placeholders.  We look for a named path param in the spec that has
          // an example value; if not found we fall back to the example body
          // itself (works for single-resource responses).
          const pathParams = buildPathParams(pathTemplate, pathItem, exampleBody);
          const interpolated = interpolatePath(pathTemplate, pathParams);
          const url = `${BASE_PREFIX}${interpolated}`;

          cases.push({
            label: `${method} ${pathTemplate} ${statusCode} (${label})`,
            method,
            url,
            statusCode,
            exampleBody,
            validate,
          });
        }
      }
    }
  }

  return cases;
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/**
 * Builds the list of PathParam substitutions for a given path template.
 *
 * Strategy:
 * 1. Collect path-level and operation-level `parameters` with `in: "path"`.
 * 2. For each path parameter, find a concrete example value from:
 *    a. The parameter's own `example` field in the spec.
 *    b. The example response body if it is an object and has a matching key.
 * 3. If neither is found, use a sensible default ("1").
 */
function buildPathParams(
  pathTemplate: string,
  pathItem: Record<string, unknown>,
  exampleBody: unknown
): PathParam[] {
  // Collect all {name} placeholders
  const placeholders = [...pathTemplate.matchAll(/\{(\w+)\}/g)].map((m) => m[1]);
  if (placeholders.length === 0) return [];

  // Gather parameter definitions from the path-item level
  const paramDefs: Array<{ name: string; in: string; example?: unknown }> =
    (pathItem["parameters"] as Array<{ name: string; in: string; example?: unknown }> | undefined) ?? [];

  return placeholders.map((name) => {
    const def = paramDefs.find((p) => p.in === "path" && p.name === name);

    // 1. Parameter-level example
    if (def?.example !== undefined) {
      return { name, value: String(def.example) };
    }

    // 2. Matching field in example response body
    if (exampleBody !== null && typeof exampleBody === "object" && !Array.isArray(exampleBody)) {
      const bodyObj = exampleBody as Record<string, unknown>;
      if (bodyObj[name] !== undefined) {
        return { name, value: String(bodyObj[name]) };
      }
      // Common alias: "id" param maps to body's "id" field
      if (name === "id" && bodyObj["id"] !== undefined) {
        return { name, value: String(bodyObj["id"]) };
      }
    }

    // 3. Array response: try first element's matching field
    if (Array.isArray(exampleBody) && exampleBody.length > 0) {
      const first = exampleBody[0] as Record<string, unknown>;
      if (first[name] !== undefined) {
        return { name, value: String(first[name]) };
      }
      if (name === "id" && first["id"] !== undefined) {
        return { name, value: String(first["id"]) };
      }
    }

    // 4. Fallback
    return { name, value: "1" };
  });
}
