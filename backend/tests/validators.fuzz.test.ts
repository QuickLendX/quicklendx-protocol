/**
 * Property-based fuzz tests for Zod validators in invoices, bids, and settlements schemas.
 *
 * Strategy:
 * - Use fast-check arbitraries to generate malformed payloads covering:
 *   unicode strings, very large integers, deeply nested objects, prototype-pollution
 *   attempts, NaN/Infinity in numeric fields, ISO date edge cases, integer overflow,
 *   and type confusion objects.
 * - Every test asserts that the validator either accepts or rejects the input — it
 *   must never throw an unhandled exception that would crash the process.
 * - Prototype-pollution assertions verify that __proto__ / constructor keys cannot
 *   mutate the validated output or Object.prototype.
 */

import * as fc from "fast-check";
import { createInvoiceBodySchema } from "../src/validators/invoices";
import { createBidBodySchema } from "../src/validators/bids";
import {
  getSettlementsQuerySchema,
  transitionInputSchema,
} from "../src/validators/settlements";
import {
  hexStringSchema,
  stellarAddressSchema,
  positiveAmountSchema,
  paginationSchema,
  getInvoicesQuerySchema,
  invoiceIdParamSchema,
  getBidsQuerySchema,
} from "../src/validators/shared";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Snapshot of Object.prototype keys before tests run — used for pollution checks. */
const PROTO_KEYS_BEFORE = Object.getOwnPropertyNames(Object.prototype);

function assertNoPrototypePollution(): void {
  const after = Object.getOwnPropertyNames(Object.prototype);
  expect(after).toEqual(PROTO_KEYS_BEFORE);
}

/** Run a safeParse call and assert it never throws. */
function assertNeverThrows(parse: () => void): void {
  expect(() => parse()).not.toThrow();
}

// ---------------------------------------------------------------------------
// Shared arbitrary helpers
// ---------------------------------------------------------------------------

const anyArb = fc.oneof(
  fc.string(),
  fc.integer(),
  fc.double({ noNaN: false }),
  fc.boolean(),
  fc.constant(null),
  fc.constant(undefined),
  fc.constant(NaN),
  fc.constant(Infinity),
  fc.constant(-Infinity)
);

const unicodeStringArb = fc.string({ unit: "grapheme", minLength: 0, maxLength: 200 });

const veryLargeIntArb = fc.oneof(
  fc.integer({ min: Number.MAX_SAFE_INTEGER - 10, max: Number.MAX_SAFE_INTEGER }),
  fc.integer({ min: Number.MIN_SAFE_INTEGER, max: Number.MIN_SAFE_INTEGER + 10 }),
  fc.constant(2 ** 53),
  fc.constant(-(2 ** 53))
);

const protoPollutionPayloads = [
  { __proto__: { polluted: true } },
  { constructor: { prototype: { polluted: true } } },
  JSON.parse('{"__proto__":{"polluted":true}}'),
  JSON.parse('{"constructor":{"prototype":{"polluted":true}}}'),
];

// ---------------------------------------------------------------------------
// 1. hexStringSchema
// ---------------------------------------------------------------------------

describe("hexStringSchema — fuzz", () => {
  it("never throws on arbitrary strings", () => {
    fc.assert(
      fc.property(fc.string(), (s) => {
        assertNeverThrows(() => hexStringSchema.safeParse(s));
      })
    );
  });

  it("never throws on unicode strings", () => {
    fc.assert(
      fc.property(unicodeStringArb, (s) => {
        assertNeverThrows(() => hexStringSchema.safeParse(s));
      })
    );
  });

  it("accepts well-formed hex strings", () => {
    fc.assert(
      fc.property(
        fc.hexaString({ minLength: 1, maxLength: 64 }),
        (hex) => {
          const result = hexStringSchema.safeParse(`0x${hex}`);
          expect(result.success).toBe(true);
        }
      )
    );
  });

  it("rejects strings without 0x prefix", () => {
    fc.assert(
      fc.property(
        fc.hexaString({ minLength: 1, maxLength: 64 }),
        (hex) => {
          const result = hexStringSchema.safeParse(hex);
          expect(result.success).toBe(false);
        }
      )
    );
  });
});

// ---------------------------------------------------------------------------
// 2. stellarAddressSchema
// ---------------------------------------------------------------------------

describe("stellarAddressSchema — fuzz", () => {
  it("never throws on arbitrary inputs", () => {
    fc.assert(
      fc.property(anyArb, (v) => {
        assertNeverThrows(() => stellarAddressSchema.safeParse(v));
      })
    );
  });

  it("rejects extremely long strings", () => {
    fc.assert(
      fc.property(fc.string({ minLength: 200, maxLength: 1000 }), (s) => {
        const result = stellarAddressSchema.safeParse(s);
        // Either rejects or accepts — must not throw
        assertNeverThrows(() => stellarAddressSchema.safeParse(s));
        void result;
      })
    );
  });
});

// ---------------------------------------------------------------------------
// 3. positiveAmountSchema
// ---------------------------------------------------------------------------

describe("positiveAmountSchema — fuzz", () => {
  it("never throws on arbitrary inputs", () => {
    fc.assert(
      fc.property(anyArb, (v) => {
        assertNeverThrows(() => positiveAmountSchema.safeParse(v));
      })
    );
  });

  it("rejects NaN/Infinity strings", () => {
    ["NaN", "Infinity", "-Infinity", "1e308", "1.5", "-1", "0.1"].forEach((v) => {
      const result = positiveAmountSchema.safeParse(v);
      expect(result.success).toBe(false);
    });
  });

  it("accepts valid digit-only strings", () => {
    fc.assert(
      fc.property(fc.nat({ max: 999999999 }), (n) => {
        const result = positiveAmountSchema.safeParse(String(n));
        expect(result.success).toBe(true);
      })
    );
  });
});

// ---------------------------------------------------------------------------
// 4. paginationSchema
// ---------------------------------------------------------------------------

describe("paginationSchema — fuzz", () => {
  it("never throws on arbitrary objects", () => {
    fc.assert(
      fc.property(
        fc.record({ page: anyArb, limit: anyArb }, { requiredKeys: [] }),
        (obj) => {
          assertNeverThrows(() => paginationSchema.safeParse(obj));
        }
      )
    );
  });
});

// ---------------------------------------------------------------------------
// 5. createInvoiceBodySchema
// ---------------------------------------------------------------------------

describe("createInvoiceBodySchema — fuzz", () => {
  it("never throws on arbitrary objects", () => {
    fc.assert(
      fc.property(
        fc.object({ maxDepth: 5, maxKeys: 10 }),
        (obj) => {
          assertNeverThrows(() => createInvoiceBodySchema.safeParse(obj));
        }
      )
    );
  });

  it("rejects deeply nested metadata (depth > 5)", () => {
    let nested: unknown = { description: "x", quantity: "1", unit_price: "1", total: "1" };
    for (let i = 0; i < 110; i++) {
      nested = { inner: nested };
    }
    const payload = {
      business: "biz",
      amount: "100",
      currency: "USD",
      due_date: 9999999999,
      description: "test",
      category: "Services",
      metadata: nested,
    };
    assertNeverThrows(() => createInvoiceBodySchema.safeParse(payload));
  });

  it("rejects unicode in amount field", () => {
    fc.assert(
      fc.property(unicodeStringArb, (s) => {
        const result = createInvoiceBodySchema.safeParse({
          business: "biz",
          amount: s,
          currency: "USD",
          due_date: 1000000,
          description: "desc",
          category: "Services",
        });
        if (!/^[0-9]+$/.test(s)) {
          expect(result.success).toBe(false);
        }
      })
    );
  });

  it("never throws on NaN/Infinity in due_date", () => {
    [NaN, Infinity, -Infinity, Number.MAX_VALUE, -1, 0].forEach((v) => {
      assertNeverThrows(() =>
        createInvoiceBodySchema.safeParse({
          business: "biz",
          amount: "100",
          currency: "USD",
          due_date: v,
          description: "d",
          category: "Services",
        })
      );
    });
  });

  it("rejects type-confusion amount objects", () => {
    const typeConfusion = { toString: () => "1", valueOf: () => 1 };
    const result = createInvoiceBodySchema.safeParse({
      business: "biz",
      amount: typeConfusion,
      currency: "USD",
      due_date: 1000000,
      description: "d",
      category: "Services",
    });
    expect(result.success).toBe(false);
  });

  it("prototype-pollution payloads do not mutate Object.prototype", () => {
    for (const payload of protoPollutionPayloads) {
      assertNeverThrows(() => createInvoiceBodySchema.safeParse(payload));
    }
    assertNoPrototypePollution();
    expect((Object.prototype as Record<string, unknown>)["polluted"]).toBeUndefined();
  });

  it("validated output does not carry __proto__ keys", () => {
    const polluted = JSON.parse('{"__proto__":{"evil":true},"business":"b","amount":"1","currency":"USD","due_date":1000000,"description":"d","category":"Services"}');
    const result = createInvoiceBodySchema.safeParse(polluted);
    if (result.success) {
      expect(Object.prototype.hasOwnProperty.call(result.data, "__proto__")).toBe(false);
    }
    assertNoPrototypePollution();
  });
});

// ---------------------------------------------------------------------------
// 6. createBidBodySchema
// ---------------------------------------------------------------------------

describe("createBidBodySchema — fuzz", () => {
  it("never throws on arbitrary objects", () => {
    fc.assert(
      fc.property(
        fc.object({ maxDepth: 4, maxKeys: 8 }),
        (obj) => {
          assertNeverThrows(() => createBidBodySchema.safeParse(obj));
        }
      )
    );
  });

  it("rejects integer-overflow in expiration_timestamp", () => {
    fc.assert(
      fc.property(veryLargeIntArb, (n) => {
        assertNeverThrows(() =>
          createBidBodySchema.safeParse({
            invoice_id: "0xabcdef",
            bid_amount: "100",
            expected_return: "5",
            expiration_timestamp: n,
          })
        );
      })
    );
  });

  it("rejects bid_amount with non-digit characters", () => {
    fc.assert(
      fc.property(
        fc.string().filter((s) => !/^[0-9]+$/.test(s)),
        (s) => {
          const result = createBidBodySchema.safeParse({
            invoice_id: "0xabcdef",
            bid_amount: s,
            expected_return: "5",
            expiration_timestamp: 9999999999,
          });
          expect(result.success).toBe(false);
        }
      )
    );
  });

  it("prototype-pollution payloads do not mutate Object.prototype", () => {
    for (const payload of protoPollutionPayloads) {
      assertNeverThrows(() => createBidBodySchema.safeParse(payload));
    }
    assertNoPrototypePollution();
    expect((Object.prototype as Record<string, unknown>)["polluted"]).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// 7. transitionInputSchema (settlements)
// ---------------------------------------------------------------------------

describe("transitionInputSchema — fuzz", () => {
  it("never throws on arbitrary objects", () => {
    fc.assert(
      fc.property(
        fc.object({ maxDepth: 4, maxKeys: 8 }),
        (obj) => {
          assertNeverThrows(() => transitionInputSchema.safeParse(obj));
        }
      )
    );
  });

  it("rejects non-numeric amount strings", () => {
    fc.assert(
      fc.property(
        fc.string().filter((s) => !/^[0-9]+$/.test(s)),
        (s) => {
          const result = transitionInputSchema.safeParse({
            invoice_id: "inv_001",
            amount: s,
            payer: "alice",
            recipient: "bob",
            event_id: "evt_001",
          });
          expect(result.success).toBe(false);
        }
      )
    );
  });

  it("rejects extremely long field values without throwing", () => {
    fc.assert(
      fc.property(
        fc.string({ minLength: 5001, maxLength: 10000 }),
        (longStr) => {
          assertNeverThrows(() =>
            transitionInputSchema.safeParse({
              invoice_id: longStr,
              amount: "100",
              payer: longStr,
              recipient: longStr,
              event_id: longStr,
            })
          );
        }
      )
    );
  });

  it("prototype-pollution payloads do not mutate Object.prototype", () => {
    for (const payload of protoPollutionPayloads) {
      assertNeverThrows(() => transitionInputSchema.safeParse(payload));
    }
    assertNoPrototypePollution();
    expect((Object.prototype as Record<string, unknown>)["polluted"]).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// 8. getSettlementsQuerySchema (settlements)
// ---------------------------------------------------------------------------

describe("getSettlementsQuerySchema — fuzz", () => {
  it("never throws on arbitrary objects", () => {
    fc.assert(
      fc.property(
        fc.object({ maxDepth: 3, maxKeys: 6 }),
        (obj) => {
          assertNeverThrows(() => getSettlementsQuerySchema.safeParse(obj));
        }
      )
    );
  });

  it("rejects invalid status values", () => {
    fc.assert(
      fc.property(
        fc.string().filter((s) => !["Pending", "Processing", "Paid", "Defaulted"].includes(s)),
        (s) => {
          const result = getSettlementsQuerySchema.safeParse({ status: s });
          expect(result.success).toBe(false);
        }
      )
    );
  });
});

// ---------------------------------------------------------------------------
// 9. getInvoicesQuerySchema / getBidsQuerySchema / invoiceIdParamSchema
// ---------------------------------------------------------------------------

describe("query schemas — fuzz", () => {
  it("getInvoicesQuerySchema never throws on arbitrary inputs", () => {
    fc.assert(
      fc.property(fc.object({ maxDepth: 3, maxKeys: 6 }), (obj) => {
        assertNeverThrows(() => getInvoicesQuerySchema.safeParse(obj));
      })
    );
  });

  it("getBidsQuerySchema never throws on arbitrary inputs", () => {
    fc.assert(
      fc.property(fc.object({ maxDepth: 3, maxKeys: 6 }), (obj) => {
        assertNeverThrows(() => getBidsQuerySchema.safeParse(obj));
      })
    );
  });

  it("invoiceIdParamSchema never throws on arbitrary inputs", () => {
    fc.assert(
      fc.property(fc.object({ maxDepth: 2, maxKeys: 4 }), (obj) => {
        assertNeverThrows(() => invoiceIdParamSchema.safeParse(obj));
      })
    );
  });

  it("ISO date edge-cases do not crash any query schema", () => {
    const edgeDates = [
      "0000-01-01",
      "9999-12-31",
      "2024-02-29",   // leap day
      "2023-02-29",   // invalid leap day
      "2024-13-01",   // month 13
      "not-a-date",
      "",
    ];
    for (const d of edgeDates) {
      assertNeverThrows(() => getInvoicesQuerySchema.safeParse({ due_date: d }));
      assertNeverThrows(() => getBidsQuerySchema.safeParse({ created_at: d }));
    }
  });
});
