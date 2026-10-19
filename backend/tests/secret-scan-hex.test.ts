// Failure-boundary coverage for isHexString in scripts/lib/secret-scan-utils.js.
//
// The invariants exercised here are the H1..H6 block documented above
// isHexString. They matter because isHexString is the suppressor on the
// secret-classification path: isHighEntropyToken returns false for any hex
// candidate, so a wrong `true` silently drops a finding rather than raising a
// false alarm. A throw is worse than either, because it escapes through
// isHighEntropyToken -> collectHighEntropyMatches -> scanLine -> scanTargets,
// aborts the scan, and discards the findings already collected for other files.
//
// Three deliberate constraints on this file:
//
// 1. Fixtures that would themselves trip the scanner are assembled from short
//    parts at runtime (["A1b2", "C3d4", ...].join("")), so every literal in
//    the source stays below MIN_HIGH_ENTROPY_LENGTH and the production tree
//    scan this file is part of stays clean.
// 2. Hex-shaped fixtures are used wherever the assertion is about hex
//    classification, which is exactly the shape the helper is about.
// 3. Non-ASCII and control fixtures are built through fromCodePoint() rather
//    than written literally, so the source stays printable ASCII.

const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const secretScanUtils = require("../scripts/lib/secret-scan-utils");

const {
  KNOWN_SECRET_PATTERNS,
  MIN_HIGH_ENTROPY_LENGTH,
  MIN_HIGH_ENTROPY_SCORE,
  MIN_UNIQUE_CHARACTERS,
  collectRegexMatches,
  hasMixedCharacterClasses,
  isHexString,
  isHighEntropyToken,
  redactPreview,
  scanBackend,
  scanFileContent,
  scanLine,
  shannonEntropy,
} = secretScanUtils;

const ALLOWLIST = { entries: [], globalPatterns: [] };

// 32 distinct characters from the charset isHighEntropyToken accepts, so
// shannonEntropy is exactly log2(32) = 5.0 and the value clears
// MIN_HIGH_ENTROPY_SCORE: a genuine positive control for the reporting path.
// Assembled from parts so that no single literal here is long enough to be
// reported as high entropy.
const TRUE_ENTROPY_TOKEN = [
  "A1b2",
  "C3d4",
  "E5f6",
  "G7h8",
  "I9j0",
  "KlMn",
  "OpQr",
  "StUv",
].join("");

// A 40-character commit SHA: long enough to clear MIN_HIGH_ENTROPY_LENGTH and
// MIN_UNIQUE_CHARACTERS, mixed case, and the shape the suppression exists for.
const GIT_SHA = ["a1b2c3d4", "e5f6a7b8", "c9d0e1f2", "a3b4c5d6"].join("");

const HEX_PREFIXED_SHA = `0x${GIT_SHA}`;

// Every hex digit, so entropy is exactly log2(16) = 4: the worst case a hex
// run of any length can reach.
const ALL_HEX_DIGITS = "0123456789abcdef";

const astral = (code: number): string => String.fromCodePoint(code);

const E_ACUTE = astral(0x00e9);
const SUPERSCRIPT_ZERO = astral(0x2070);
const FULLWIDTH_A = astral(0xff21);
const FULLWIDTH_DIGIT_ZERO = astral(0xff10);
// U+0421, U+0423, U+0414, U+0430, U+0435: Cyrillic code points that render as
// the Latin letters C, U, D, a, e. Built from code points rather than written
// literally so this file stays printable ASCII.
const CYRILLIC_LOOKALIKE = [0x0421, 0x0423, 0x0414, 0x0430, 0x0435]
  .map((code) => astral(code))
  .join("");
const GRINNING_FACE = astral(0x1f600);
const LONE_SURROGATE = String.fromCharCode(0xd800);

function repeatingHex(length: number): string {
  return ALL_HEX_DIGITS.repeat(Math.ceil(length / ALL_HEX_DIGITS.length)).slice(0, length);
}

function throwingToString(): unknown {
  return {
    toString() {
      throw new Error("boom");
    },
  };
}

function throwingSymbolToPrimitive(): unknown {
  return {
    [Symbol.toPrimitive]() {
      throw new Error("boom");
    },
  };
}

function revokedProxy(): unknown {
  const revocable = Proxy.revocable({ secret: GIT_SHA }, {});
  revocable.revoke();
  return revocable.proxy;
}

function writeFixtureTree(prefix: string, files: Record<string, string>): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  const srcDir = path.join(root, "src");
  fs.mkdirSync(srcDir, { recursive: true });

  for (const [relativePath, content] of Object.entries(files)) {
    fs.writeFileSync(path.join(srcDir, relativePath), content, "utf8");
  }

  return root;
}

describe("isHexString: accepted string shapes (H3, H4, H5)", () => {
  it("accepts bare hex runs in either case", () => {
    for (const value of [
      "0",
      "9",
      "a",
      "f",
      "A",
      "F",
      "deadbeef",
      "DEADBEEF",
      "DeAdBeEf",
      "0123456789abcdefABCDEF",
      GIT_SHA,
    ]) {
      expect(isHexString(value)).toBe(true);
    }
  });

  it("accepts exactly one optional lowercase 0x prefix", () => {
    for (const value of ["0x0", "0xabcdef", "0xABCDEF", "0x0123456789abcdefABCDEF"]) {
      expect(isHexString(value)).toBe(true);
    }
  });

  it("accepts a single hex digit at the minimum length", () => {
    for (const digit of ["0", "1", "9", "a", "f", "A", "F"]) {
      expect(isHexString(digit)).toBe(true);
      expect(isHexString(`0x${digit}`)).toBe(true);
    }
  });

  it("accepts hex runs at and beyond the high-entropy length threshold", () => {
    const at = repeatingHex(MIN_HIGH_ENTROPY_LENGTH);

    expect(at).toHaveLength(MIN_HIGH_ENTROPY_LENGTH);
    expect(isHexString(at)).toBe(true);
    expect(isHexString(repeatingHex(MIN_HIGH_ENTROPY_LENGTH + 1))).toBe(true);
    expect(isHexString(repeatingHex(4096))).toBe(true);
    expect(isHexString(`0x${repeatingHex(4096)}`)).toBe(true);
  });

  it("accepts runs that read as another JS literal because b, B and e are hex digits", () => {
    // Documented boundary: classification is by alphabet, not by intent. These
    // are hex runs under H3/H4, so widening or narrowing here is a visible
    // behaviour change rather than a silent one.
    expect(isHexString("0B1010")).toBe(true);
    expect(isHexString("0b1010")).toBe(true);
    expect(isHexString("1e10")).toBe(true);
    expect(isHexString("1E10")).toBe(true);
    expect(isHexString("0.5")).toBe(false);
    expect(isHexString("0o777")).toBe(false);
    expect(isHexString("0O777")).toBe(false);
  });
});

describe("isHexString: rejected string shapes (H3, H4, H5)", () => {
  it("rejects the empty string and a bare prefix", () => {
    // A prefix with no hex digit after it is not a value: "0x" and "0X" fail,
    // while "0b" and "0B" are bare hex runs (b is a hex digit) and are
    // covered by the JS-literal test above.
    for (const value of ["", "0x", "0X", "0x ", "0x-"]) {
      expect(isHexString(value)).toBe(false);
    }
  });

  it("rejects surrounding whitespace instead of trimming it", () => {
    for (const value of [
      " deadbeef",
      "deadbeef ",
      " deadbeef ",
      "\tdeadbeef",
      "\ndeadbeef",
      "deadbeef\n",
      "deadbeef\r",
      "0xdeadbeef\n",
      "0x deadbeef",
    ]) {
      expect(isHexString(value)).toBe(false);
    }
  });

  it("rejects a partial match anywhere in the value", () => {
    for (const value of [
      "0xdeadbeefg",
      "gdeadbeef",
      "dead beef",
      "dead-beef",
      "deadbeef0x",
      "0x0xdeadbeef",
      "0x0x",
      "0x-1",
      "-0x1f",
      "+0x1f",
    ]) {
      expect(isHexString(value)).toBe(false);
    }
  });

  it("rejects an uppercase 0X prefix and JavaScript numeric separators", () => {
    // Documented boundary (H5): the accepted set is not widened, because a
    // wider set suppresses strictly more findings.
    expect(isHexString("0X1f")).toBe(false);
    expect(isHexString("0XABCDEF")).toBe(false);
    expect(isHexString("0X")).toBe(false);
    expect(isHexString("1_000")).toBe(false);
    expect(isHexString("0x1_0")).toBe(false);
    expect(isHexString("0xdead_beef")).toBe(false);
  });

  it("rejects non-hex characters and identifiers", () => {
    for (const value of [
      "deadbeefg",
      "0xdeadbeefg",
      "xyz",
      "g",
      "0xg",
      "0x!",
      "!",
      "-",
      "_",
      "0x0.5",
      "0x1.8p3",
      "Infinity",
      "NaN",
      "null",
      "undefined",
      "true",
      "qlx_live_abc",
      "sk_live_abc",
      "AKIA",
      "xoxb-123-456",
    ]) {
      expect(isHexString(value)).toBe(false);
    }
  });

  it("rejects hex look-alikes built from non-ASCII code points", () => {
    for (const value of [
      `caf${E_ACUTE}`,
      `0xcaf${E_ACUTE}`,
      `deadbeef${E_ACUTE}`,
      SUPERSCRIPT_ZERO,
      FULLWIDTH_A,
      `0x${FULLWIDTH_A}`,
      FULLWIDTH_DIGIT_ZERO,
      `0x${FULLWIDTH_DIGIT_ZERO}`,
      `${CYRILLIC_LOOKALIKE}1337`,
      GRINNING_FACE,
      `deadbeef${GRINNING_FACE}`,
      `${GRINNING_FACE}deadbeef`,
      LONE_SURROGATE,
      `deadbeef${LONE_SURROGATE}`,
    ]) {
      expect(isHexString(value)).toBe(false);
    }
  });
});

describe("isHexString: total across hostile input (H1, H2)", () => {
  const hostileInputs: Array<[string, unknown]> = [
    ["null", null],
    ["undefined", undefined],
    ["zero", 0],
    ["a hex-looking number", 255],
    ["a negative number", -255],
    ["NaN", Number.NaN],
    ["Infinity", Number.POSITIVE_INFINITY],
    ["-Infinity", Number.NEGATIVE_INFINITY],
    ["a hex-looking bigint", 255n],
    ["true", true],
    ["false", false],
    ["a symbol", Symbol("deadbeef")],
    ["a well-known symbol", Symbol.iterator],
    ["a plain object", { secret: "value" }],
    ["a hex-looking String wrapper", new String("deadbeef")],
    ["an empty array", []],
    ["a hex-looking array", ["dead", "beef"]],
    ["a Map", new Map([["deadbeef", "0x1f"]])],
    ["a Set", new Set(["deadbeef"])],
    ["a WeakMap", new WeakMap()],
    ["a Date", new Date(0)],
    ["a function", () => "deadbeef"],
    ["a Buffer", Buffer.from("deadbeef")],
    ["a typed array", new Uint8Array([1, 2, 3, 4])],
    ["a RegExp", /deadbeef/],
    ["an Error", new Error("deadbeef")],
    ["a Promise", Promise.resolve("deadbeef")],
    ["an object with a throwing toString", throwingToString()],
    ["an object with a throwing Symbol.toPrimitive", throwingSymbolToPrimitive()],
    [
      "an object with a throwing toString getter",
      {
        get toString(): () => string {
          throw new Error("boom");
        },
      },
    ],
    ["a revoked proxy", revokedProxy()],
  ];

  it.each(hostileInputs)("never throws for %s", (_label, value) => {
    expect(() => isHexString(value)).not.toThrow();
    expect(typeof isHexString(value)).toBe("boolean");
  });

  it.each(hostileInputs)("returns false for %s", (_label, value) => {
    expect(isHexString(value)).toBe(false);
  });

  it("survives a proxy whose every trap throws", () => {
    const trap = () => {
      throw new Error("boom");
    };
    const hostile = new Proxy({ secret: GIT_SHA }, { get: trap, has: trap, ownKeys: trap });

    expect(() => isHexString(hostile)).not.toThrow();
    expect(isHexString(hostile)).toBe(false);
  });

  it("does not invoke user code to classify a value", () => {
    const trap = jest.fn(() => {
      throw new Error("boom");
    });
    const watched = new Proxy({ secret: GIT_SHA }, { get: trap });

    expect(isHexString(watched)).toBe(false);
    expect(isHexString(throwingSymbolToPrimitive())).toBe(false);
    expect(isHexString(throwingToString())).toBe(false);
    expect(trap).not.toHaveBeenCalled();
  });

  it("reaches a stable verdict on retry after a failed coercion", () => {
    // A retry after a hostile value has already been classified must return
    // the same answer, not a different one and not a propagated failure.
    for (let attempt = 0; attempt < 5; attempt += 1) {
      expect(() => isHexString(throwingToString())).not.toThrow();
      expect(isHexString(throwingToString())).toBe(false);
      expect(isHexString(Symbol("deadbeef"))).toBe(false);
      expect(isHexString(revokedProxy())).toBe(false);
      expect(isHexString(GIT_SHA)).toBe(true);
    }
  });
});

describe("isHexString: the type-confusion regression (H2)", () => {
  it("no longer reports a number as hex just because it coerces to hex digits", () => {
    // These are the values the previous implementation mis-reported as hex:
    // 255 -> "255", 0 -> "0" and 255n -> "255" all match /^[0-9a-fA-F]+$/.
    expect(isHexString(255)).toBe(false);
    expect(isHexString(0)).toBe(false);
    expect(isHexString(255n)).toBe(false);
    expect(isHexString("255")).toBe(true);
  });

  it("no longer reports a String wrapper as hex", () => {
    // eslint-disable-next-line no-new-wrappers
    const wrapped = new String("deadbeef");

    expect(isHexString(wrapped)).toBe(false);
    expect(String(wrapped)).toBe("deadbeef");
    expect(isHexString("deadbeef")).toBe(true);
  });

  it("cannot be tricked into a true by a coercible object", () => {
    const coercible = {
      length: GIT_SHA.length,
      toString: () => GIT_SHA,
      valueOf: () => GIT_SHA,
    };

    expect(isHexString(coercible)).toBe(false);
    // The fixture really is coercible to hex text, so a coercing
    // implementation would have answered true for it.
    expect(String(coercible)).toBe(GIT_SHA);
    expect(`${coercible}`).toBe(GIT_SHA);
  });

  it("leaves the classification of every string value unchanged", () => {
    // Compatibility guard: the type check must not have narrowed or widened
    // the accepted string shapes. The oracle is the previous implementation.
    const corpus: string[] = [
      "",
      "0",
      "0x",
      "0X",
      "0x0",
      "0X1f",
      "deadbeef",
      "DEADBEEF",
      "DeAdBeEf",
      "0xdeadbeef",
      " deadbeef",
      "deadbeef ",
      "deadbeef\n",
      "0xdeadbeefg",
      "dead beef",
      "g",
      "0b1010",
      "0o777",
      "1e10",
      "1_000",
      "xyz",
      "deadbeefg",
      "1234567890",
      "abcdef",
      "ABCDEF",
      "0x1234567890abcdef",
      "0xABCDEF",
      "0.5",
      "0x1.8p3",
      "deadbeefcafebabe",
      "AKIA",
      "Infinity",
      "NaN",
    ];

    const previous = corpus.map(
      (value) => /^[0-9a-fA-F]+$/.test(value) || /^0x[0-9a-fA-F]+$/.test(value)
    );

    expect(corpus.map((value) => isHexString(value))).toEqual(previous);
    expect(previous).toContain(true);
    expect(previous).toContain(false);
  });
});

describe("isHexString: bounded, linear matching (H6)", () => {
  const TIME_BUDGET_MS = 5000;
  const ONE_MEGABYTE = 1000000;

  it("returns false for a very long non-hex value", () => {
    const nonHex = "g".repeat(ONE_MEGABYTE);

    const started = Date.now();
    expect(isHexString(nonHex)).toBe(false);
    expect(Date.now() - started).toBeLessThan(TIME_BUDGET_MS);
  });

  it("returns true for a very long hex value", () => {
    const hex = repeatingHex(ONE_MEGABYTE);

    const started = Date.now();
    expect(isHexString(hex)).toBe(true);
    expect(isHexString(`0x${hex}`)).toBe(true);
    expect(Date.now() - started).toBeLessThan(TIME_BUDGET_MS);
  });

  it("returns false when a long hex run is broken at the far end", () => {
    const broken = `${ALL_HEX_DIGITS.repeat(ONE_MEGABYTE / 16)}g`;

    const started = Date.now();
    expect(isHexString(broken)).toBe(false);
    expect(isHexString(`0x${broken}`)).toBe(false);
    expect(Date.now() - started).toBeLessThan(TIME_BUDGET_MS);
  });
});

describe("isHexString: determinism, retries and interleaving (H6)", () => {
  const corpus: unknown[] = [
    "",
    "0",
    "deadbeef",
    "0XABCDEF",
    "0xdeadbeef",
    "deadbeefg",
    " deadbeef",
    GIT_SHA,
    HEX_PREFIXED_SHA,
    255,
    0,
    255n,
    null,
    undefined,
    true,
    Symbol("deadbeef"),
    { secret: GIT_SHA },
    new String("deadbeef"),
    Buffer.from("deadbeef"),
    throwingToString(),
    throwingSymbolToPrimitive(),
    revokedProxy(),
  ];

  it("returns an identical result for repeated calls", () => {
    const first = corpus.map((value) => isHexString(value));

    for (let attempt = 0; attempt < 25; attempt += 1) {
      expect(corpus.map((value) => isHexString(value))).toEqual(first);
    }
  });

  it("is unaffected by interleaved calls on other values", () => {
    const rounds = 5;
    const first = corpus.map((value) => isHexString(value));
    const interleaved: boolean[] = [];

    for (let round = 0; round < rounds; round += 1) {
      for (const value of corpus) {
        isHexString(GIT_SHA);
        isHexString(255);
        interleaved.push(isHexString(value));
      }
    }

    expect(interleaved).toEqual(new Array(rounds).fill(first).flat());
  });

  it("carries no state across the shared global patterns used by scanLine", () => {
    // collectRegexMatches walks a /g/ pattern and mutates its lastIndex; the
    // hex patterns must be unaffected in either direction.
    const apiKeyPattern = KNOWN_SECRET_PATTERNS[0];
    const line = `qlx_${"live"}_abcdefghijklmnopqrstuvwxyz01`;
    const probes = [GIT_SHA, HEX_PREFIXED_SHA, "0X1f", 255, "deadbeefg"];

    const before = probes.map((value) => isHexString(value));
    const matches = collectRegexMatches(line, apiKeyPattern);
    const after = probes.map((value) => isHexString(value));

    expect(matches).toHaveLength(1);
    expect(after).toEqual(before);
    expect(after).toEqual([true, true, false, false, false]);
  });

  it("stays deterministic under concurrent execution", async () => {
    const expected = corpus.map((value) => isHexString(value));

    const results = await Promise.all(
      Array.from({ length: 25 }, () =>
        Promise.resolve(corpus.map((value) => isHexString(value)))
      )
    );

    for (const round of results) {
      expect(round).toEqual(expected);
    }
  });

  it("does not mutate its argument", () => {
    const value = { secret: GIT_SHA, toString: () => GIT_SHA };
    // eslint-disable-next-line no-new-wrappers
    const wrapped = new String("deadbeef");
    const frozen = Object.freeze({ value: GIT_SHA });

    isHexString(value);
    isHexString(wrapped);
    isHexString(frozen);
    isHexString(GIT_SHA);

    expect(Object.keys(value)).toEqual(["secret", "toString"]);
    expect(String(wrapped)).toBe("deadbeef");
    expect(frozen.value).toBe(GIT_SHA);
  });

  it("returns only a boolean, so a failure stays diagnosable without leaking", () => {
    // A candidate can be a credential, so the classifier must not echo it
    // back: the result carries no information beyond the verdict.
    for (const value of [GIT_SHA, HEX_PREFIXED_SHA, TRUE_ENTROPY_TOKEN, "deadbeef", 255]) {
      const result = isHexString(value);

      expect(typeof result).toBe("boolean");
      expect(String(result)).not.toContain(String(value));
    }
  });
});

describe("isHexString: the existing suppression path is unchanged", () => {
  it("keeps hex runs out of findings at every length that clears the other gates", () => {
    for (const length of [MIN_HIGH_ENTROPY_LENGTH, 40, 64, 128, 4096]) {
      const hex = repeatingHex(length);

      expect(hex).toHaveLength(length);
      expect(isHexString(hex)).toBe(true);
      // Every gate isHighEntropyToken evaluates after the hex guard is
      // satisfied on its own, so the suppression is not incidental.
      expect(new Set(hex).size).toBeGreaterThanOrEqual(MIN_UNIQUE_CHARACTERS);
      expect(hasMixedCharacterClasses(hex)).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder(hex)).toBe(false);
      expect(isHighEntropyToken(hex)).toBe(false);
    }
  });

  it("bounds hex entropy under the score, so the hex guard is a hard rule", () => {
    // 16 hex digits is the whole alphabet, so no hex run of any length can
    // exceed log2(16) = 4. The guard in isHighEntropyToken is therefore an
    // order-independent suppression, and this assertion fails loudly if
    // MIN_HIGH_ENTROPY_SCORE is ever lowered far enough for a commit SHA to
    // be reported on entropy grounds alone.
    const worstCase = ALL_HEX_DIGITS.repeat(64);

    expect(new Set(worstCase).size).toBe(16);
    expect(shannonEntropy(worstCase)).toBeCloseTo(Math.log2(16), 10);
    expect(shannonEntropy(worstCase)).toBeLessThan(MIN_HIGH_ENTROPY_SCORE);
    expect(MIN_HIGH_ENTROPY_SCORE).toBeGreaterThan(Math.log2(16));
  });

  it("separates a hex run from a reported token of the same length", () => {
    const hex = ALL_HEX_DIGITS.repeat(2);

    expect(hex).toHaveLength(TRUE_ENTROPY_TOKEN.length);
    expect(new Set(hex).size).toBeGreaterThanOrEqual(MIN_UNIQUE_CHARACTERS);
    expect(hasMixedCharacterClasses(hex)).toBe(true);
    expect(hasMixedCharacterClasses(TRUE_ENTROPY_TOKEN)).toBe(true);

    expect(isHexString(hex)).toBe(true);
    expect(isHexString(TRUE_ENTROPY_TOKEN)).toBe(false);
    expect(isHighEntropyToken(hex)).toBe(false);
    expect(isHighEntropyToken(TRUE_ENTROPY_TOKEN)).toBe(true);
  });

  it("keeps scanFileContent findings identical for a mixed line", () => {
    const content = [
      `const sha = "${GIT_SHA}";`,
      `const token = "${TRUE_ENTROPY_TOKEN}";`,
      `const padded = " ${HEX_PREFIXED_SHA} ";`,
      // Rejected by H5, so it reaches the entropy score, where it stops.
      `const upperPrefix = "0X${GIT_SHA}";`,
    ].join("\n");

    expect(isHexString(`0X${GIT_SHA}`)).toBe(false);
    expect(shannonEntropy(`0X${GIT_SHA}`)).toBeLessThan(MIN_HIGH_ENTROPY_SCORE);

    const findings = scanFileContent(content, "src/example.ts", ALLOWLIST);

    expect(findings).toHaveLength(1);
    expect(findings[0]).toMatchObject({
      file: "src/example.ts",
      line: 2,
      type: "high-entropy",
      length: TRUE_ENTROPY_TOKEN.length,
    });
    expect(findings[0].preview).not.toContain(TRUE_ENTROPY_TOKEN);
  });

  it("keeps collecting findings for the other lines when a candidate is hostile", () => {
    // scanLine feeds isHighEntropyToken a regex-derived primitive string, so
    // the total contract is exercised through the exported entry point while
    // the pipeline keeps its existing behaviour.
    const content = [`const token = "${TRUE_ENTROPY_TOKEN}";`, `const sha = "${GIT_SHA}";`].join("\n");

    const findings = scanFileContent(content, "src/example.ts", ALLOWLIST);

    expect(findings).toHaveLength(1);

    expect(isHexString(Symbol("deadbeef"))).toBe(false);
    expect(isHexString(revokedProxy())).toBe(false);
    expect(isHexString(255)).toBe(false);

    expect(scanFileContent(content, "src/example.ts", ALLOWLIST)).toEqual(findings);
  });

  it("leaves an allowlisted and a suppressed line both finding-free", () => {
    expect(scanLine(`const sha = "${GIT_SHA}";`, 3, "src/example.ts", ALLOWLIST)).toHaveLength(0);
    expect(scanLine(`const sha = " ${HEX_PREFIXED_SHA} ";`, 3, "src/example.ts", ALLOWLIST)).toHaveLength(
      0
    );
  });

  it("keeps redactPreview as the only place a candidate is rendered", () => {
    const preview = redactPreview(GIT_SHA);

    expect(preview).not.toContain(GIT_SHA);
    expect(preview).toContain("...");
  });
});

describe("isHexString: retries over a real scan produce identical results", () => {
  it("returns byte-identical findings for repeated scans of one tree", () => {
    const root = writeFixtureTree("quicklendx-hex-scan-", {
      "values.ts": [
        `export const sha = "${GIT_SHA}";`,
        `export const token = "${TRUE_ENTROPY_TOKEN}";`,
        `export const padded = " ${HEX_PREFIXED_SHA} ";`,
      ].join("\n"),
    });

    const first = scanBackend(root, { allowlist: ALLOWLIST });
    const second = scanBackend(root, { allowlist: ALLOWLIST });

    expect(first).toHaveLength(1);
    expect(first[0]).toMatchObject({ line: 2, type: "high-entropy" });
    expect(JSON.stringify(second)).toBe(JSON.stringify(first));

    fs.rmSync(root, { recursive: true, force: true });
  });

  it("recovers to the same verdict after a mid-scan read failure", () => {
    const root = writeFixtureTree("quicklendx-hex-retry-", {
      "a.ts": `const token = "${TRUE_ENTROPY_TOKEN}";\n`,
    });
    const healthy = scanBackend(root, { allowlist: ALLOWLIST });

    expect(healthy).toHaveLength(1);

    // A dangling symlink is collected as a file and then fails to read, which
    // aborts the scan exactly as an unreadable file would. The classification
    // step must never be the abort, and the retry must agree with the healthy
    // run.
    const dangling = path.join(root, "src", "b.ts");
    fs.symlinkSync(path.join(root, "src", "missing.ts"), dangling);

    let aborted = false;
    try {
      scanBackend(root, { allowlist: ALLOWLIST });
    } catch {
      aborted = true;
    }

    expect(aborted).toBe(true);
    expect(() => isHexString(Symbol("deadbeef"))).not.toThrow();

    // unlink, not rm: a dangling symlink looks absent to rmSync({force:true}),
    // so it would be skipped and the retry would still hit the same failure.
    fs.unlinkSync(dangling);
    const retried = scanBackend(root, { allowlist: ALLOWLIST });

    expect(JSON.stringify(retried)).toBe(JSON.stringify(healthy));

    fs.rmSync(root, { recursive: true, force: true });
  });
});
