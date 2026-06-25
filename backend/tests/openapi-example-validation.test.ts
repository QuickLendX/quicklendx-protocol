/**
 * openapi-example-validation.test.ts
 *
 * Validation test suite for OpenAPI requestBody examples.
 *
 * Purpose
 * ───────
 * When a developer updates a Zod validator in backend/src/validators/*.ts
 * without updating the corresponding example in openapi.yaml, this suite
 * detects the mismatch and fails the build, preventing silent documentation drift.
 *
 * Approach
 * ────────
 * 1. Parse backend/openapi.yaml with js-yaml
 * 2. Walk all paths and operations
 * 3. For each operation with requestBody.examples:
 *    a. Extract the operationId
 *    b. Look up the registered Zod schema in example-routing.ts
 *    c. Validate the example.value against the schema
 *    d. Fail if validation fails or operationId is unmapped
 * 4. Test coverage includes:
 *    - All current examples pass
 *    - Invalid examples fail
 *    - Missing operationId mappings fail
 *
 * Environment
 * ────────────
 * This test requires NO network or database access.
 * It only reads local files (openapi.yaml and validators).
 */

import fs from "fs";
import path from "path";
import yaml from "js-yaml";
import {
  OPERATION_ID_TO_SCHEMA,
  validateRegisteredOperationIds,
  getSchemaForOperationId,
} from "../src/tests/helpers/example-routing";

// ─── Type definitions ──────────────────────────────────────────────────────

interface OpenApiExample {
  value: any;
  description?: string;
}

interface OpenApiRequestBody {
  required?: boolean;
  content?: Record<
    string,
    {
      schema?: any;
      examples?: Record<string, OpenApiExample>;
    }
  >;
}

interface OpenApiOperation {
  operationId?: string;
  summary?: string;
  requestBody?: OpenApiRequestBody;
  responses?: Record<string, any>;
}

interface OpenApiPath {
  [method: string]: OpenApiOperation;
}

interface OpenApiSpec {
  paths: Record<string, OpenApiPath>;
}

// ─── Test Suite ───────────────────────────────────────────────────────────

describe("OpenAPI Example Validation", () => {
  let spec: OpenApiSpec;

  beforeAll(() => {
    // Load the OpenAPI spec from disk
    const specPath = path.join(
      __dirname,
      "../openapi.yaml"
    );
    const specContent = fs.readFileSync(specPath, "utf-8");
    spec = yaml.load(specContent) as OpenApiSpec;

    if (!spec || !spec.paths) {
      throw new Error("Failed to load or parse openapi.yaml");
    }
  });

  describe("Schema Registration", () => {
    it("should find all operationIds with requestBody.examples and validate registration", () => {
      const operationIdsWithExamples = new Set<string>();

      // Walk all paths and operations to find those with examples
      Object.entries(spec.paths).forEach(([pathKey, pathItem]) => {
        Object.entries(pathItem).forEach(([method, operation]) => {
          const op = operation as OpenApiOperation;

          if (op.requestBody?.content) {
            Object.values(op.requestBody.content).forEach((mediaType) => {
              if (mediaType.examples) {
                if (op.operationId) {
                  operationIdsWithExamples.add(op.operationId);
                }
              }
            });
          }
        });
      });

      // If there are examples in the spec, validate that all operationIds are registered
      if (operationIdsWithExamples.size > 0) {
        expect(() => {
          validateRegisteredOperationIds(operationIdsWithExamples);
        }).not.toThrow();
      }
    });

    it("should reject unmapped operationIds gracefully", () => {
      const unmappedId = "nonexistentOperationId";
      expect(() => {
        getSchemaForOperationId(unmappedId);
      }).toThrow(/No schema registered for operationId/);
    });

    it("should fail loud when validateRegisteredOperationIds is given an unmapped operationId", () => {
      const idsWithOneUnmapped = new Set<string>([
        ...Object.keys(OPERATION_ID_TO_SCHEMA),
        "someUnregisteredOperationId",
      ]);

      expect(() => {
        validateRegisteredOperationIds(idsWithOneUnmapped);
      }).toThrow(/Missing operationId.*example-routing\.ts[\s\S]*someUnregisteredOperationId/);
    });

    it("should not throw when validateRegisteredOperationIds is given only registered operationIds", () => {
      const allRegisteredIds = new Set<string>(Object.keys(OPERATION_ID_TO_SCHEMA));

      expect(() => {
        validateRegisteredOperationIds(allRegisteredIds);
      }).not.toThrow();
    });
  });

  describe("Example Validation", () => {
    // Collect validation results to report at the end
    const validationResults: Array<{
      path: string;
      method: string;
      operationId: string;
      mediaType: string;
      exampleName: string;
      valid: boolean;
      error?: string;
    }> = [];

    beforeAll(() => {
      // Walk all paths and validate examples
      Object.entries(spec.paths).forEach(([pathKey, pathItem]) => {
        Object.entries(pathItem).forEach(([method, operation]) => {
          const op = operation as OpenApiOperation;

          if (op.requestBody?.content) {
            Object.entries(op.requestBody.content).forEach(
              ([mediaType, mediaTypeObj]) => {
                if (mediaTypeObj.examples && op.operationId) {
                  Object.entries(mediaTypeObj.examples).forEach(
                    ([exampleName, example]) => {
                      const exampleValue = (
                        example as OpenApiExample
                      ).value;

                      try {
                        // Get the schema for this operationId
                        const operationId = op.operationId!; // Now we know it's not undefined
                        const schema = getSchemaForOperationId(operationId);

                        // Validate the example against the schema
                        const result = schema.safeParse(exampleValue);

                        validationResults.push({
                          path: pathKey,
                          method: method.toUpperCase(),
                          operationId: operationId,
                          mediaType,
                          exampleName,
                          valid: result.success,
                          error: result.success
                            ? undefined
                            : JSON.stringify(result.error.issues, null, 2),
                        });
                      } catch (err) {
                        validationResults.push({
                          path: pathKey,
                          method: method.toUpperCase(),
                          operationId: op.operationId || "unknown",
                          mediaType,
                          exampleName,
                          valid: false,
                          error: err instanceof Error ? err.message : String(err),
                        });
                      }
                    }
                  );
                }
              }
            );
          }
        });
      });
    });

    it("should validate all examples in openapi.yaml against their registered schemas", () => {
      const failedValidations = validationResults.filter((r) => !r.valid);

      if (failedValidations.length > 0) {
        const errorMessages = failedValidations
          .map(
            (r) =>
              `\n${r.method} ${r.path} (operationId: ${r.operationId})\n` +
              `  Example: ${r.exampleName}\n` +
              `  Media Type: ${r.mediaType}\n` +
              `  Error: ${r.error}`
          )
          .join("\n");

        throw new Error(
          `${failedValidations.length} example(s) failed validation:${errorMessages}`
        );
      }

      // Pass if all examples validated successfully
      expect(validationResults.length).toBeGreaterThanOrEqual(0);
    });

    it("should report validation results for debugging", () => {
      if (validationResults.length === 0) {
        console.log(
          "ℹ️  No examples found in openapi.yaml yet. " +
            "Add examples to requestBody sections to enable validation."
        );
      } else {
        console.log("\n✓ Example Validation Results:");
        validationResults.forEach((r) => {
          const status = r.valid ? "✓" : "✗";
          console.log(
            `  ${status} ${r.method} ${r.path} (${r.exampleName}): ${r.operationId}`
          );
        });

        const passCount = validationResults.filter((r) => r.valid).length;
        const failCount = validationResults.filter((r) => !r.valid).length;
        console.log(
          `\n  Summary: ${passCount} passed, ${failCount} failed`
        );
      }
    });
  });

  describe("Edge Cases", () => {
    it("should handle operations without operationId gracefully", () => {
      // This is a documentation check - operations without operationId
      // should not break the validator, they just won't be validated
      expect(() => {
        Object.entries(spec.paths).forEach(([pathKey, pathItem]) => {
          Object.entries(pathItem).forEach(([method, operation]) => {
            const op = operation as OpenApiOperation;
            // Should not throw even if operationId is missing
            if (op.operationId) {
              try {
                getSchemaForOperationId(op.operationId);
              } catch (e) {
                if (e instanceof Error && e.message.includes("No schema registered")) {
                  return;
                }
                throw e;
              }
            }
          });
        });
      }).not.toThrow();
    });

    it("should handle operations without requestBody gracefully", () => {
      // Operations like GET, DELETE typically don't have requestBody
      // The validator should skip them without error
      expect(() => {
        Object.entries(spec.paths).forEach(([pathKey, pathItem]) => {
          Object.entries(pathItem).forEach(([method, operation]) => {
            const op = operation as OpenApiOperation;
            if (!op.requestBody) {
              // These should be skipped without error
              expect(op.requestBody).toBeUndefined();
            }
          });
        });
      }).not.toThrow();
    });

    it("should handle requestBody without examples gracefully", () => {
      // RequestBody can exist without examples (examples are optional)
      expect(() => {
        Object.entries(spec.paths).forEach(([pathKey, pathItem]) => {
          Object.entries(pathItem).forEach(([method, operation]) => {
            const op = operation as OpenApiOperation;
            if (op.requestBody?.content) {
              Object.values(op.requestBody.content).forEach((mediaType) => {
                if (!mediaType.examples) {
                  // These should be skipped without error
                  expect(mediaType.examples).toBeUndefined();
                }
              });
            }
          });
        });
      }).not.toThrow();
    });
  });

  describe("Coverage Metrics", () => {
    it("should track operationId and example counts for coverage reporting", () => {
      let operationIdCount = 0;
      let exampleCount = 0;

      Object.entries(spec.paths).forEach(([pathKey, pathItem]) => {
        Object.entries(pathItem).forEach(([method, operation]) => {
          const op = operation as OpenApiOperation;

          if (op.requestBody?.content) {
            if (op.operationId) {
              operationIdCount++;

              Object.values(op.requestBody.content).forEach((mediaType) => {
                if (mediaType.examples) {
                  exampleCount += Object.keys(mediaType.examples).length;
                }
              });
            }
          }
        });
      });

      console.log(
        `\n📊 Coverage: ${exampleCount} examples across ${operationIdCount} operations with requestBody`
      );

      // Both should be >= 0 (can be 0 if no examples have been added yet)
      expect(operationIdCount).toBeGreaterThanOrEqual(0);
      expect(exampleCount).toBeGreaterThanOrEqual(0);
    });
  });
});
