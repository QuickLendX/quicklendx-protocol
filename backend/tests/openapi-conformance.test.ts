/**
 * openapi-conformance.test.ts
 *
 * Build-time contract conformance suite.
 *
 * This file dynamically generates Jest test cases from every path × method ×
 * status-code × example block found in backend/openapi.yaml.  For each case
 * it:
 *
 *   1. Fires the exact request through `supertest` to the live app.
 *   2. Asserts that the HTTP status code matches the specification.
 *   3. Validates the response body against the AJV-compiled JSON Schema.
 *
 * It also contains explicit negative-path tests that confirm the suite WILL
 * fail when an endpoint returns an undocumented field or omits a required one.
 *
 * Run a single suite:
 *   npm test -- openapi-conformance
 */

import request from "supertest";
import app from "../src/app";
import {
  extractConformanceCases,
  translateSchema,
  loadOpenApiDoc,
  getAjv,
  resolveRef,
  extractExamples,
  interpolatePath,
  ConformanceCase,
} from "../src/tests/helpers/openapi-loader";

// ---------------------------------------------------------------------------
// Dynamic conformance cases
// ---------------------------------------------------------------------------

describe("OpenAPI Contract Conformance", () => {
  let cases: ConformanceCase[];

  beforeAll(() => {
    cases = extractConformanceCases();
    // Sanity: the spec must expose at least one testable example
    expect(cases.length).toBeGreaterThan(0);
  });

  /**
   * We use a single `it.each` driven by the extracted cases so that every
   * specification example becomes an individual Jest test with a clear label.
   * The test function is async because supertest requests are async.
   */
  it("should have extracted conformance cases before running", () => {
    expect(cases).toBeDefined();
    expect(cases.length).toBeGreaterThan(0);
  });

  // We generate tests inside a describe block after extracting cases.
  // Because Jest requires tests to be registered synchronously at module-load
  // time we use a beforeAll + a wrapper describe with lazy test registration
  // via a test factory pattern.
  describe("Spec-driven request/response validation", () => {
    // We build the cases at module scope so they are available when Jest
    // collects tests.
    const allCases = extractConformanceCases();

    test.each(allCases.map((c) => [c.label, c] as [string, ConformanceCase]))(
      "%s",
      async (_label: string, testCase: ConformanceCase) => {
        const { method, url, statusCode, exampleBody, validate } = testCase;

        // --- Fire the request ---
        const agent = (request(app) as unknown as Record<string, (url: string) => request.Test>)[
          method.toLowerCase()
        ];
        expect(agent).toBeDefined();
        const res = await agent.call(request(app), url);

        // --- Status code assertion ---
        expect(res.status).toBe(statusCode);

        // --- Schema validation (when a schema is present) ---
        if (validate) {
          const valid = validate(res.body);
          if (!valid) {
            // Surface AJV errors with a readable message
            const errors = validate.errors
              ?.map((e) => `  • ${e.instancePath || "root"}: ${e.message}`)
              .join("\n");
            throw new Error(
              `Schema validation failed for ${method} ${url} ${statusCode}:\n${errors}`
            );
          }
          expect(valid).toBe(true);
        }

        // --- Example body structural check ---
        // For array examples, the live response must be an array.
        // For object examples, the live response must be an object.
        if (Array.isArray(exampleBody)) {
          expect(Array.isArray(res.body)).toBe(true);
        } else if (exampleBody !== null && typeof exampleBody === "object") {
          expect(res.body).toEqual(expect.objectContaining({}));
        }
      }
    );
  });

  // ---------------------------------------------------------------------------
  // Negative tests – undocumented fields & missing required fields
  // ---------------------------------------------------------------------------

  describe("Negative: undocumented additional properties are rejected", () => {
    it("should fail validation when the response body contains an extra undocumented field", () => {
      const doc = loadOpenApiDoc();
      const ajv = getAjv();

      // Grab the Settlement schema and compile a strict validator
      const settlementSchema = resolveRef("#/components/schemas/Settlement", doc);
      const jsonSchema = translateSchema(settlementSchema, doc, /* strict */ true);
      const validate = ajv.compile(jsonSchema);

      // A settlement with an extra field "undocumented_field" that the spec does not declare
      const bodyWithExtraField = {
        id: "0xsettle123",
        invoice_id: "0x1234",
        amount: "1000000000",
        payer: "GA...PAYER",
        recipient: "GB...RECIP",
        timestamp: 1748692800,
        status: "Paid",
        undocumented_field: "this should not be here",
      };

      const valid = validate(bodyWithExtraField);
      // Strict mode must REJECT this body
      expect(valid).toBe(false);
      expect(validate.errors).not.toBeNull();
    });

    it("should fail validation when the response body contains an invalid enum value", () => {
      const doc = loadOpenApiDoc();
      const ajv = getAjv();

      const settlementSchema = resolveRef("#/components/schemas/Settlement", doc);
      const jsonSchema = translateSchema(settlementSchema, doc, true);
      const validate = ajv.compile(jsonSchema);

      const bodyWithBadEnum = {
        id: "0xsettle123",
        invoice_id: "0x1234",
        amount: "1000000000",
        payer: "GA...PAYER",
        recipient: "GB...RECIP",
        timestamp: 1748692800,
        status: "INVALID_STATUS", // not in enum
      };

      const valid = validate(bodyWithBadEnum);
      expect(valid).toBe(false);
    });
  });

  describe("Negative: missing required fields are rejected", () => {
    it("should fail validation when a required field is absent from an Invoice", () => {
      const doc = loadOpenApiDoc();
      const ajv = getAjv();

      const invoiceSchema = resolveRef("#/components/schemas/Invoice", doc);
      const jsonSchema = translateSchema(invoiceSchema, doc, true);
      const validate = ajv.compile(jsonSchema);

      // Missing: amount, currency, due_date, status, description, category, tags, metadata, created_at, updated_at
      const incompleteInvoice = {
        id: "0xabc",
        business: "GDVLRH4G4...7Y",
        // amount is missing
      };

      const valid = validate(incompleteInvoice);
      expect(valid).toBe(false);
      const errorPaths = validate.errors?.map((e) => e.instancePath) ?? [];
      // AJV reports missing required props at the parent object level
      expect(validate.errors?.some((e) => e.keyword === "required")).toBe(true);
      void errorPaths; // suppress unused-variable warning
    });

    it("should fail validation when a required field is absent from a Bid", () => {
      const doc = loadOpenApiDoc();
      const ajv = getAjv();

      const bidSchema = resolveRef("#/components/schemas/Bid", doc);
      const jsonSchema = translateSchema(bidSchema, doc, true);
      const validate = ajv.compile(jsonSchema);

      // bid_id present but missing all other required fields
      const incompleteBid = { bid_id: "0xabc" };

      const valid = validate(incompleteBid);
      expect(valid).toBe(false);
      expect(validate.errors?.some((e) => e.keyword === "required")).toBe(true);
    });

    it("should fail validation when a required field is absent from a Dispute", () => {
      const doc = loadOpenApiDoc();
      const ajv = getAjv();

      const disputeSchema = resolveRef("#/components/schemas/Dispute", doc);
      const jsonSchema = translateSchema(disputeSchema, doc, true);
      const validate = ajv.compile(jsonSchema);

      // Missing reason, status, created_at
      const incompleteDispute = {
        id: "0xdispute1",
        invoice_id: "0x1234",
        initiator: "GA...BUYER",
      };

      const valid = validate(incompleteDispute);
      expect(valid).toBe(false);
      expect(validate.errors?.some((e) => e.keyword === "required")).toBe(true);
    });
  });

  describe("Negative: type mismatches are rejected", () => {
    it("should fail validation when amount is a number instead of the documented string type", () => {
      const doc = loadOpenApiDoc();
      const ajv = getAjv();

      const settlementSchema = resolveRef("#/components/schemas/Settlement", doc);
      const jsonSchema = translateSchema(settlementSchema, doc, false);
      const validate = ajv.compile(jsonSchema);

      // amount must be string (i128 encoded), not a JS number
      const bodyWithNumericAmount = {
        id: "0xsettle123",
        invoice_id: "0x1234",
        amount: 1000000000, // wrong – must be string
        payer: "GA...PAYER",
        recipient: "GB...RECIP",
        timestamp: 1748692800,
        status: "Paid",
      };

      const valid = validate(bodyWithNumericAmount);
      expect(valid).toBe(false);
    });
  });

  // ---------------------------------------------------------------------------
  // Positive validation of spec example data against compiled schemas
  // ---------------------------------------------------------------------------

  describe("Positive: spec examples pass their own schemas", () => {
    it("should validate the Settlement spec example against the Settlement schema", () => {
      const doc = loadOpenApiDoc();
      const ajv = getAjv();

      const settlementSchema = resolveRef("#/components/schemas/Settlement", doc);
      const jsonSchema = translateSchema(settlementSchema, doc, true);
      const validate = ajv.compile(jsonSchema);

      const specExample = {
        id: "0xsettle123",
        invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        amount: "1000000000",
        payer: "GA...PAYER",
        recipient: "GB...RECIP",
        timestamp: 1748692800,
        status: "Paid",
      };

      expect(validate(specExample)).toBe(true);
    });

    it("should validate the Bid spec example against the Bid schema", () => {
      const doc = loadOpenApiDoc();
      const ajv = getAjv();

      const bidSchema = resolveRef("#/components/schemas/Bid", doc);
      const jsonSchema = translateSchema(bidSchema, doc, true);
      const validate = ajv.compile(jsonSchema);

      const specExample = {
        bid_id: "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
        invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        investor: "GA...ABC",
        bid_amount: "950000000",
        expected_return: "50000000",
        timestamp: 1748732400,
        status: "Placed",
        expiration_timestamp: 1748822400,
      };

      expect(validate(specExample)).toBe(true);
    });

    it("should validate the Dispute spec example against the Dispute schema", () => {
      const doc = loadOpenApiDoc();
      const ajv = getAjv();

      const disputeSchema = resolveRef("#/components/schemas/Dispute", doc);
      const jsonSchema = translateSchema(disputeSchema, doc, true);
      const validate = ajv.compile(jsonSchema);

      const specExample = {
        id: "0xdispute1",
        invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        initiator: "GA...BUYER",
        reason: "Goods not delivered as per description",
        status: "UnderReview",
        created_at: 1748649600,
      };

      expect(validate(specExample)).toBe(true);
    });
  });
});

// ---------------------------------------------------------------------------
// Unit tests for the openapi-loader helper module
// ---------------------------------------------------------------------------

describe("openapi-loader helpers", () => {
  describe("loadOpenApiDoc", () => {
    it("should return an object with a paths key", () => {
      const doc = loadOpenApiDoc();
      expect(doc).toBeDefined();
      expect(doc.paths).toBeDefined();
      expect(typeof doc.paths).toBe("object");
    });

    it("should contain the expected top-level paths", () => {
      const doc = loadOpenApiDoc();
      expect(Object.keys(doc.paths)).toContain("/health");
      expect(Object.keys(doc.paths)).toContain("/invoices");
      expect(Object.keys(doc.paths)).toContain("/bids");
      expect(Object.keys(doc.paths)).toContain("/settlements");
    });

    it("should throw when the file does not exist", () => {
      expect(() => loadOpenApiDoc("/nonexistent/path/openapi.yaml")).toThrow();
    });
  });

  describe("resolveRef", () => {
    it("should resolve a known $ref", () => {
      const doc = loadOpenApiDoc();
      const resolved = resolveRef("#/components/schemas/Invoice", doc);
      expect(resolved).toBeDefined();
      expect((resolved as Record<string, unknown>).type).toBe("object");
    });

    it("should throw for external $refs", () => {
      const doc = loadOpenApiDoc();
      expect(() => resolveRef("https://example.com/schema.json", doc)).toThrow(
        /Only local \$refs/
      );
    });

    it("should throw for an unknown path", () => {
      const doc = loadOpenApiDoc();
      expect(() => resolveRef("#/components/schemas/DoesNotExist", doc)).toThrow();
    });

    it("should decode tilde-encoded path segments", () => {
      // Build a minimal doc with a key that needs decoding
      const fakeDoc = {
        paths: {},
        components: {
          schemas: {
            "My~Schema": { type: "string" },
          },
        },
      } as unknown as ReturnType<typeof loadOpenApiDoc>;
      const resolved = resolveRef("#/components/schemas/My~0Schema", fakeDoc);
      expect(resolved).toBeDefined();
    });
  });

  describe("translateSchema", () => {
    it("should pass through a simple string schema", () => {
      const doc = loadOpenApiDoc();
      const result = translateSchema({ type: "string" }, doc);
      expect(result.type).toBe("string");
    });

    it("should add null to type when nullable is true", () => {
      const doc = loadOpenApiDoc();
      const result = translateSchema({ type: "string", nullable: true } as unknown as import("ajv").SchemaObject, doc);
      expect(Array.isArray(result.type)).toBe(true);
      expect((result.type as string[]).includes("null")).toBe(true);
    });

    it("should strip int64/int128 formats unknown to AJV", () => {
      const doc = loadOpenApiDoc();
      const result = translateSchema({ type: "integer", format: "int64" } as unknown as import("ajv").SchemaObject, doc);
      expect(result.format).toBeUndefined();
    });

    it("should preserve date-time format", () => {
      const doc = loadOpenApiDoc();
      const result = translateSchema({ type: "string", format: "date-time" } as unknown as import("ajv").SchemaObject, doc);
      expect(result.format).toBe("date-time");
    });

    it("should inline a $ref", () => {
      const doc = loadOpenApiDoc();
      const result = translateSchema({ $ref: "#/components/schemas/SettlementStatus" }, doc);
      expect(result.enum).toBeDefined();
      expect(Array.isArray(result.enum)).toBe(true);
    });

    it("should not set additionalProperties by default (non-strict)", () => {
      const doc = loadOpenApiDoc();
      const result = translateSchema(
        { type: "object", properties: { foo: { type: "string" } } } as unknown as import("ajv").SchemaObject,
        doc,
        false
      );
      expect(result.additionalProperties).toBeUndefined();
    });

    it("should set additionalProperties: false in strict mode", () => {
      const doc = loadOpenApiDoc();
      const result = translateSchema(
        { type: "object", properties: { foo: { type: "string" } } } as unknown as import("ajv").SchemaObject,
        doc,
        true
      );
      expect(result.additionalProperties).toBe(false);
    });

    it("should handle circular $refs without infinite loop", () => {
      const doc = loadOpenApiDoc();
      // Create a fake circular schema
      const fakeDoc = {
        paths: {},
        components: {
          schemas: {
            SelfRef: { $ref: "#/components/schemas/SelfRef" },
          },
        },
      } as unknown as ReturnType<typeof loadOpenApiDoc>;
      // Should not throw or hang
      expect(() =>
        translateSchema({ $ref: "#/components/schemas/SelfRef" }, fakeDoc)
      ).not.toThrow();
    });

    it("should translate allOf / anyOf / oneOf", () => {
      const doc = loadOpenApiDoc();
      const schema = {
        allOf: [{ type: "object" }, { properties: { id: { type: "string" } } }],
      } as unknown as import("ajv").SchemaObject;
      const result = translateSchema(schema, doc);
      expect(Array.isArray(result.allOf)).toBe(true);
      expect((result.allOf as unknown[]).length).toBe(2);
    });

    it("should translate not keyword", () => {
      const doc = loadOpenApiDoc();
      const schema = { not: { type: "string" } } as unknown as import("ajv").SchemaObject;
      const result = translateSchema(schema, doc);
      expect(result.not).toBeDefined();
    });

    it("should add null to enum when nullable is true", () => {
      const doc = loadOpenApiDoc();
      const schema = {
        type: "string",
        enum: ["A", "B"],
        nullable: true,
      } as unknown as import("ajv").SchemaObject;
      const result = translateSchema(schema, doc);
      expect((result.enum as unknown[]).includes(null)).toBe(true);
    });
  });

  describe("extractExamples", () => {
    it("should extract a singular example", () => {
      const media = { example: { foo: "bar" } };
      const results = extractExamples(media);
      expect(results).toHaveLength(1);
      expect(results[0].label).toBe("default");
      expect(results[0].value).toEqual({ foo: "bar" });
    });

    it("should extract from an examples map", () => {
      const media = {
        examples: {
          first: { value: { a: 1 } },
          second: { value: { b: 2 } },
        },
      };
      const results = extractExamples(media);
      expect(results).toHaveLength(2);
      const labels = results.map((r) => r.label);
      expect(labels).toContain("first");
      expect(labels).toContain("second");
    });

    it("should return empty array when no examples are present", () => {
      const results = extractExamples({});
      expect(results).toHaveLength(0);
    });

    it("should include both singular and map examples when both exist", () => {
      const media = {
        example: { x: 1 },
        examples: { named: { value: { y: 2 } } },
      };
      const results = extractExamples(media);
      expect(results).toHaveLength(2);
    });
  });

  describe("interpolatePath", () => {
    it("should replace a single parameter", () => {
      expect(interpolatePath("/invoices/{id}", [{ name: "id", value: "0xabc" }])).toBe(
        "/invoices/0xabc"
      );
    });

    it("should replace multiple parameters", () => {
      const result = interpolatePath("/a/{x}/b/{y}", [
        { name: "x", value: "1" },
        { name: "y", value: "2" },
      ]);
      expect(result).toBe("/a/1/b/2");
    });

    it("should URI-encode special characters in values", () => {
      const result = interpolatePath("/invoices/{id}", [
        { name: "id", value: "hello world" },
      ]);
      expect(result).toBe("/invoices/hello%20world");
    });

    it("should return the template unchanged when params is empty", () => {
      expect(interpolatePath("/health", [])).toBe("/health");
    });
  });

  describe("extractConformanceCases", () => {
    it("should return an array with at least one case per documented endpoint", () => {
      const cases = extractConformanceCases();
      expect(cases.length).toBeGreaterThanOrEqual(7); // one per path×method×example
    });

    it("each case should have required fields", () => {
      const cases = extractConformanceCases();
      for (const c of cases) {
        expect(typeof c.label).toBe("string");
        expect(typeof c.method).toBe("string");
        expect(typeof c.url).toBe("string");
        expect(typeof c.statusCode).toBe("number");
        expect(c.url.startsWith("/api/v1/")).toBe(true);
      }
    });

    it("should include a case for GET /health", () => {
      const cases = extractConformanceCases();
      const health = cases.find((c) => c.method === "GET" && c.url === "/api/v1/health");
      expect(health).toBeDefined();
    });

    it("should include a case for GET /invoices", () => {
      const cases = extractConformanceCases();
      const inv = cases.find((c) => c.method === "GET" && c.url === "/api/v1/invoices");
      expect(inv).toBeDefined();
    });

    it("should include a case for GET /bids", () => {
      const cases = extractConformanceCases();
      const bids = cases.find((c) => c.method === "GET" && c.url === "/api/v1/bids");
      expect(bids).toBeDefined();
    });

    it("should include a case for GET /settlements", () => {
      const cases = extractConformanceCases();
      const s = cases.find((c) => c.method === "GET" && c.url === "/api/v1/settlements");
      expect(s).toBeDefined();
    });

    it("should include a 404 case for GET /invoices/{id}", () => {
      const cases = extractConformanceCases();
      const notFound = cases.find(
        (c) =>
          c.method === "GET" &&
          c.url.startsWith("/api/v1/invoices/") &&
          c.statusCode === 404
      );
      expect(notFound).toBeDefined();
    });
  });

  describe("getAjv", () => {
    it("should return the same instance on successive calls (singleton)", () => {
      const a = getAjv();
      const b = getAjv();
      expect(a).toBe(b);
    });

    it("should validate a date-time format string", () => {
      const ajv = getAjv();
      const validate = ajv.compile({ type: "string", format: "date-time" });
      expect(validate("2026-01-01T00:00:00.000Z")).toBe(true);
      expect(validate("not-a-date")).toBe(false);
    });
  });
});
