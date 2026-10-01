#!/usr/bin/env ts-node
/**
 * Simple test runner for pagination property tests
 * Run: ts-node run-pagination-tests.ts
 */

import fc from "fast-check";
import {
  encodeCursor,
  decodeCursor,
  parsePaginationParams,
  CursorPayload,
  PaginationError,
  DEFAULT_LIMIT,
  MAX_LIMIT,
} from "./src/utils/pagination";

interface TestResult {
  name: string;
  passed: boolean;
  error?: string;
  duration: number;
}

const results: TestResult[] = [];

async function runTest(name: string, testFn: () => void): Promise<void> {
  const start = Date.now();
  try {
    testFn();
    results.push({ name, passed: true, duration: Date.now() - start });
    console.log(`✓ ${name}`);
  } catch (error) {
    results.push({
      name,
      passed: false,
      error: String(error),
      duration: Date.now() - start,
    });
    console.log(`✗ ${name}: ${error}`);
  }
}

console.log("Running pagination property-based tests...\n");

// POSITIVE DOMAIN: Round-trip correctness
runTest("Round-trip: encode and decode arbitrary CursorPayload", () => {
  fc.assert(
    fc.property(
      fc.tuple(
        fc.string({ minLength: 1, maxLength: 100 }),
        fc.integer()
      ),
      ([id, sort_val]: [string, number]) => {
        const payload: CursorPayload = { id, sort_val };
        const encoded = encodeCursor(payload);
        const decoded = decodeCursor(encoded);
        if (!decoded || decoded.id !== payload.id || decoded.sort_val !== payload.sort_val) {
          throw new Error(`Round-trip failed: ${JSON.stringify(payload)} -> ${encoded} -> ${JSON.stringify(decoded)}`);
        }
      }
    )
  );
});

runTest("Handle extreme numeric values in sort_val", () => {
  fc.assert(
    fc.property(
      fc.tuple(
        fc.string({ minLength: 1, maxLength: 100 }),
        fc.oneof(
          fc.constant(0),
          fc.constant(1),
          fc.constant(-1),
          fc.constant(Number.MAX_SAFE_INTEGER),
          fc.constant(Number.MIN_SAFE_INTEGER)
        )
      ),
      ([id, sort_val]: [string, number]) => {
        const payload: CursorPayload = { id, sort_val };
        const encoded = encodeCursor(payload);
        const decoded = decodeCursor(encoded);
        if (!decoded || JSON.stringify(decoded) !== JSON.stringify(payload)) {
          throw new Error(`Extreme value test failed for ${JSON.stringify(payload)}`);
        }
      }
    )
  );
});

// NEGATIVE DOMAIN: Malformed inputs
runTest("Reject invalid base64url strings", () => {
  const validChars = new Set("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_=");
  fc.assert(
    fc.property(
      fc.string({ minLength: 1, maxLength: 100 }),
      (input: string) => {
        // Only test strings with invalid characters
        if ([...input].every((c) => validChars.has(c))) {
          return; // skip valid base64url strings
        }
        const result = decodeCursor(input);
        if (result !== null) {
          throw new Error(`Should reject malformed cursor: ${input}`);
        }
      }
    )
  );
});

runTest("Reject JSON with wrong field types", () => {
  const invalid = [
    Buffer.from(JSON.stringify({ id: 123, sort_val: "string" })).toString("base64url"),
    Buffer.from(JSON.stringify({ id: "abc" })).toString("base64url"),
    Buffer.from(JSON.stringify({ sort_val: 123 })).toString("base64url"),
    Buffer.from(JSON.stringify({ id: null, sort_val: 123 })).toString("base64url"),
    Buffer.from(JSON.stringify({ id: "abc", sort_val: NaN })).toString("base64url"),
    Buffer.from(JSON.stringify({ id: "abc", sort_val: Infinity })).toString("base64url"),
  ];

  for (const cursor of invalid) {
    const result = decodeCursor(cursor);
    if (result !== null) {
      throw new Error(`Should reject invalid cursor: ${cursor}`);
    }
  }
});

runTest("Reject empty and whitespace cursors", () => {
  const invalid = ["", " ", "\t", "\n", "   "];
  for (const cursor of invalid) {
    const result = decodeCursor(cursor);
    if (result !== null) {
      throw new Error(`Should reject empty cursor: "${cursor}"`);
    }
  }
});

// INVARIANT PROPERTIES: Limit clamping
runTest("Clamp limit to [1, MAX_LIMIT]", () => {
  fc.assert(
    fc.property(fc.integer(), (limitValue: number) => {
      try {
        const params = parsePaginationParams({ limit: limitValue });
        // If no error thrown, verify clamping
        if (params.limit < 1 || params.limit > MAX_LIMIT) {
          throw new Error(`Limit not clamped: got ${params.limit}, expected [1, ${MAX_LIMIT}]`);
        }
      } catch (error) {
        // Negative/zero limits should throw PaginationError
        if (limitValue < 1 && error instanceof PaginationError) {
          return; // Expected behavior
        }
        throw error;
      }
    })
  );
});

runTest("Return DEFAULT_LIMIT when limit is undefined", () => {
  const params = parsePaginationParams({});
  if (params.limit !== DEFAULT_LIMIT) {
    throw new Error(`Expected DEFAULT_LIMIT (${DEFAULT_LIMIT}), got ${params.limit}`);
  }
});

runTest("Accept and clamp large limits to MAX_LIMIT", () => {
  fc.assert(
    fc.property(
      fc.integer({ min: MAX_LIMIT + 1, max: Number.MAX_SAFE_INTEGER }),
      (largeLimit: number) => {
        const params = parsePaginationParams({ limit: largeLimit });
        if (params.limit !== MAX_LIMIT) {
          throw new Error(`Should clamp ${largeLimit} to ${MAX_LIMIT}, got ${params.limit}`);
        }
      }
    )
  );
});

runTest("Accept limits in valid range [1, MAX_LIMIT]", () => {
  fc.assert(
    fc.property(fc.integer({ min: 1, max: MAX_LIMIT }), (validLimit: number) => {
      const params = parsePaginationParams({ limit: validLimit });
      if (params.limit !== validLimit) {
        throw new Error(`Should accept limit ${validLimit}, got ${params.limit}`);
      }
    })
  );
});

// CURSOR PARAMETER PARSING
runTest("Decode valid cursors passed to parsePaginationParams", () => {
  fc.assert(
    fc.property(
      fc.tuple(
        fc.string({ minLength: 1, maxLength: 100 }),
        fc.integer()
      ),
      ([id, sort_val]: [string, number]) => {
        const payload: CursorPayload = { id, sort_val };
        const encoded = encodeCursor(payload);
        const params = parsePaginationParams({ cursor: encoded });
        if (!params.cursor || JSON.stringify(params.cursor) !== JSON.stringify(payload)) {
          throw new Error(`Failed to decode cursor in parsePaginationParams`);
        }
      }
    )
  );
});

// DETERMINISM
runTest("Encode deterministically", () => {
  fc.assert(
    fc.property(
      fc.tuple(
        fc.string({ minLength: 1, maxLength: 100 }),
        fc.integer()
      ),
      ([id, sort_val]: [string, number]) => {
        const payload: CursorPayload = { id, sort_val };
        const encoded1 = encodeCursor(payload);
        const encoded2 = encodeCursor(payload);
        if (encoded1 !== encoded2) {
          throw new Error(`Encoding not deterministic: ${encoded1} vs ${encoded2}`);
        }
      }
    )
  );
});

// Print results summary
console.log("\n" + "=".repeat(60));
const passed = results.filter((r) => r.passed).length;
const failed = results.filter((r) => !r.passed).length;
const totalTime = results.reduce((sum, r) => sum + r.duration, 0);

console.log(`\nTest Results: ${passed} passed, ${failed} failed`);
console.log(`Total time: ${totalTime}ms`);
console.log("=".repeat(60) + "\n");

if (failed > 0) {
  console.log("Failed tests:");
  results.filter((r) => !r.passed).forEach((r) => {
    console.log(`  - ${r.name}`);
    if (r.error) {
      console.log(`    ${r.error}`);
    }
  });
  process.exit(1);
} else {
  console.log("✓ All tests passed!");
  process.exit(0);
}
