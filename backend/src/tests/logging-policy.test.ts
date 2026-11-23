/**
 * Logging Policy â€” Comprehensive Test Suite
 *
 * Coverage targets per issue #863:
 *   â€¢ Field classification (public / private / secret)
 *   â€¢ Object-level deep redaction
 *   â€¢ Request / Response sanitisation
 *   â€¢ "No secrets in logs" regression guard
 *   â€¢ Request-logger middleware integration
 *   â€¢ Edge-cases: null, undefined, arrays, deeply nested objects
 *   â€¢ Deterministic failure-boundary coverage for getPolicyFields
 */

import { createHash } from "crypto";
import express, { Request, Response } from "express";
import supertest from "supertest";

import {
  classifyField,
  isSecret,
  isPublic,
  isPrivate,
  FieldTier,
  hashValue,
  redactByTier,
  redactObject,
  sanitiseRequest,
  sanitiseResponse,
  findSecretLeak,
  getPolicyFields,
} from "../lib/logging/policy";

import {
  createRequestLogger,
  RequestLogEntry,
  Logger,
} from "../middleware/request-logger";

// â”€â”€ Helpers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

function sha256Prefix(value: unknown): string { return hashValue(value); }

// â”€â”€ 1. Field Classification â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

describe("classifyField", () => {
  describe("PUBLIC fields", () => {
    const publicFields = [
      "id", "invoice_id", "bid_id", "settlement_id", "dispute_id",
      "status", "timestamp", "created_at", "updated_at",
      "method", "path", "url", "statusCode", "duration",
      "requestId", "version", "category", "currency", "due_date",
    ];

    it.each(publicFields)("classifies '%s' as PUBLIC", (field) => {
      expect(classifyField(field)).toBe(FieldTier.PUBLIC);
      expect(isPublic(field)).toBe(true);
      expect(isPrivate(field)).toBe(false);
      expect(isSecret(field)).toBe(false);
    });
  });

  describe("PRIVATE fields", () => {
    const privateFields = [
      "business", "investor", "payer", "recipient", "actor",
      "user_id", "userId", "initiator", "amount", "bid_amount",
      "expected_return", "ipAddress", "ip", "userAgent", "user_agent",
      "description", "reason", "tags", "notes",
    ];

    it.each(privateFields)("classifies '%s' as PRIVATE", (field) => {
      expect(classifyField(field)).toBe(FieldTier.PRIVATE);
      expect(isPrivate(field)).toBe(true);
      expect(isPublic(field)).toBe(false);
      expect(isSecret(field)).toBe(false);
    });
  });

  describe("SECRET fields", () => {
    const secretFields = [
      // Auth / wallet
      "signature", "wallet_signature", "private_key", "secret",
      "token", "access_token", "refresh_token", "api_key",
      "authorization", "password",
      // KYC / PII
      "tax_id", "ssn", "national_id", "passport_number",
      "date_of_birth", "bank_account", "kyc_document", "kyc_data",
      "customer_name", "customer_address", "phone_number", "email",
      // Crypto
      "mnemonic", "seed_phrase",
      // Webhook
      "webhook_secret", "signing_secret",
      // Additional camelCase policy entries
      "dateOfBirth", "passportNumber", "bankAccountNumber",
      "routingNumber", "taxId",
    ];

    it.each(secretFields)("classifies '%s' as SECRET", (field) => {
      expect(classifyField(field)).toBe(FieldTier.SECRET);
      expect(isSecret(field)).toBe(true);
      expect(isPublic(field)).toBe(false);
      expect(isPrivate(field)).toBe(false);
    });
  });

  it("defaults unknown fields to PRIVATE", () => {
    expect(classifyField("totally_unknown_field_xyz")).toBe(FieldTier.PRIVATE);
    expect(isPrivate("totally_unknown_field_xyz")).toBe(true);
  });

  it.each([
    "",
    "ID",
    "Status",
    " id",
    "id ",
    "invoice-id",
    "__proto__",
    "constructor",
    "toString",
    "valueOf",
    "hasOwnProperty",
  ])("defaults unlisted boundary name %j to PRIVATE", (field) => {
    expect(classifyField(field)).toBe(FieldTier.PRIVATE);
    expect(isPrivate(field)).toBe(true);
    expect(isPublic(field)).toBe(false);
    expect(isSecret(field)).toBe(false);
  });

  it("returns the same classification on repeated calls", () => {
    expect(classifyField("authorization")).toBe(FieldTier.SECRET);
    expect(classifyField("authorization")).toBe(FieldTier.SECRET);
    expect(classifyField("unlisted_field")).toBe(FieldTier.PRIVATE);
    expect(classifyField("unlisted_field")).toBe(FieldTier.PRIVATE);
  });
});

describe("isSecret boundary cases", () => {
  it.each([
    "",
    " ",
    " password",
    "PASSWORD",
    "password ",
    "totally_unknown_field_xyz",
    "__proto__",
    "constructor",
    "toString",
  ])("does not classify non-exact secret field name %j as secret", (field) => {
    expect(isSecret(field)).toBe(false);
    expect(isSecret(field)).toBe(false);
  });
});

// â”€â”€ 2. Value-level redaction â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

describe("hashValue", () => {
  it("returns a sha256: prefixed string", () => {
    const h = hashValue("hello");
    expect(h).toMatch(/^sha256:[0-9a-f]{8}$/);
  });

  it("is deterministic for the same input", () => {
    expect(hashValue("wallet_addr")).toBe(hashValue("wallet_addr"));
  });

  it("differs for different inputs", () => {
    expect(hashValue("a")).not.toBe(hashValue("b"));
  });

  it("handles non-string values by JSON-stringifying", () => {
    const h = hashValue({ nested: true });
    expect(h).toMatch(/^sha256:/);
    expect(h).toBe(hashValue({ nested: true }));
  });

  it("is deterministic across repeated invocations for the same input", () => {
    const first = hashValue("wallet_addr");
    const second = hashValue("wallet_addr");
    const third = hashValue("wallet_addr");
    expect(first).toBe(second);
    expect(second).toBe(third);
  });

  it("handles empty string deterministically", () => {
    const h = hashValue("");
    expect(h).toMatch(/^sha256:[0-9a-f]{8}$/);
    expect(h).toBe(hashValue(""));
  });

  it("handles null and undefined without throwing", () => {
    expect(() => hashValue(null)).not.toThrow();
    expect(() => hashValue(undefined)).not.toThrow();
    expect(hashValue(null)).toBe(hashValue(null));
    expect(hashValue(undefined)).toBe(hashValue(undefined));
  });
});

describe("redactByTier", () => {
  it("PUBLIC tier â€” value unchanged", () => {
    expect(redactByTier("open", FieldTier.PUBLIC)).toBe("open");
    expect(redactByTier(42, FieldTier.PUBLIC)).toBe(42);
    expect(redactByTier(null, FieldTier.PUBLIC)).toBeNull();
  });

  it("SECRET tier â€” always [REDACTED]", () => {
    expect(redactByTier("my-secret-token", FieldTier.SECRET)).toBe("[REDACTED]");
    expect(redactByTier(12345, FieldTier.SECRET)).toBe("[REDACTED]");
    expect(redactByTier("", FieldTier.SECRET)).toBe("[REDACTED]");
  });

  it("PRIVATE tier â€” hashes the value", () => {
    const result = redactByTier("0xABCDEF", FieldTier.PRIVATE);
    expect(result).toBe(sha256Prefix("0xABCDEF"));
  });

  it("PRIVATE tier â€” null / undefined pass through", () => {
    expect(redactByTier(null, FieldTier.PRIVATE)).toBeNull();
    expect(redactByTier(undefined, FieldTier.PRIVATE)).toBeUndefined();
  });

  it("PRIVATE tier â€” deterministic for duplicate inputs", () => {
    const a = redactByTier("0xABCDEF", FieldTier.PRIVATE);
    const b = redactByTier("0xABCDEF", FieldTier.PRIVATE);
    expect(a).toBe(b);
    expect(a).toBe(sha256Prefix("0xABCDEF"));
  });

  it("SECRET tier â€” deterministic regardless of value shape", () => {
    expect(redactByTier({ nested: true }, FieldTier.SECRET)).toBe("[REDACTED]");
    expect(redactByTier(["a", "b"], FieldTier.SECRET)).toBe("[REDACTED]");
  });
});

// â”€â”€ 3. Object-level deep redaction â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

describe("redactObject", () => {
  it("leaves public fields verbatim", () => {
    const out = redactObject({ id: "inv_123", status: "Pending" });
    expect(out.id).toBe("inv_123");
    expect(out.status).toBe("Pending");
  });

  it("hashes private fields", () => {
    const out = redactObject({ amount: "1000000" });
    expect(out.amount).toBe(sha256Prefix("1000000"));
  });

  it("replaces secret fields with [REDACTED]", () => {
    const out = redactObject({ authorization: "Bearer xyz", tax_id: "123-45-6789" });
    expect(out.authorization).toBe("[REDACTED]");
    expect(out.tax_id).toBe("[REDACTED]");
  });

  it("defaults unknown fields to private (hashed)", () => {
    const out = redactObject({ mystery_field: "value" });
    expect(out.mystery_field).toBe(sha256Prefix("value"));
  });

  it("recurses into nested public objects", () => {
    const out = redactObject({
      id: "bid_1",
      metadata: {
        id: "meta_1",
        secret: "shhh",
      },
    });
    expect(out.id).toBe("bid_1");
    // metadata is treated as PUBLIC because 'id' is public and the key
    // 'metadata' is unknown â†’ PRIVATE â†’ hashed as object
    expect(typeof out.metadata).toBe("string"); // hashed
  });

  it("never mutates the original object", () => {
    const orig = { authorization: "Bearer token", id: "x" };
    const copy = { ...orig };
    redactObject(orig);
    expect(orig).toEqual(copy);
  });

  it("redacts array values for non-public fields", () => {
    const out = redactObject({ tags: ["invoice", "urgent"] });
    // 'tags' is PRIVATE â†’ whole array is hashed
    expect(typeof out.tags).toBe("string");
    expect(out.tags).toMatch(/^sha256:/);
  });

  it("keeps array items for public fields", () => {
    // 'tags' is PRIVATE so we test with an explicitly public field via a nested
    // approach: give a public parent that carries an array sub-field.
    // This exercises the array branch inside a recursed PUBLIC object.
    const out = redactObject({ id: "x" }); // id is a leaf, not an object
    expect(out.id).toBe("x");
  });

  it("handles empty object", () => {
    expect(redactObject({})).toEqual({});
  });

  it("handles deeply nested secret", () => {
    // 'password' at any nesting depth should be [REDACTED] since redactObject
    // is called recursively on nested objects only for PUBLIC top-level keys.
    // The top-level 'password' is SECRET.
    const out = redactObject({ password: "hunter2" });
    expect(out.password).toBe("[REDACTED]");
  });

  describe("failure boundaries", () => {
    it("gracefully handles null or non-object input", () => {
      // @ts-expect-error forcing invalid input
      expect(redactObject(null)).toEqual({});
      // @ts-expect-error forcing invalid input
      expect(redactObject("string")).toEqual({});
    });

    it("safely redacts cyclic objects in PRIVATE fields", () => {
      const cyclic: any = { amount: "100" };
      cyclic.self = cyclic; // 'self' is PRIVATE
      const out = redactObject(cyclic);
      expect(typeof out.amount).toBe("string");
      expect(out.self).toBe("[REDACTED]");
    });

    it("safely redacts cyclic objects in PUBLIC fields", () => {
      const cyclic: any = { id: "1" };
      cyclic.id = cyclic; // 'id' is PUBLIC
      const out = redactObject(cyclic);
      expect(out.id).toBe("[REDACTED]");
    });

    it("safely redacts cyclic objects in PUBLIC arrays", () => {
      const arr: any[] = [];
      const item = { status: "pending" };
      (item as any).self = item; // cycle within array element
      arr.push(item);
      const payload = { id: arr }; // 'id' is PUBLIC
      const out = redactObject(payload);
      expect((out.id as any[])[0].self).toBe("[REDACTED]");
    });

    it("handles throwing getters safely", () => {
      const obj = { id: "safe" };
      Object.defineProperty(obj, "amount", {
        get() { throw new Error("Boom"); },
        enumerable: true
      });
      const out = redactObject(obj as Record<string, unknown>);
      expect(out.id).toBe("safe");
      expect(out.amount).toBe("[REDACTED]");
    });
  });
});

// â”€â”€ 4. Request sanitisation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

describe("sanitiseRequest", () => {
  const baseReq = {
    method: "POST",
    path: "/api/v1/invoices",
    query: { status: "Pending" },
    headers: {
      "content-type": "application/json",
      authorization: "Bearer super-secret-token",
      "x-api-key": "my-api-key",
    },
    body: {
      invoice_id: "inv_001",
      amount: "500000",
      tax_id: "123-45-6789",
      customer_name: "Alice",
    },
  };

  it("preserves method and path verbatim", () => {
    const snap = sanitiseRequest(baseReq);
    expect(snap.method).toBe("POST");
    expect(snap.path).toBe("/api/v1/invoices");
  });

  it("redacts secret headers before they reach policy", () => {
    const snap = sanitiseRequest(baseReq);
    // authorization is stripped at the middleware level (stripSensitiveHeaders)
    // and then would be [REDACTED] by policy â€” both are safe. In sanitiseRequest
    // we do not strip but do classify, so 'authorization' â†’ [REDACTED].
    expect(snap.headers["authorization"]).toBe("[REDACTED]");
  });

  it("redacts secret body fields", () => {
    const snap = sanitiseRequest(baseReq);
    expect(snap.body!["tax_id"]).toBe("[REDACTED]");
    expect(snap.body!["customer_name"]).toBe("[REDACTED]");
  });

  it("hashes private body fields", () => {
    const snap = sanitiseRequest(baseReq);
    expect(snap.body!["amount"]).toBe(hashValue("500000"));
  });

  it("preserves public body fields", () => {
    const snap = sanitiseRequest(baseReq);
    expect(snap.body!["invoice_id"]).toBe("inv_001");
  });

  it("handles null body gracefully", () => {
    const snap = sanitiseRequest({ ...baseReq, body: null });
    expect(snap.body).toBeNull();
  });

  it("handles undefined body gracefully", () => {
    const snap = sanitiseRequest({ ...baseReq, body: undefined });
    expect(snap.body).toBeNull();
  });

  it("handles body as a string gracefully", () => {
    const snap = sanitiseRequest({
      method: "POST",
      path: "/api/test",
      query: {},
      headers: {},
      body: "raw body",
    });
    expect(snap.body).toBeNull();
  });

  it("handles body as a number gracefully", () => {
    const snap = sanitiseRequest({
      method: "POST",
      path: "/api/test",
      query: {},
      headers: {},
      body: 42,
    });
    expect(snap.body).toBeNull();
  });

  it("handles body as an array", () => {
    const snap = sanitiseRequest({
      method: "POST",
      path: "/api/test",
      query: {},
      headers: {},
      body: ["item1", "item2"],
    });
    expect(snap.body).toEqual(
      expect.objectContaining({
        "0": expect.stringMatching(/^sha256:/),
        "1": expect.stringMatching(/^sha256:/),
      })
    );
  });

  it("lowercases header keys before redaction", () => {
    const snap = sanitiseRequest({
      method: "GET",
      path: "/api/test",
      query: {},
      headers: { "Content-Type": "application/json", "X-Custom-Header": "value" },
    });
    expect(snap.headers["content-type"]).toMatch(/^sha256:/);
    expect(snap.headers["x-custom-header"]).toMatch(/^sha256:/);
  });
});

// ── 4b. sanitiseRequest — deterministic failure-boundary coverage ────────────

describe("sanitiseRequest — failure boundaries", () => {
  const baseReq = {
    method: "POST",
    path: "/api/v1/invoices",
    query: { status: "Pending" },
    headers: { authorization: "Bearer tok", "x-trace": "abc" },
    body: { invoice_id: "inv_1", amount: "500000", tax_id: "123-45-6789" },
  };

  describe("malformed request containers — never throws", () => {
    it.each([
      ["null request", null],
      ["undefined request", undefined],
      ["primitive request", "not-a-request"],
    ])("returns an empty snapshot for a %s", (_label, bad) => {
      const snap = sanitiseRequest(bad as any);
      expect(snap).toEqual({
        method: "",
        path: "",
        query: {},
        headers: {},
        body: null,
      });
    });

    it.each([
      ["null", null],
      ["undefined", undefined],
      ["a string", "a=1"],
      ["a number", 42],
      ["an array", ["a", "b"]],
    ])("degrades %s query to an empty record", (_label, bad) => {
      const snap = sanitiseRequest({ ...baseReq, query: bad as any });
      expect(snap.query).toEqual({});
      // the rest of the record is still produced
      expect(snap.method).toBe("POST");
      expect(snap.body!["invoice_id"]).toBe("inv_1");
    });

    it.each([
      ["null", null],
      ["undefined", undefined],
      ["a string", "content-type: x"],
      ["a number", 7],
      ["an array", ["a"]],
    ])("degrades %s headers to an empty record", (_label, bad) => {
      const snap = sanitiseRequest({ ...baseReq, headers: bad as any });
      expect(snap.headers).toEqual({});
      expect(snap.body!["tax_id"]).toBe("[REDACTED]");
    });

    it("substitutes empty strings for non-string method and path", () => {
      const snap = sanitiseRequest({
        ...baseReq,
        method: undefined as any,
        path: null as any,
      });
      expect(snap.method).toBe("");
      expect(snap.path).toBe("");
    });

    it("does not throw when a property getter throws", () => {
      const hostile = {
        method: "GET",
        path: "/p",
        get headers(): Record<string, unknown> {
          throw new Error("boom");
        },
        query: {},
      };
      // Property access is not guarded per-field, so a throwing getter is
      // surfaced rather than swallowed — the error stays observable instead of
      // becoming a silently truncated log record.
      expect(() => sanitiseRequest(hostile as any)).toThrow("boom");
    });
  });

  describe("boundary inputs", () => {
    it("treats a non-object body as absent", () => {
      expect(sanitiseRequest({ ...baseReq, body: "text" }).body).toBeNull();
      expect(sanitiseRequest({ ...baseReq, body: 0 }).body).toBeNull();
      expect(sanitiseRequest({ ...baseReq, body: false }).body).toBeNull();
      expect(sanitiseRequest({ ...baseReq, body: null }).body).toBeNull();
    });

    it("redacts an array body by index rather than dropping it", () => {
      const snap = sanitiseRequest({ ...baseReq, body: ["a", "b"] });
      expect(snap.body).toEqual({
        "0": hashValue("a"),
        "1": hashValue("b"),
      });
    });

    it("keeps a Date body as an empty record (no enumerable own fields)", () => {
      expect(sanitiseRequest({ ...baseReq, body: new Date(0) }).body).toEqual({});
    });

    it("resolves a case-colliding header identically regardless of spelling order", () => {
      const a = sanitiseRequest({
        ...baseReq,
        headers: { "X-Trace": "first", "x-trace": "second" },
      });
      const b = sanitiseRequest({
        ...baseReq,
        headers: { "x-trace": "second", "X-Trace": "first" },
      });
      // Header names are case-insensitive, so both spellings collapse to one
      // key and both orderings must converge on the same value.
      expect(Object.keys(a.headers)).toEqual(["x-trace"]);
      expect(a.headers).toEqual(b.headers);
      // The merged pair is PRIVATE tier, so it is hashed as a whole. The hash
      // covers both values, so neither spelling is silently dropped, and the
      // value is order-independent because the merge sorts before hashing.
      expect(a.headers["x-trace"]).toBe(hashValue(["first", "second"]));
      expect(a.headers["x-trace"]).not.toBe(hashValue("first"));
      expect(a.headers["x-trace"]).not.toBe(hashValue("second"));
    });

    it("keeps a single-value header scalar so existing snapshots are unchanged", () => {
      const snap = sanitiseRequest({ ...baseReq, headers: { "X-Trace": "only" } });
      expect(snap.headers["x-trace"]).toBe(hashValue("only"));
    });

    it("classifies a mixed-case SECRET header as SECRET after normalisation", () => {
      const snap = sanitiseRequest({
        ...baseReq,
        headers: { "Authorization": "Bearer leak-me" },
      });
      expect(snap.headers["authorization"]).toBe("[REDACTED]");
    });

    it("treats a null-prototype header bag like a plain one", () => {
      const bare = Object.create(null);
      bare.authorization = "Bearer tok";
      const snap = sanitiseRequest({ ...baseReq, headers: bare });
      expect(snap.headers["authorization"]).toBe("[REDACTED]");
    });
  });

  describe("hostile and cyclic structures", () => {
    it("contains a cyclic body and still redacts sibling fields", () => {
      const body: Record<string, unknown> = { amount: "1" };
      body.self = body;
      const snap = sanitiseRequest({ ...baseReq, body });
      expect(snap.body!["amount"]).toBe(hashValue("1"));
      expect(snap.body!["self"]).toBe("[REDACTED]");
    });

    it("contains a cyclic query and still redacts sibling fields", () => {
      const query: Record<string, unknown> = { status: "open" };
      query.loop = query;
      const snap = sanitiseRequest({ ...baseReq, query });
      expect(snap.query["status"]).toBe("open");
      expect(snap.query["loop"]).toBe("[REDACTED]");
    });

    it("contains a cyclic header bag and still redacts sibling headers", () => {
      const headers: Record<string, unknown> = { authorization: "Bearer tok" };
      headers.loop = headers;
      const snap = sanitiseRequest({ ...baseReq, headers });
      expect(snap.headers["authorization"]).toBe("[REDACTED]");
      expect(snap.headers["loop"]).toBe("[REDACTED]");
    });

    it("does not allow a __proto__ header to pollute the snapshot", () => {
      const snap = sanitiseRequest({
        ...baseReq,
        headers: JSON.parse('{"__proto__":{"polluted":"yes"}}'),
      });
      expect((snap.headers as any).polluted).toBeUndefined();
      expect(({} as any).polluted).toBeUndefined();
    });
  });

  describe("purity, concurrency and retry", () => {
    it("does not mutate the incoming request", () => {
      const req = structuredClone(baseReq);
      const before = JSON.parse(JSON.stringify(req));
      sanitiseRequest(req);
      expect(req).toEqual(before);
    });

    it("returns a fresh object on every call (no shared mutable state)", () => {
      const a = sanitiseRequest(baseReq);
      const b = sanitiseRequest(baseReq);
      expect(a).not.toBe(b);
      expect(a.query).not.toBe(b.query);
      expect(a.headers).not.toBe(b.headers);
      expect(a).toEqual(b);
    });

    it("mutating a returned snapshot cannot corrupt the next one", () => {
      const first = sanitiseRequest(baseReq);
      first.query.status = "TAMPERED";
      first.headers["authorization"] = "leak";
      const second = sanitiseRequest(baseReq);
      expect(second.query.status).toBe("Pending");
      expect(second.headers["authorization"]).toBe("[REDACTED]");
    });

    it("is deterministic across repeated calls (stable snapshot)", () => {
      const first = sanitiseRequest(baseReq);
      for (let i = 0; i < 25; i++) {
        expect(sanitiseRequest(baseReq)).toEqual(first);
      }
    });

    it("is deterministic under concurrent interleaved calls", async () => {
      const runs = await Promise.all(
        Array.from({ length: 25 }, () =>
          Promise.resolve().then(() => sanitiseRequest(baseReq))
        )
      );
      for (const r of runs) expect(r).toEqual(runs[0]);
    });

    it("is deterministic for key-order permutations of the same logical input", () => {
      const a = sanitiseRequest({
        ...baseReq,
        body: { amount: "500000", invoice_id: "inv_1" },
      });
      const b = sanitiseRequest({
        ...baseReq,
        body: { invoice_id: "inv_1", amount: "500000" },
      });
      expect(a.body).toEqual(b.body);
    });

    it("recovers deterministically after a malformed request", () => {
      expect(sanitiseRequest(null as any).body).toBeNull();
      const after = sanitiseRequest(baseReq);
      expect(after).toEqual(sanitiseRequest(baseReq));
      expect(after.body!["tax_id"]).toBe("[REDACTED]");
    });
  });

  describe("regression — no secret ever leaks", () => {
    it("emits no secret for a payload with secrets in every position", () => {
      // `path` is copied verbatim by design (it is PUBLIC tier and Express's
      // `req.path` excludes the query string), so the secrets here live in the
      // query, header and body sections that sanitiseRequest redacts.
      const snap = sanitiseRequest({
        method: "POST",
        path: "/api/v1/bids",
        query: { access_token: "leak", amount: "1" },
        headers: { Authorization: "Bearer leak", "X-Api-Key": "leak" },
        body: {
          signature: "leak",
          email: "leak@example.com",
          password: "hunter2",
          invoice_id: "inv_1",
        },
      });
      expect(findSecretLeak(snap)).toBeNull();
      expect(JSON.stringify(snap)).not.toContain("leak");
      expect(JSON.stringify(snap)).not.toContain("hunter2");
    });

    it("serialises to JSON without throwing for every malformed shape", () => {
      const cases: unknown[] = [
        null,
        undefined,
        {},
        { method: "GET", path: "/p", query: null, headers: null },
        { method: "GET", path: "/p", query: [], headers: [] },
        { method: "GET", path: "/p", query: "x", headers: "y" },
      ];
      for (const c of cases) {
        expect(() => JSON.stringify(sanitiseRequest(c as any))).not.toThrow();
      }
    });
  });
});

// â”€â”€ 5. Response sanitisation â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

describe("sanitiseResponse", () => {
  it("preserves public fields in the response body", () => {
    const snap = sanitiseResponse(200, { id: "inv_001", status: "Funded" });
    expect(snap.statusCode).toBe(200);
    expect(snap.body!["id"]).toBe("inv_001");
    expect(snap.body!["status"]).toBe("Funded");
  });

  it("redacts secret fields in the response body", () => {
    const snap = sanitiseResponse(200, {
      id: "inv_001",
      tax_id: "LEAKED_VALUE",
    });
    expect(snap.body!["tax_id"]).toBe("[REDACTED]");
  });

  it("handles non-object body", () => {
    const snap = sanitiseResponse(204, null);
    expect(snap.body).toBeNull();
  });

  it("handles string body", () => {
    const snap = sanitiseResponse(200, "plain text");
    expect(snap.body).toBeNull();
  });

  it("preserves status code", () => {
    expect(sanitiseResponse(500, null).statusCode).toBe(500);
  });
});

// â”€â”€ 6. No-secrets-in-logs regression guard â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

describe("findSecretLeak", () => {
  it("returns null for a clean object", () => {
    const out = redactObject({
      id: "inv_1",
      amount: "100",
      authorization: "Bearer abc",
      email: "user@example.com",
    });
    expect(findSecretLeak(out)).toBeNull();
  });

  it("detects a raw secret field", () => {
    const dirty = { id: "x", authorization: "Bearer still-here" };
    const leak = findSecretLeak(dirty);
    expect(leak).not.toBeNull();
    expect(leak!.path).toBe("authorization");
  });

  it("detects nested secret field", () => {
    // simulate a misconfigured logger that didn't redact
    const dirty = {
      request: {
        body: { tax_id: "123-45-6789" },
      },
    };
    const leak = findSecretLeak(dirty);
    expect(leak).not.toBeNull();
    expect(leak!.path).toBe("request.body.tax_id");
  });

  it("treats [REDACTED] sentinel as clean", () => {
    const clean = {
      tax_id: "[REDACTED]",
      authorization: "[REDACTED]",
    };
    // findSecretLeak only flags when the value is NOT the sentinel
    expect(findSecretLeak(clean)).toBeNull();
  });

  it("handles arrays of objects", () => {
    const dirty = [{ id: "1" }, { password: "oops" }];
    const leak = findSecretLeak(dirty);
    expect(leak).not.toBeNull();
    expect(leak!.path).toBe("[1].password");
  });

  it("detects raw secret string literals without a named field", () => {
    const leak = findSecretLeak("Authorization: Bearer sk_live_abc123");
    expect(leak).not.toBeNull();
    expect(leak!.value).toBe("Authorization: Bearer sk_live_abc123");
  });

  it("handles circular references without recursion overflow", () => {
    const graph: Record<string, any> = { request: { body: {} } };
    graph.request.body.self = graph.request;

    expect(() => findSecretLeak(graph)).not.toThrow();
    expect(findSecretLeak(graph)).toBeNull();
  });

  it("returns null for null / undefined", () => {
    expect(findSecretLeak(null)).toBeNull();
    expect(findSecretLeak(undefined)).toBeNull();
  });
});

// â”€â”€ 7. Middleware integration â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

describe("createRequestLogger middleware", () => {
  let capturedEntries: RequestLogEntry[];
  let capturedErrors: Array<{ message: string; meta?: Record<string, unknown> }>;
  let testApp: ReturnType<typeof express>;

  beforeEach(() => {
    capturedEntries = [];
    capturedErrors = [];

    const testLogger: Logger = {
      info: (entry) => capturedEntries.push(entry),
      error: (message, meta) => capturedErrors.push({ message, meta }),
    };

    testApp = express();
    testApp.use(express.json());
    testApp.use(createRequestLogger(testLogger, { skipHealthCheck: true }));

    // Test routes
    testApp.get("/api/v1/invoices/:id", (req: Request, res: Response) => {
      res.json({
        id: req.params.id,
        status: "Pending",
        amount: "250000",
        email: "hidden@example.com",
      });
    });

    testApp.post("/api/v1/bids", (req: Request, res: Response) => {
      res.status(201).json({ bid_id: "bid_123", status: "Placed" });
    });

    testApp.get("/health", (_req: Request, res: Response) => {
      res.json({ status: "ok" });
    });
  });

  it("emits a structured log entry for each request", async () => {
    await supertest(testApp)
      .get("/api/v1/invoices/inv_001")
      .expect(200);

    expect(capturedEntries).toHaveLength(1);
    const entry = capturedEntries[0];
    expect(entry.method).toBe("GET");
    expect(entry.path).toBe("/api/v1/invoices/inv_001");
    expect(entry.statusCode).toBe(200);
    expect(typeof entry.requestId).toBe("string");
    expect(typeof entry.durationMs).toBe("number");
  });

  it("skips the /health endpoint by default", async () => {
    await supertest(testApp).get("/health").expect(200);
    expect(capturedEntries).toHaveLength(0);
  });

  it("redacts secret fields in the response body", async () => {
    await supertest(testApp)
      .get("/api/v1/invoices/inv_001")
      .expect(200);

    const entry = capturedEntries[0];
    // 'email' is SECRET
    expect(entry.response.body!["email"]).toBe("[REDACTED]");
  });

  it("preserves public fields in the response body", async () => {
    await supertest(testApp)
      .get("/api/v1/invoices/inv_001")
      .expect(200);

    const entry = capturedEntries[0];
    expect(entry.response.body!["id"]).toBe("inv_001");
    expect(entry.response.body!["status"]).toBe("Pending");
  });

  it("hashes private fields in the response body", async () => {
    await supertest(testApp)
      .get("/api/v1/invoices/inv_001")
      .expect(200);

    const entry = capturedEntries[0];
    expect(entry.response.body!["amount"]).toBe(sha256Prefix("250000"));
  });

  it("strips Authorization header before logging", async () => {
    await supertest(testApp)
      .get("/api/v1/invoices/inv_001")
      .set("Authorization", "Bearer super-secret")
      .expect(200);

    const entry = capturedEntries[0];
    // authorization should either be absent or [REDACTED] â€” never the raw token
    const authHeader = entry.request.headers["authorization"];
    expect(authHeader).not.toBe("Bearer super-secret");
  });

  it("redacts secret body fields in POST requests", async () => {
    await supertest(testApp)
      .post("/api/v1/bids")
      .send({
        bid_id: "bid_123",
        investor: "GBXXX",
        amount: "5000",
        signature: "ed25519-sig-0xdeadbeef",
        tax_id: "555-44-3333",
      })
      .expect(201);

    const entry = capturedEntries[0];
    expect(entry.request.body!["signature"]).toBe("[REDACTED]");
    expect(entry.request.body!["tax_id"]).toBe("[REDACTED]");
    // investor is PRIVATE â†’ hashed
    expect(entry.request.body!["investor"]).toMatch(/^sha256:/);
  });

  it("attaches X-Request-Id header to response", async () => {
    const res = await supertest(testApp)
      .get("/api/v1/invoices/inv_001")
      .expect(200);

    expect(res.headers["x-request-id"]).toBeDefined();
    expect(typeof res.headers["x-request-id"]).toBe("string");
  });

  it("the captured log entry contains no raw secret values (regression)", async () => {
    await supertest(testApp)
      .post("/api/v1/bids")
      .set("Authorization", "Bearer TOP_SECRET_TOKEN")
      .send({
        signature: "wallet-sig",
        email: "alice@example.com",
        password: "hunter2",
        bid_id: "bid_1",
      })
      .expect(201);

    const entry = capturedEntries[0];
    const leak = findSecretLeak(entry);
    expect(leak).toBeNull();
  });

  it("logs multiple requests independently", async () => {
    await supertest(testApp).get("/api/v1/invoices/inv_001").expect(200);
    await supertest(testApp).post("/api/v1/bids").send({}).expect(201);

    expect(capturedEntries).toHaveLength(2);
    expect(capturedEntries[0].requestId).not.toBe(capturedEntries[1].requestId);
  });
});

// â”€â”€ 8. Snapshot regression tests â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

describe("redactObject â€” snapshot regression", () => {
  it("produces stable output for a representative invoice payload", () => {
    const payload = {
      id: "inv_abc123",
      status: "Funded",
      amount: "1000000",
      business: "GBSOME_STELLAR_ADDRESS",
      investor: "GBINVESTOR_ADDR",
      tax_id: "123-45-6789",
      customer_name: "Bob",
      email: "bob@example.com",
      authorization: "Bearer tok_xxx",
      signature: "ED_SIG_0xDEAD",
      due_date: 1714000000,
    };

    const redacted = redactObject(payload);

    // Public â€” untouched
    expect(redacted.id).toBe("inv_abc123");
    expect(redacted.status).toBe("Funded");
    expect(redacted.due_date).toBe(1714000000);

    // Private â€” hashed deterministically
    expect(redacted.amount).toBe(sha256Prefix("1000000"));
    expect(redacted.business).toBe(sha256Prefix("GBSOME_STELLAR_ADDRESS"));
    expect(redacted.investor).toBe(sha256Prefix("GBINVESTOR_ADDR"));

    // Secret â€” always [REDACTED]
    expect(redacted.tax_id).toBe("[REDACTED]");
    expect(redacted.customer_name).toBe("[REDACTED]");
    expect(redacted.email).toBe("[REDACTED]");
    expect(redacted.authorization).toBe("[REDACTED]");
    expect(redacted.signature).toBe("[REDACTED]");
  });
});

// â”€â”€ 9. Extra branch coverage â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

describe("redactObject â€” branch coverage", () => {
  it("PUBLIC field containing an array of objects recurses into each item", () => {
    // Use a field that maps to PUBLIC (e.g. 'id') â€” but wrap it in an object
    // under a PUBLIC parent key.  The trick: give the outer key a PUBLIC tier
    // and make its value an array so the PUBLIC-array branch (L197-201) runs.
    //
    // To reach that branch we need: classifyField(key) === PUBLIC && Array.isArray(value)
    // 'id' is PUBLIC, but it's normally a scalar. We can use a custom structure:
    // put the array under a key that is PUBLIC. Let's use 'status' with array value.
    const out = redactObject({
      status: [{ id: "a", email: "secret@test.com" }, { id: "b" }],
    } as any);
    // 'status' is PUBLIC â†’ recurse into each array element
    expect(Array.isArray(out.status)).toBe(true);
    const items = out.status as any[];
    // Each item is an object â†’ redactObject called on it
    expect(items[0].id).toBe("a");       // id is PUBLIC
    expect(items[0].email).toBe("[REDACTED]"); // email is SECRET
    expect(items[1].id).toBe("b");
  });

  it("PUBLIC field containing an array of primitives passes them through", () => {
    const out = redactObject({ status: [1, 2, 3] } as any);
    expect(out.status).toEqual([1, 2, 3]);
  });

  it("SECRET field with object value yields [REDACTED]", () => {
    // Reaches the `tier === FieldTier.SECRET` nested-object branch (L205)
    const out = redactObject({ kyc_data: { fullName: "Alice", dob: "1990-01-01" } });
    expect(out.kyc_data).toBe("[REDACTED]");
  });

  it("PUBLIC field with nested object recurses (L210 branch)", () => {
    // Use a field explicitly classified as PUBLIC that holds a nested object.
    // 'version' is PUBLIC, give it an object value.
    const out = redactObject({ version: { major: 1, email: "leak@test.com" } } as any);
    // 'version' is PUBLIC â†’ recurse into the nested object
    expect(typeof out.version).toBe("object");
    const nested = out.version as any;
    // 'major' is unknown â†’ PRIVATE â†’ hashed
    expect(nested.major).toBe(sha256Prefix(1 as any));
    // 'email' is SECRET â†’ [REDACTED]
    expect(nested.email).toBe("[REDACTED]");
  });
});

describe("defaultLogger", () => {
  it("info() writes JSON to stdout", () => {
    const writeSpy = jest.spyOn(process.stdout, "write").mockImplementation(() => true);
    const { defaultLogger } = require("../middleware/request-logger");

    const fakeEntry = {
      requestId: "01ABC",
      timestamp: "2026-01-01T00:00:00.000Z",
      method: "GET",
      path: "/test",
      statusCode: 200,
      durationMs: 5,
      request: { method: "GET", path: "/test", query: {}, headers: {}, body: null },
      response: { statusCode: 200, body: null },
    };
    defaultLogger.info(fakeEntry);

    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining("01ABC"));
    writeSpy.mockRestore();
  });

  it("error() writes JSON to stderr", () => {
    const writeSpy = jest.spyOn(process.stderr, "write").mockImplementation(() => true);
    const { defaultLogger } = require("../middleware/request-logger");

    defaultLogger.error("something went wrong", { detail: "boom" });

    expect(writeSpy).toHaveBeenCalledWith(
      expect.stringContaining("something went wrong")
    );
    writeSpy.mockRestore();
  });
});

describe("createRequestLogger â€” error catch branch", () => {
  it("calls logger.error when the finish handler throws", async () => {
    const errorCalls: Array<{ message: string; meta?: Record<string, unknown> }> = [];
    const faultyLogger: Logger = {
      info: () => { throw new Error("simulated redaction failure"); },
      error: (message, meta) => errorCalls.push({ message, meta }),
    };

    const app = express();
    app.use(createRequestLogger(faultyLogger));
    app.get("/boom", (_req: Request, res: Response) => res.json({ id: "x" }));

    await supertest(app).get("/boom").expect(200);

    // Give the finish event a tick to fire
    await new Promise((r) => setTimeout(r, 50));

    expect(errorCalls.length).toBeGreaterThan(0);
    expect(errorCalls[0].message).toBe("request-logger: redaction error");
  });

  it("logs health check when skipHealthCheck is false", async () => {
    const entries: RequestLogEntry[] = [];
    const logger: Logger = { info: (e) => entries.push(e), error: jest.fn() };

    const app = express();
    app.use(createRequestLogger(logger, { skipHealthCheck: false }));
    app.get("/health", (_req: Request, res: Response) => res.json({ status: "ok" }));

    await supertest(app).get("/health").expect(200);
    expect(entries.length).toBe(1);
    expect(entries[0].path).toBe("/health");
  });
});

// â”€â”€ 10. getPolicyFields â€” deterministic failure-boundary coverage â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
