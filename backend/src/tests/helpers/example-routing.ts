/**
 * example-routing.ts
 *
 * Central manifest mapping OpenAPI operationIds to their corresponding
 * request body Zod validators. This file ensures every endpoint with a
 * requestBody.examples.value entry has a registered validator.
 *
 * When a new endpoint with examples is added to openapi.yaml:
 *   1. Import its validator from backend/src/validators/*.ts
 *   2. Add a mapping entry to OPERATION_ID_TO_SCHEMA below
 *   3. The test suite will automatically validate all examples against it
 *
 * Validation strategy:
 *   • If an operationId appears in openapi.yaml but NOT in this manifest → FAIL_LOUD
 *   • If an example fails Zod validation → FAIL_LOUD
 *   • If no examples exist for an endpoint → PASS (optional coverage)
 *
 * This prevents silent failures where validators change but documentation lies.
 */

import { z } from "zod";
import { createBidBodySchema } from "../../validators/bids";
import { createInvoiceBodySchema } from "../../validators/invoices";

/**
 * Mapping of OpenAPI operationId → Zod request body schema.
 * Add new entries as you add requestBody examples to openapi.yaml.
 */
export const OPERATION_ID_TO_SCHEMA: Record<string, z.ZodType<any>> = {
  // Bids endpoints
  createBid: createBidBodySchema,

  // Invoices endpoints
  createInvoice: createInvoiceBodySchema,

  // Admin endpoints (schemas are inline in openapi.yaml, so we define them here)
  toggleMaintenanceMode: z.object({
    enabled: z.boolean(),
  }),

  updateDangerousAdminConfig: z.object({
    allowEmergencyConfigChanges: z.boolean(),
    maintenanceWindowMinutes: z.number().int().min(1).max(1440),
  }),

  queueBackfillJob: z.object({
    scope: z.string().optional(),
  }),

  // Notifications endpoints
  updateNotificationPreferences: z.object({
    email_enabled: z.boolean().optional(),
    email_address: z.string().email().optional(),
    notifications: z.record(z.string(), z.boolean()).optional(),
  }),

  // Events endpoint
  processBlockchainEvents: z.union([
    z.object({
      type: z.string(),
      data: z.record(z.string(), z.any()).optional(),
    }),
    z.array(
      z.object({
        type: z.string(),
        data: z.record(z.string(), z.any()).optional(),
      })
    ),
  ]),
};

/**
 * Validates that all required operationIds are registered in the manifest.
 * Call this during test setup to catch missing mappings early.
 *
 * @param operationIds - Set of operationIds found in openapi.yaml with requestBody.examples
 * @throws Error if any operationId is not registered
 */
export function validateRegisteredOperationIds(operationIds: Set<string>): void {
  const missingMappings = Array.from(operationIds).filter(
    (id) => !(id in OPERATION_ID_TO_SCHEMA)
  );

  if (missingMappings.length > 0) {
    throw new Error(
      `Missing operationId → schema mappings in example-routing.ts:\n` +
        missingMappings.map((id) => `  - ${id}`).join("\n") +
        `\n\nAdd these mappings to OPERATION_ID_TO_SCHEMA to enable example validation.`
    );
  }
}

/**
 * Gets the schema validator for a given operationId.
 *
 * @param operationId - The operationId from openapi.yaml
 * @returns The Zod schema, or throws an error if not found
 */
export function getSchemaForOperationId(operationId: string): z.ZodType<any> {
  const schema = OPERATION_ID_TO_SCHEMA[operationId];
  if (!schema) {
    throw new Error(
      `No schema registered for operationId: ${operationId}\n` +
        `Register it in backend/src/tests/helpers/example-routing.ts`
    );
  }
  return schema;
}
