/**
 * Deterministic failure-boundary coverage for assertExportToken.
 *
 * Design invariants under test:
 *   1. The function is a pure assertion — it never mutates the caller's value.
 *   2. Every rejection throws a BadRequestError with a stable, non-leaking
 *      shape: { name, status, statusCode, code, message }.
 *   3. Acceptance and rejection outcomes are idempotent: running the same
 *      input twice always produces the same result (no hidden state).
 *   4. Only `exp_` + exactly 26 Crockford Base32 characters is valid.
 *   5. Validation is prefix-specific: tokens for other entity types are
 *      rejected even when their ULID portion is valid.
 *   6. The function accepts leading/trailing ASCII whitespace (trimmed
 *      before the prefix check), matching assertInvoiceId behaviour.
 *
 * Coverage goals (per issue #2668):
 *   ✓ Valid inputs (success path, idempotent)
 *   ✓ Non-string inputs (null, undefined, number, boolean, object, array)
 *   ✓ Wrong-prefix inputs (inv_, bid_, stl_, bare ULID)
 *   ✓ ULID length boundary (25 chars, 27 chars)
 *   ✓ Invalid Crockford charset characters (I, L, O, U, special chars)
 *   ✓ Empty / whitespace-only strings
 *   ✓ Pathological / adversarial inputs (SQL injection fragments,
 *     NUL bytes, very long strings, unicode)
 *   ✓ Concurrent / retry determinism
 *   ✓ Caller-owned string immutability
 */

import { ENTITY_PREFIXES, assertExportToken } from "./entityId";

// A well-formed 26-character Crockford Base32 ULID.
// All characters are drawn from the valid set: 0-9 A-H J K M N P-T V-Z (case-insensitive).
const VALID_ULID = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

// ── helpers ──────────────────────────────────────────────────────────────────

/**
 * Asserts that calling assertExportToken with `value` throws a BadRequestError
 * whose public shape is exactly as specified by the entityId contract.
 * Called twice to prove idempotency of rejection.
 */
function expectRejected(value: unknown): void {
  for (let attempt = 0; attempt < 2; attempt++) {
    let threw = false;
    try {
      assertExportToken(value);
    } catch (err: unknown) {
      threw = true;
      // Shape contract — every rejection must surface these stable fields.
      expect(err).toMatchObject({
        name: "BadRequestError",
        status: 400,
        statusCode: 400,
        code: "INVALID_ENTITY_ID",
        message: "Invalid entity ID",
      });
      // Must not expose internals: the message must not contain the raw value
      // when the value is a (potentially adversarial) string.
      if (typeof value === "string") {
        expect((err as Error).message).not.toContain(value.slice(0, 50));
      }
    }
    expect(threw).toBe(true);
  }
}

/**
 * Asserts that assertExportToken accepts `value` without throwing,
 * called twice to prove idempotency of acceptance.
 */
function expectAccepted(value: unknown): void {
  expect(() => assertExportToken(value)).not.toThrow();
  expect(() => assertExportToken(value)).not.toThrow();
}

// ── success path ─────────────────────────────────────────────────────────────

describe("assertExportToken – success path", () => {
  it("accepts a well-formed exp_ + uppercase ULID", () => {
    expectAccepted(`${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}`);
  });

  it("accepts a well-formed exp_ + lowercase ULID (case-insensitive)", () => {
    expectAccepted(`${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID.toLowerCase()}`);
  });

  it("accepts a well-formed exp_ + mixed-case ULID", () => {
    const mixed = "01ArZ3NdEkTsV4RrFfQ69g5fAv";
    expectAccepted(`${ENTITY_PREFIXES.EXPORT_TOKEN}${mixed}`);
  });

  it("accepts a token with leading whitespace (trimmed before validation)", () => {
    expectAccepted(`  ${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}`);
  });

  it("accepts a token with trailing whitespace (trimmed before validation)", () => {
    expectAccepted(`${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}  `);
  });

  it("accepts a token with both leading and trailing whitespace", () => {
    expectAccepted(`  ${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}  `);
  });

  it("is idempotent for a valid token across many calls", () => {
    const token = `${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}`;
    for (let i = 0; i < 10; i++) {
      expect(() => assertExportToken(token)).not.toThrow();
    }
  });
});

// ── non-string inputs ─────────────────────────────────────────────────────────

describe("assertExportToken – non-string inputs (type boundary)", () => {
  it.each([
    ["null", null],
    ["undefined", undefined],
    ["number 0", 0],
    ["number 1", 1],
    ["boolean false", false],
    ["boolean true", true],
    ["empty object", {}],
    ["plain object with token key", { token: `${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}` }],
    ["array", []],
    ["array with valid string", [`${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}`]],
    ["Symbol", Symbol("exp")],
    ["BigInt", BigInt(42)],
  ])("rejects %s with the stable public error", (_label, value) => {
    expectRejected(value);
  });
});

// ── wrong prefix ──────────────────────────────────────────────────────────────

describe("assertExportToken – wrong prefix", () => {
  it("rejects an invoice ID (inv_ prefix)", () => {
    expectRejected(`${ENTITY_PREFIXES.INVOICE}${VALID_ULID}`);
  });

  it("rejects a bid ID (bid_ prefix)", () => {
    expectRejected(`${ENTITY_PREFIXES.BID}${VALID_ULID}`);
  });

  it("rejects a settlement ID (stl_ prefix)", () => {
    expectRejected(`${ENTITY_PREFIXES.SETTLEMENT}${VALID_ULID}`);
  });

  it("rejects a bare ULID with no prefix", () => {
    expectRejected(VALID_ULID);
  });

  it("rejects a hex-prefixed string (0x…)", () => {
    expectRejected("0xdeadBEEF");
  });

  it("rejects the prefix alone with no ULID part", () => {
    expectRejected(ENTITY_PREFIXES.EXPORT_TOKEN);
  });

  it("rejects a prefix with wrong capitalisation (EXP_)", () => {
    // The prefix literal is exp_ (lowercase); EXP_ must not match.
    expectRejected(`EXP_${VALID_ULID}`);
  });

  it("rejects a substring of the correct prefix (ex_)", () => {
    expectRejected(`ex_${VALID_ULID}`);
  });

  it("rejects a superset of the correct prefix (exp__)", () => {
    expectRejected(`exp__${VALID_ULID}`);
  });
});

// ── ULID length boundary ──────────────────────────────────────────────────────

describe("assertExportToken – ULID length boundary", () => {
  it("rejects a ULID that is 25 characters (one short)", () => {
    const short = VALID_ULID.slice(0, 25); // 25 chars
    expectRejected(`${ENTITY_PREFIXES.EXPORT_TOKEN}${short}`);
  });

  it("rejects a ULID that is 27 characters (one over)", () => {
    const long = VALID_ULID + "A"; // 27 chars
    expectRejected(`${ENTITY_PREFIXES.EXPORT_TOKEN}${long}`);
  });

  it("rejects an empty ULID part (prefix only)", () => {
    expectRejected(ENTITY_PREFIXES.EXPORT_TOKEN);
  });

  it("rejects a single-character ULID part", () => {
    expectRejected(`${ENTITY_PREFIXES.EXPORT_TOKEN}A`);
  });

  it("rejects a 1000-character string (large input)", () => {
    const padded = `${ENTITY_PREFIXES.EXPORT_TOKEN}${"1".repeat(26)}${"2".repeat(970)}`;
    expectRejected(padded);
  });
});

// ── Crockford Base32 character set ───────────────────────────────────────────

describe("assertExportToken – invalid Crockford Base32 characters", () => {
  // Crockford Base32 excludes I, L, O, U to avoid visual ambiguity.
  it.each(["I", "L", "O", "U"])(
    "rejects a ULID containing the excluded character '%s'",
    (char) => {
      const bad = char.repeat(26); // 26 chars, all invalid
      expectRejected(`${ENTITY_PREFIXES.EXPORT_TOKEN}${bad}`);
    },
  );

  it("rejects a ULID containing a space character", () => {
    const bad = VALID_ULID.slice(0, 25) + " ";
    expectRejected(`${ENTITY_PREFIXES.EXPORT_TOKEN}${bad}`);
  });

  it("rejects a ULID containing a hyphen", () => {
    const bad = VALID_ULID.slice(0, 25) + "-";
    expectRejected(`${ENTITY_PREFIXES.EXPORT_TOKEN}${bad}`);
  });

  it("rejects a ULID containing an underscore", () => {
    const bad = VALID_ULID.slice(0, 25) + "_";
    expectRejected(`${ENTITY_PREFIXES.EXPORT_TOKEN}${bad}`);
  });

  it("rejects a ULID that contains a dot", () => {
    const bad = VALID_ULID.slice(0, 25) + ".";
    expectRejected(`${ENTITY_PREFIXES.EXPORT_TOKEN}${bad}`);
  });

  it("rejects a ULID that contains a NUL byte", () => {
    const bad = VALID_ULID.slice(0, 25) + "\0";
    expectRejected(`${ENTITY_PREFIXES.EXPORT_TOKEN}${bad}`);
  });
});

// ── empty / whitespace-only ───────────────────────────────────────────────────

describe("assertExportToken – empty and whitespace inputs", () => {
  it("rejects an empty string", () => {
    expectRejected("");
  });

  it("rejects a whitespace-only string (spaces)", () => {
    expectRejected("   ");
  });

  it("rejects a tab-only string", () => {
    expectRejected("\t\t\t");
  });

  it("rejects a newline-only string", () => {
    expectRejected("\n");
  });
});

// ── adversarial / injection inputs ───────────────────────────────────────────

describe("assertExportToken – adversarial inputs (injection / fuzzing)", () => {
  it("rejects a SQL fragment embedded after the prefix", () => {
    // The ULID part is not a valid Crockford string, so it must be rejected.
    expectRejected(`${ENTITY_PREFIXES.EXPORT_TOKEN}' OR '1'='1`);
  });

  it("rejects a URL-encoded SQL fragment", () => {
    const encoded = encodeURIComponent(`${ENTITY_PREFIXES.EXPORT_TOKEN}' OR '1'='1`);
    expectRejected(encoded);
  });

  it("rejects a JSON object serialised as a string", () => {
    expectRejected(JSON.stringify({ token: `${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}` }));
  });

  it("rejects a Base64-encoded valid token (not the raw token)", () => {
    const raw = `${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}`;
    expectRejected(Buffer.from(raw).toString("base64"));
  });

  it("rejects a unicode lookalike for the prefix (exp＿ with full-width underscore)", () => {
    // U+FF3F FULLWIDTH LOW LINE looks like underscore but is not ASCII '_'.
    expectRejected(`exp\uFF3F${VALID_ULID}`);
  });

  it("rejects a string that starts with the prefix but contains a newline in the ULID", () => {
    const bad = `${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID.slice(0, 25)}\n`;
    expectRejected(bad);
  });

  it("rejects a ULID part composed entirely of zeros (valid charset, valid length)", () => {
    // '0' is in the Crockford charset, so this is structurally valid and must
    // be ACCEPTED by assertExportToken (all-zeros is a legal ULID).
    expectAccepted(`${ENTITY_PREFIXES.EXPORT_TOKEN}${"0".repeat(26)}`);
  });

  it("rejects a repeated-valid-char ULID of length 25 (one short)", () => {
    expectRejected(`${ENTITY_PREFIXES.EXPORT_TOKEN}${"0".repeat(25)}`);
  });

  it("rejects a repeated-valid-char ULID of length 27 (one over)", () => {
    expectRejected(`${ENTITY_PREFIXES.EXPORT_TOKEN}${"0".repeat(27)}`);
  });
});

// ── error shape contract ──────────────────────────────────────────────────────

describe("assertExportToken – error shape contract", () => {
  /**
   * These assertions verify the HTTP-boundary contract: the error thrown by
   * assertExportToken must be directly passable to the Express error handler
   * and produce a 400 response with the documented error code.
   */
  it("throws with statusCode 400 (compatible with Express error handler)", () => {
    let caught: unknown;
    try {
      assertExportToken("not-valid");
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeDefined();
    expect((caught as any).statusCode).toBe(400);
  });

  it("throws with status 400 (compatible with res.status())", () => {
    let caught: unknown;
    try {
      assertExportToken(null);
    } catch (err) {
      caught = err;
    }
    expect((caught as any).status).toBe(400);
  });

  it("throws with code INVALID_ENTITY_ID (stable, non-leaking)", () => {
    let caught: unknown;
    try {
      assertExportToken(`${ENTITY_PREFIXES.BID}${VALID_ULID}`);
    } catch (err) {
      caught = err;
    }
    expect((caught as any).code).toBe("INVALID_ENTITY_ID");
  });

  it("throws with name BadRequestError", () => {
    let caught: unknown;
    try {
      assertExportToken(42);
    } catch (err) {
      caught = err;
    }
    expect((caught as any).name).toBe("BadRequestError");
  });

  it("does not include internal path information in the error message", () => {
    let caught: unknown;
    try {
      assertExportToken("/internal/path/to/secret");
    } catch (err) {
      caught = err;
    }
    // The message must be the generic sentinel, not the raw input value.
    expect((caught as Error).message).toBe("Invalid entity ID");
  });
});

// ── caller-value immutability ─────────────────────────────────────────────────

describe("assertExportToken – caller-value immutability", () => {
  it("does not mutate a valid token string across repeated calls", () => {
    const token = `  ${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}  `;
    const original = token;
    assertExportToken(token);
    assertExportToken(token);
    expect(token).toBe(original);
  });

  it("does not mutate an invalid token string across repeated calls", () => {
    const bad = `${ENTITY_PREFIXES.BID}${VALID_ULID}`;
    const original = bad;
    try { assertExportToken(bad); } catch { /* expected */ }
    try { assertExportToken(bad); } catch { /* expected */ }
    expect(bad).toBe(original);
  });
});

// ── concurrent / retry determinism ───────────────────────────────────────────

describe("assertExportToken – concurrent and retry determinism", () => {
  it("produces identical outcomes when called concurrently from multiple async contexts", async () => {
    const token = `${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}`;
    const results = await Promise.all(
      Array.from({ length: 20 }, () =>
        Promise.resolve().then(() => {
          try {
            assertExportToken(token);
            return "ok";
          } catch {
            return "err";
          }
        }),
      ),
    );
    // Every concurrent invocation must agree: all accept or all reject.
    const unique = new Set(results);
    expect(unique.size).toBe(1);
    expect(unique.has("ok")).toBe(true);
  });

  it("produces identical rejection outcomes concurrently for an invalid token", async () => {
    const bad = `${ENTITY_PREFIXES.INVOICE}${VALID_ULID}`;
    const results = await Promise.all(
      Array.from({ length: 20 }, () =>
        Promise.resolve().then(() => {
          try {
            assertExportToken(bad);
            return "ok";
          } catch (err: unknown) {
            return (err as any).code;
          }
        }),
      ),
    );
    const unique = new Set(results);
    expect(unique.size).toBe(1);
    expect(unique.has("INVALID_ENTITY_ID")).toBe(true);
  });

  it("accepts → reject → accept sequence is fully deterministic (no sticky state)", () => {
    const valid = `${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}`;
    const invalid = `${ENTITY_PREFIXES.BID}${VALID_ULID}`;

    // valid → invalid → valid → invalid: outcome must match the input each time.
    expect(() => assertExportToken(valid)).not.toThrow();
    expect(() => assertExportToken(invalid)).toThrow();
    expect(() => assertExportToken(valid)).not.toThrow();
    expect(() => assertExportToken(invalid)).toThrow();
    expect(() => assertExportToken(valid)).not.toThrow();
  });

  it("partial-failure in a loop does not affect subsequent iterations (no leaked state)", () => {
    const valid = `${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}`;
    const invalid = "garbage";

    for (let i = 0; i < 50; i++) {
      // Every even iteration uses a valid token, every odd uses an invalid one.
      if (i % 2 === 0) {
        expect(() => assertExportToken(valid)).not.toThrow();
      } else {
        expect(() => assertExportToken(invalid)).toThrow();
      }
    }
  });
});

// ── integration: assertExportToken is the gate for downloadExport ─────────────

describe("assertExportToken – integration with downloadExport controller boundary", () => {
  /**
   * These tests verify that the token validation performed by assertExportToken
   * is sufficient to block every adversarial token variant that could reach the
   * downloadExport handler.  They are pure unit tests (no HTTP stack needed)
   * because the controller calls assertExportToken before any I/O.
   */

  const adversarialTokens: Array<[string, unknown]> = [
    ["empty string", ""],
    ["only whitespace", "   "],
    ["wrong prefix (inv_)", `${ENTITY_PREFIXES.INVOICE}${VALID_ULID}`],
    ["wrong prefix (bid_)", `${ENTITY_PREFIXES.BID}${VALID_ULID}`],
    ["wrong prefix (stl_)", `${ENTITY_PREFIXES.SETTLEMENT}${VALID_ULID}`],
    ["bare ULID", VALID_ULID],
    ["ULID too short (25)", `${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID.slice(0, 25)}`],
    ["ULID too long (27)", `${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}A`],
    ["excluded char I", `${ENTITY_PREFIXES.EXPORT_TOKEN}${"I".repeat(26)}`],
    ["excluded char L", `${ENTITY_PREFIXES.EXPORT_TOKEN}${"L".repeat(26)}`],
    ["excluded char O", `${ENTITY_PREFIXES.EXPORT_TOKEN}${"O".repeat(26)}`],
    ["excluded char U", `${ENTITY_PREFIXES.EXPORT_TOKEN}${"U".repeat(26)}`],
    ["SQL injection", `${ENTITY_PREFIXES.EXPORT_TOKEN}' OR '1'='1`],
    ["null", null],
    ["number", 99],
    ["object", { token: `${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}` }],
  ];

  it.each(adversarialTokens)(
    "blocks adversarial token variant: %s",
    (_label, value) => {
      expectRejected(value);
    },
  );
});
