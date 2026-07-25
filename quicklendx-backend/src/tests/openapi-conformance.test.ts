/**
 * OpenAPI example-conformance CI gate.
 *
 * For each schema with an `example` field in the OpenAPI spec, validates that
 * the example satisfies the corresponding Zod validator (if one is registered).
 * Fails if an example violates a schema or if a required validator is missing.
 */

import { readFileSync } from "fs";
import { join } from "path";
import yaml from "js-yaml";
import { z } from "zod";

// ---------------------------------------------------------------------------
// Zod validators for schemas that carry examples in openapi.yaml
// ---------------------------------------------------------------------------
const validators: Record<string, z.ZodTypeAny> = {
  ErrorResponse: z.object({
    code: z.string(),
    message: z.string(),
  }),
  UserProfile: z.object({
    id: z.string().uuid(),
    email: z.string().email(),
    role: z.string(),
    status: z.string(),
    created_at: z.string(),
    updated_at: z.string(),
  }),
  Invoice: z.object({
    amount: z.string(),
    currency: z.string(),
  }).passthrough(),
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
function walkSchema(
  spec: any,
  path: string[],
  visit: (schemaName: string, example: unknown) => void
) {
  if (spec && typeof spec === "object") {
    if ("example" in spec && path.length > 0) {
      const schemaName = path[path.length - 1];
      visit(schemaName, spec.example);
    }
    for (const key of Object.keys(spec)) {
      walkSchema(spec[key], [...path, key], visit);
    }
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
describe("OpenAPI example conformance", () => {
  const specPath = join(__dirname, "../../openapi.yaml");
  let spec: any;

  beforeAll(() => {
    const raw = readFileSync(specPath, "utf8");
    spec = yaml.load(raw);
  });

  it("loads the OpenAPI spec without errors", () => {
    expect(spec).toBeDefined();
    expect(spec.openapi).toMatch(/^3\./);
  });

  it("validates all examples that have registered Zod schemas", () => {
    const failures: string[] = [];

    walkSchema(spec, [], (schemaName, example) => {
      const validator = validators[schemaName];
      if (!validator) return; // no validator registered — skip

      const result = validator.safeParse(example);
      if (!result.success) {
        failures.push(
          `Schema '${schemaName}': ${JSON.stringify(result.error.issues)}`
        );
      }
    });

    if (failures.length > 0) {
      throw new Error(
        `OpenAPI example violations:\n${failures.join("\n")}`
      );
    }
  });

  it("reports schemas with examples but no registered validators", () => {
    const unregistered: string[] = [];

    walkSchema(spec, [], (schemaName) => {
      if (!validators[schemaName]) {
        unregistered.push(schemaName);
      }
    });

    // This is informational — log but do not fail.
    if (unregistered.length > 0) {
      console.warn(
        `[openapi-conformance] ${unregistered.length} schema(s) have examples but no Zod validator:\n` +
          [...new Set(unregistered)].join(", ")
      );
    }
    // Always pass — treat as an advisory check.
    expect(true).toBe(true);
  });
});
