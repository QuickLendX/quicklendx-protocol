import { z } from 'zod';
import * as fc from 'fast-check';
import * as sharedSchemas from '../validators/shared';
import * as invoiceSchemas from '../validators/invoices';
import * as bidSchemas from '../validators/bids';
import * as settlementSchemas from '../validators/settlements';

// Helper to extract Zod schemas from a module
function extractSchemas(moduleObj: Record<string, any>): Record<string, z.ZodTypeAny> {
  const schemas: Record<string, z.ZodTypeAny> = {};
  for (const [key, val] of Object.entries(moduleObj)) {
    if (val && typeof val === 'object' && 'safeParse' in val && typeof val.safeParse === 'function') {
      schemas[key] = val;
    }
  }
  return schemas;
}

const allSchemas = {
  ...extractSchemas(sharedSchemas),
  ...extractSchemas(invoiceSchemas),
  ...extractSchemas(bidSchemas),
  ...extractSchemas(settlementSchemas),
};

describe('Zod Validators Fuzz Tests (Property-Based)', () => {
  it('should have extracted at least one schema to test', () => {
    expect(Object.keys(allSchemas).length).toBeGreaterThan(0);
  });

  describe.each(Object.keys(allSchemas))('Schema: %s', (schemaName) => {
    const schema = allSchemas[schemaName];

    it('should never throw an unhandled error for arbitrary deeply-nested payloads', () => {
      fc.assert(
        fc.property(
          fc.object({ maxDepth: 100, maxKeys: 10 }),
          (payload) => {
            // Zod's safeParse should catch all validation errors internally
            // If it throws, the test fails
            expect(() => schema.safeParse(payload)).not.toThrow();
          }
        ),
        { numRuns: 100 }
      );
    });

    it('should never throw an unhandled error for arbitrary values (NaN, Infinity, Strings, etc.)', () => {
      fc.assert(
        fc.property(
          fc.anything({ maxDepth: 10 }),
          (payload) => {
            expect(() => schema.safeParse(payload)).not.toThrow();
          }
        ),
        { numRuns: 1000 }
      );
    });

    it('should protect against prototype pollution and type confusion', () => {
      fc.assert(
        fc.property(
          // Fuzz standard objects with potentially dangerous keys explicitly added
          fc.record({
            __proto__: fc.constant({ polluted: true }),
            constructor: fc.constant({ prototype: { polluted: true } }),
            amount: fc.constant({ toString: () => "1" }), // Type confusion
            invoice_id: fc.constant({ valueOf: () => "0x123" }),
            randomField: fc.string(),
          }),
          (payload) => {
            // Because we generated an object that literally has __proto__ defined via fc.record,
            // we simulate a JSON.parse payload that an attacker might send.
            
            // Re-create the payload using JSON.parse to properly set the prototype 
            // if it was passed via express body parser
            const jsonPayload = JSON.parse(JSON.stringify(payload));
            
            let result: any;
            expect(() => {
              result = schema.safeParse(jsonPayload);
            }).not.toThrow();

            if (result && result.success) {
              const output = result.data;
              // If safeParse succeeds, the parsed object must NOT have the polluted prototype injected
              if (output && typeof output === 'object') {
                expect((output as any).polluted).toBeUndefined();
                // Ensure output prototype isn't modified directly
                expect(Object.prototype.hasOwnProperty.call(output, 'polluted')).toBe(false);
              }
            }
          }
        ),
        { numRuns: 100 }
      );
    });
  });
});
