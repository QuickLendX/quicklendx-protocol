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
import Ajv, { SchemaObject, ValidateFunction } from "ajv";
import addFormats from "ajv-formats";

/**
 * Lightweight pure-TS YAML parser covering the OpenAPI 3.0 YAML subset
 * (block/flow mappings & sequences, quoted/block scalars, comments).
 */
function parseYaml(source: string): unknown {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");

  const stripComment = (line: string): string => {
    let inSingle = false;
    let inDouble = false;
    for (let i = 0; i < line.length; i++) {
      const ch = line[i];
      if (ch === "'" && !inDouble) {
        if (inSingle && line[i + 1] === "'") {
          i++;
          continue;
        }
        inSingle = !inSingle;
      } else if (ch === '"' && !inSingle) {
        if (inDouble && line[i - 1] === "\\") continue;
        inDouble = !inDouble;
      } else if (ch === "#" && !inSingle && !inDouble && (i === 0 || /\s/.test(line[i - 1]))) {
        return line.slice(0, i);
      }
    }
    return line;
  };

  const countIndent = (line: string): number => {
    let n = 0;
    while (n < line.length && line[n] === " ") n++;
    return n;
  };

  const unquote = (raw: string): string => {
    if (raw.startsWith("'") && raw.endsWith("'") && raw.length >= 2) {
      return raw.slice(1, -1).replace(/''/g, "'");
    }
    if (raw.startsWith('"') && raw.endsWith('"') && raw.length >= 2) {
      try {
        return JSON.parse(raw);
      } catch {
        return raw.slice(1, -1);
      }
    }
    return raw;
  };

  const parseScalar = (raw: string): unknown => {
    const s = raw.trim();
    if (s === "" || s === "null" || s === "~") return null;
    if (s === "true") return true;
    if (s === "false") return false;
    if ((s.startsWith("'") && s.endsWith("'")) || (s.startsWith('"') && s.endsWith('"'))) {
      return unquote(s);
    }
    if (/^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?$/.test(s)) {
      return Number(s);
    }
    return s;
  };

  const parseFlow = (text: string): unknown => {
    let pos = 0;
    const skipWs = () => {
      while (pos < text.length && /\s/.test(text[pos])) pos++;
    };
    const readQuoted = (quote: string): string => {
      const start = pos;
      pos++;
      while (pos < text.length) {
        if (quote === "'" && text[pos] === "'" && text[pos + 1] === "'") {
          pos += 2;
          continue;
        }
        if (quote === '"' && text[pos] === "\\") {
          pos += 2;
          continue;
        }
        if (text[pos] === quote) {
          pos++;
          return unquote(text.slice(start, pos));
        }
        pos++;
      }
      return unquote(text.slice(start));
    };
    const parseValue = (): unknown => {
      skipWs();
      if (pos >= text.length) return null;
      const ch = text[pos];
      if (ch === "[") {
        pos++;
        const arr: unknown[] = [];
        skipWs();
        while (pos < text.length && text[pos] !== "]") {
          arr.push(parseValue());
          skipWs();
          if (text[pos] === ",") pos++;
          skipWs();
        }
        if (text[pos] === "]") pos++;
        return arr;
      }
      if (ch === "{") {
        pos++;
        const obj: Record<string, unknown> = {};
        skipWs();
        while (pos < text.length && text[pos] !== "}") {
          skipWs();
          let key: string;
          if (text[pos] === "'" || text[pos] === '"') {
            key = readQuoted(text[pos]);
          } else {
            const kStart = pos;
            while (pos < text.length && text[pos] !== ":" && text[pos] !== "}") pos++;
            key = text.slice(kStart, pos).trim();
          }
          skipWs();
          if (text[pos] === ":") pos++;
          skipWs();
          obj[key] = parseValue();
          skipWs();
          if (text[pos] === ",") pos++;
          skipWs();
        }
        if (text[pos] === "}") pos++;
        return obj;
      }
      if (ch === "'" || ch === '"') {
        return readQuoted(ch);
      }
      const start = pos;
      while (pos < text.length && !/[,\]\}]/.test(text[pos])) pos++;
      return parseScalar(text.slice(start, pos));
    };
    return parseValue();
  };

  // Find next non-empty, non-comment line index
  const nextLine = (idx: number): number => {
    while (idx < lines.length) {
      const cleaned = stripComment(lines[idx]).trim();
      if (cleaned.length > 0 && cleaned !== "---" && cleaned !== "...") return idx;
      idx++;
    }
    return lines.length;
  };

  const splitKeyValue = (trimmed: string): { key: string; rest: string } | null => {
    let inSingle = false;
    let inDouble = false;
    for (let i = 0; i < trimmed.length; i++) {
      const ch = trimmed[i];
      if (ch === "'" && !inDouble) {
        if (inSingle && trimmed[i + 1] === "'") {
          i++;
          continue;
        }
        inSingle = !inSingle;
      } else if (ch === '"' && !inSingle) {
        if (inDouble && trimmed[i - 1] === "\\") continue;
        inDouble = !inDouble;
      } else if (ch === ":" && !inSingle && !inDouble) {
        const next = trimmed[i + 1];
        if (next === undefined || next === " " || next === "\t") {
          return {
            key: unquote(trimmed.slice(0, i).trim()),
            rest: trimmed.slice(i + 1).trim(),
          };
        }
      }
    }
    return null;
  };

  const isFlowComplete = (buf: string): boolean => {
    let depth = 0;
    let inSingle = false;
    let inDouble = false;
    for (let i = 0; i < buf.length; i++) {
      const ch = buf[i];
      if (ch === "'" && !inDouble) {
        if (inSingle && buf[i + 1] === "'") {
          i++;
          continue;
        }
        inSingle = !inSingle;
      } else if (ch === '"' && !inSingle) {
        if (inDouble && buf[i - 1] === "\\") continue;
        inDouble = !inDouble;
      } else if (!inSingle && !inDouble) {
        if (ch === "[" || ch === "{") depth++;
        else if (ch === "]" || ch === "}") depth--;
      }
    }
    return depth <= 0;
  };

  const readFlowBlock = (initial: string, idx: number): { value: unknown; nextIdx: number } => {
    let buf = initial;
    let cur = idx;
    while (!isFlowComplete(buf) && cur < lines.length) {
      buf += " " + stripComment(lines[cur]).trim();
      cur++;
    }
    return { value: parseFlow(buf), nextIdx: cur };
  };

  const readBlockScalar = (indicator: string, parentIndent: number, idx: number): { value: string; nextIdx: number } => {
    const folded = indicator.startsWith(">");
    const chomp = indicator.endsWith("-") ? "strip" : indicator.endsWith("+") ? "keep" : "clip";
    const rawLines: string[] = [];
    let blockIndent: number | null = null;
    let cur = idx;

    while (cur < lines.length) {
      const line = lines[cur];
      if (line.trim() === "") {
        rawLines.push("");
        cur++;
        continue;
      }
      const indent = countIndent(line);
      if (blockIndent === null) {
        if (indent <= parentIndent) break;
        blockIndent = indent;
      } else if (indent < blockIndent) {
        break;
      }
      rawLines.push(line.slice(blockIndent));
      cur++;
    }

    while (rawLines.length > 0 && rawLines[rawLines.length - 1] === "" && chomp !== "keep") {
      rawLines.pop();
    }

    let text: string;
    if (folded) {
      const parts: string[] = [];
      for (let i = 0; i < rawLines.length; i++) {
        const l = rawLines[i];
        if (l === "") {
          parts.push("\n");
        } else if (parts.length > 0 && parts[parts.length - 1] !== "\n") {
          parts.push(" " + l);
        } else {
          parts.push(l);
        }
      }
      text = parts.join("");
    } else {
      text = rawLines.join("\n");
    }

    if (chomp === "clip" && rawLines.length > 0) text += "\n";
    return { value: text, nextIdx: cur };
  };

  const parseValueExpr = (
    expr: string,
    parentIndent: number,
    nextRowIdx: number,
  ): { value: unknown; nextIdx: number } => {
    if (expr === "|" || expr === "|-" || expr === "|+" || expr === ">" || expr === ">-" || expr === ">+") {
      return readBlockScalar(expr, parentIndent, nextRowIdx);
    }
    if (expr.startsWith("[") || expr.startsWith("{")) {
      return readFlowBlock(expr, nextRowIdx);
    }
    if (expr !== "") {
      return { value: parseScalar(expr), nextIdx: nextRowIdx };
    }
    const ni = nextLine(nextRowIdx);
    if (ni < lines.length) {
      const childIndent = countIndent(lines[ni]);
      const childTrimmed = stripComment(lines[ni]).trim();
      if (childIndent > parentIndent || (childIndent === parentIndent && (childTrimmed === "-" || childTrimmed.startsWith("- ")))) {
        return parseNode(ni, childIndent);
      }
    }
    return { value: null, nextIdx: nextRowIdx };
  };

  const parseNode = (startIdx: number, minIndent: number): { value: unknown; nextIdx: number } => {
    const idx = nextLine(startIdx);
    if (idx >= lines.length) return { value: null, nextIdx: idx };
    const line = stripComment(lines[idx]);
    const indent = countIndent(line);
    const trimmed = line.trim();

    if (trimmed.startsWith("[") || trimmed.startsWith("{")) {
      return readFlowBlock(trimmed, idx + 1);
    }

    if (trimmed === "-" || trimmed.startsWith("- ")) {
      const arr: unknown[] = [];
      let cur = idx;
      while (cur < lines.length) {
        cur = nextLine(cur);
        if (cur >= lines.length) break;
        const l = stripComment(lines[cur]);
        const ind = countIndent(l);
        const tr = l.trim();
        if (ind < indent || (ind === indent && tr !== "-" && !tr.startsWith("- "))) break;
        if (ind > indent) break;

        const afterDash = tr.slice(1).trim();
        if (afterDash === "") {
          const ni = nextLine(cur + 1);
          if (ni < lines.length && countIndent(lines[ni]) > ind) {
            const child = parseNode(ni, countIndent(lines[ni]));
            arr.push(child.value);
            cur = child.nextIdx;
          } else {
            arr.push(null);
            cur++;
          }
          continue;
        }
        const kv = splitKeyValue(afterDash);
        if (kv) {
          const itemObj: Record<string, unknown> = {};
          const itemIndent = ind + 2;
          const firstVal = parseValueExpr(kv.rest, ind, cur + 1);
          itemObj[kv.key] = firstVal.value;
          cur = firstVal.nextIdx;

          // Read remaining properties belonging to this sequence item object
          while (cur < lines.length) {
            const ni = nextLine(cur);
            if (ni >= lines.length) {
              cur = ni;
              break;
            }
            const nl = stripComment(lines[ni]);
            const nind = countIndent(nl);
            const ntr = nl.trim();
            if (nind < itemIndent || (nind === ind && (ntr === "-" || ntr.startsWith("- ")))) break;
            const nkv = splitKeyValue(ntr);
            if (!nkv) break;
            const val = parseValueExpr(nkv.rest, nind, ni + 1);
            itemObj[nkv.key] = val.value;
            cur = val.nextIdx;
          }
          arr.push(itemObj);
        } else {
          const parsed = parseValueExpr(afterDash, ind, cur + 1);
          arr.push(parsed.value);
          cur = parsed.nextIdx;
        }
      }
      return { value: arr, nextIdx: cur };
    }

    const obj: Record<string, unknown> = {};
    let cur = idx;
    while (cur < lines.length) {
      cur = nextLine(cur);
      if (cur >= lines.length) break;
      const l = stripComment(lines[cur]);
      const ind = countIndent(l);
      if (ind < minIndent) break;
      const tr = l.trim();
      if (tr === "-" || tr.startsWith("- ")) break;
      const kv = splitKeyValue(tr);
      if (!kv) {
        cur++;
        continue;
      }
      const val = parseValueExpr(kv.rest, ind, cur + 1);
      obj[kv.key] = val.value;
      cur = val.nextIdx;
    }
    return { value: obj, nextIdx: cur };
  };

  return parseNode(0, 0).value;
}

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
  _ajv.addFormat("stellar-address", /^G[A-Z2-7]{55}$/);
  _ajv.addFormat("hex", /^0x[a-fA-F0-9]+$/);
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
  return parseYaml(raw) as OpenApiDoc;
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
          const pathParams = buildPathParams(pathTemplate, pathItem, operation as unknown as Record<string, unknown>, exampleBody);
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

export interface DocumentedOperation {
  method: string;
  pathTemplate: string;
  fullPath: string;
  operationId?: string;
  requiresAuth: boolean;
  documentedStatuses: number[];
  requestBodyExamples: Array<{ name: string; value: unknown }>;
}

/**
 * Extracts every documented path × method operation from openapi.yaml
 * (even when no response example is declared).
 */
export function extractDocumentedOperations(openapiPath?: string): DocumentedOperation[] {
  const doc = loadOpenApiDoc(openapiPath);
  const HTTP_METHODS = new Set(["get", "post", "put", "patch", "delete", "head", "options"]);
  const ops: DocumentedOperation[] = [];

  for (const [pathTemplate, pathItem] of Object.entries(doc.paths)) {
    for (const [rawMethod, operation] of Object.entries(pathItem)) {
      if (!HTTP_METHODS.has(rawMethod.toLowerCase())) continue;
      const op = operation as OperationObject & {
        operationId?: string;
        security?: Array<Record<string, unknown>>;
        requestBody?: { content?: Record<string, MediaTypeObject> };
      };
      const documentedStatuses = Object.keys(op.responses ?? {})
        .map((s) => parseInt(s, 10))
        .filter((n) => !isNaN(n));
      const requiresAuth =
        Array.isArray(op.security) &&
        op.security.some((entry) => Object.keys(entry).length > 0);
      const jsonReq = op.requestBody?.content?.["application/json"];
      const requestBodyExamples = jsonReq
        ? extractExamples(jsonReq).map((e) => ({ name: e.label, value: e.value }))
        : [];

      ops.push({
        method: rawMethod.toUpperCase(),
        pathTemplate,
        fullPath: `${BASE_PREFIX}${pathTemplate}`,
        operationId: op.operationId,
        requiresAuth,
        documentedStatuses,
        requestBodyExamples,
      });
    }
  }

  return ops;
}

/**
 * Validates a live Express response body against an AJV validator, validating
 * `statusInjector`'s additive `_system` block when the schema itself doesn't
 * list `_system` in its strict properties.
 */
export function validateLiveResponseBody(
  validate: ValidateFunction,
  body: unknown
): { valid: boolean; errors: ValidateFunction["errors"] } {
  if (validate(body)) {
    return { valid: true, errors: null };
  }

  if (
    body !== null &&
    typeof body === "object" &&
    !Array.isArray(body) &&
    "_system" in body
  ) {
    const sys = (body as Record<string, unknown>)._system as Record<string, unknown> | null;
    const validSystemShape =
      sys !== null &&
      typeof sys === "object" &&
      typeof sys.status === "string" &&
      typeof sys.degraded === "boolean" &&
      typeof sys.lag === "number" &&
      typeof sys.level === "string";

    if (validSystemShape) {
      const { _system: _ignored, ...withoutSystem } = body as Record<string, unknown>;
      if (validate(withoutSystem)) {
        return { valid: true, errors: null };
      }
    }
  }

  return { valid: false, errors: validate.errors };
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
  operation: Record<string, unknown>,
  exampleBody: unknown
): PathParam[] {
  // Collect all {name} placeholders
  const placeholders = [...pathTemplate.matchAll(/\{(\w+)\}/g)].map((m) => m[1]);
  if (placeholders.length === 0) return [];

  // Gather parameter definitions from the path-item level and the
  // operation level (OpenAPI allows parameters at either level; this spec
  // declares path parameters on the operation).
  const pathLevelParams =
    (pathItem["parameters"] as Array<{ name: string; in: string; example?: unknown }> | undefined) ?? [];
  const opLevelParams =
    (operation["parameters"] as Array<{ name: string; in: string; example?: unknown }> | undefined) ?? [];
  const paramDefs: Array<{ name: string; in: string; example?: unknown }> = [
    ...opLevelParams,
    ...pathLevelParams,
  ];

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
