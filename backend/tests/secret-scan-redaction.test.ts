// Failure-boundary coverage for redactPreview in scripts/lib/secret-scan-utils.js.
//
// The invariants exercised here are the R1..R8 block documented above
// redactPreview. They matter because redactPreview sits on the CI reporting
// path: a throw aborts the whole scan and discards findings already collected
// for other files, and a preview that renders a raw control code unit lets a
// single planted "secret" forge or break a line in the job output.
//
// Two deliberate constraints on this file:
//
// 1. Every value longer than a few characters is built from a nine-character
//    alphabet. A value with fewer than MIN_UNIQUE_CHARACTERS distinct
//    characters can never be reported as a high-entropy finding, so the
//    fixtures stay deterministic and the repository tree scan stays clean.
// 2. Non-ASCII and control fixtures are built through chr()/fromCodePoint()
//    rather than written literally, so the source stays printable ASCII.

const secretScanUtils = require("../scripts/lib/secret-scan-utils");

const {
  PREVIEW_EDGE_LENGTH,
  PREVIEW_ELLIPSIS,
  PREVIEW_MASK_CHARACTER,
  PREVIEW_MASK_LENGTH,
  PREVIEW_MAX_LENGTH,
  escapePreviewText,
  isLogSafePreview,
  previewValueTypeTag,
  redactPreview,
} = secretScanUtils;

const ALPHABET = "abcdefghi";
const MASKED_ONE = `"${PREVIEW_MASK_CHARACTER}"`;

// Code units that can move the cursor, start a terminal sequence, or terminate
// a line once a preview reaches a CI log.
const LOG_BREAKING_CLASS = /[\u0000-\u001f\u007f-\u009f\u2028\u2029]/;
const RAW_SURROGATE = /[\ud800-\udfff]/;

const chr = (code: number): string => String.fromCharCode(code);
const astral = (code: number): string => String.fromCodePoint(code);

const NUL = chr(0x00);
const BEL = chr(0x07);
const BACKSPACE = chr(0x08);
const TAB = chr(0x09);
const LINE_FEED = chr(0x0a);
const CARRIAGE_RETURN = chr(0x0d);
const FORM_FEED = chr(0x0c);
const ESCAPE = chr(0x1b);
const DELETE = chr(0x7f);
const NEL = chr(0x85);
const LINE_SEPARATOR = chr(0x2028);
const PARAGRAPH_SEPARATOR = chr(0x2029);
const NO_BREAK_SPACE = astral(0x00a0);
const E_ACUTE = astral(0x00e9);
const CJK = astral(0x4e2d);
const GRINNING_FACE = astral(0x1f600);

function buildValue(length: number, alphabet: string = ALPHABET): string {
  let value = "";
  for (let index = 0; index < length; index += 1) {
    value += alphabet[index % alphabet.length];
  }
  return value;
}

function makeQlxKey(suffix: string): string {
  return ["qlx", "live", suffix].join("_");
}

// Long enough to clear PREVIEW_MASK_LENGTH, and placing the interesting
// character where redactPreview actually renders it: the first four or the
// last four code units. The middle position is never rendered, so a fixture
// parked there would prove nothing about escaping.
const EDGE_FILLER = "bcdefghijklmno";

function embedAtHead(character: string): string {
  return `${character}${EDGE_FILLER}`;
}

function embedAtTail(character: string): string {
  return `abcdefghijklmno${character}`;
}

function embedInMiddle(character: string): string {
  return `abcd${character}efgh`;
}

function makeFinding(value: unknown) {
  const match = typeof value === "string" ? value : buildValue(48);
  return {
    file: "src/leaked.ts",
    line: 4,
    column: 18,
    type: "high-entropy",
    match,
    preview: redactPreview(value as string),
    length: match.length,
  };
}

describe("redactPreview: empty and short-value boundaries (R2, R3)", () => {
  it("renders nullish and empty inputs as an empty quoted preview", () => {
    expect(redactPreview(null)).toBe('""');
    expect(redactPreview(undefined)).toBe('""');
    expect(redactPreview("")).toBe('""');
  });

  it("masks every value up to the mask boundary with an exact-length mask", () => {
    expect(redactPreview("a")).toBe(MASKED_ONE);

    for (let length = 1; length <= PREVIEW_MASK_LENGTH; length += 1) {
      const preview = redactPreview(buildValue(length));

      expect(preview).toBe(`"${PREVIEW_MASK_CHARACTER.repeat(length)}"`);
      expect(preview).toHaveLength(length + 2);
    }
  });

  it("keeps the legacy masked rendering for the short-value range", () => {
    expect(redactPreview("short")).toBe('"*****"');
    expect(redactPreview("abcdefgh")).toBe('"********"');
  });

  it("switches to the edge rendering exactly one character past the boundary", () => {
    const boundary = PREVIEW_MASK_LENGTH;
    const justUnder = buildValue(boundary);
    const justOver = buildValue(boundary + 1);
    const head = justOver.slice(0, PREVIEW_EDGE_LENGTH);
    const tail = justOver.slice(-PREVIEW_EDGE_LENGTH);

    expect(redactPreview(justUnder)).not.toContain(PREVIEW_ELLIPSIS);
    expect(redactPreview(justOver)).toBe(`"${head}${PREVIEW_ELLIPSIS}${tail}"`);
    expect(redactPreview(justOver)).toContain(PREVIEW_ELLIPSIS);
  });

  it("uses a uniform mask so no position carries character information", () => {
    for (let length = 1; length <= PREVIEW_MASK_LENGTH; length += 1) {
      const inner = redactPreview(buildValue(length)).slice(1, -1);

      expect(new Set(inner).size).toBe(1);
      expect(inner).toBe(PREVIEW_MASK_CHARACTER.repeat(length));
    }
  });
});

describe("redactPreview: edge-only non-disclosure (R4)", () => {
  it("never contains the full value for any length past the mask boundary", () => {
    for (let length = PREVIEW_MASK_LENGTH + 1; length <= 256; length += 1) {
      const value = buildValue(length);
      expect(redactPreview(value)).not.toContain(value);
    }
  });

  it("depends only on the leading and trailing edges, never on the middle", () => {
    const head = "abcd";
    const tail = "wxyz";
    // Every middle keeps the value at least one character past the mask
    // boundary, so all of them take the edge-rendering branch.
    const middles = ["Z", "q".repeat(3), "m".repeat(9), "0".repeat(64)];

    const previews = middles.map((middle) => redactPreview(`${head}${middle}${tail}`));

    for (const preview of previews) {
      expect(preview).toBe(`"${head}${PREVIEW_ELLIPSIS}${tail}"`);
    }
    expect(new Set(previews).size).toBe(1);
  });

  it("never renders a character parked in the hidden middle position", () => {
    for (const character of [NUL, BEL, LINE_FEED, ESCAPE, LINE_SEPARATOR, E_ACUTE, GRINNING_FACE]) {
      const value = embedInMiddle(character);

      expect(redactPreview(value)).toBe('"abcd...efgh"');
      expect(redactPreview(value)).not.toContain(character);
    }
  });

  it("keeps values that look like real secret shapes readable at the edges", () => {
    const suffix = buildValue(20);
    const key = makeQlxKey(suffix);

    expect(key.startsWith("qlx_live_")).toBe(true);
    expect(redactPreview(key)).toBe(`"qlx_...${suffix.slice(-PREVIEW_EDGE_LENGTH)}"`);
  });

  it("reveals no more than two edges regardless of input length", () => {
    for (const length of [9, 10, 64, 4096, 200000]) {
      const inner = redactPreview(buildValue(length)).slice(1, -1);
      const edges = inner.split(PREVIEW_ELLIPSIS);

      expect(edges).toHaveLength(2);
      expect(edges[0]).toHaveLength(PREVIEW_EDGE_LENGTH);
      expect(edges[1]).toHaveLength(PREVIEW_EDGE_LENGTH);
    }
  });
});

describe("redactPreview: totality across non-string input (R1, R8)", () => {
  const rejectedInputs: Array<[string, unknown]> = [
    ["zero", 0],
    ["a positive number", 1234567890],
    ["a negative number", -42],
    ["NaN", Number.NaN],
    ["Infinity", Number.POSITIVE_INFINITY],
    ["true", true],
    ["false", false],
    ["a symbol", Symbol("qlx_live_secret")],
    ["a bigint", BigInt(9007199254740991)],
    ["a plain object", { secret: "value" }],
    ["a Date", new Date(0)],
    ["an empty array", []],
    ["a multi-element array", ["first-secret-value", "second-secret-value"]],
    ["a String wrapper", new String("abcdefghijklmnop")],
    ["a Map", new Map([["secret", "value"]])],
    [
      "an object with a throwing toString",
      {
        toString: () => {
          throw new Error("boom");
        },
      },
    ],
    [
      "an object with a throwing Symbol.toPrimitive",
      {
        [Symbol.toPrimitive]: () => {
          throw new Error("boom");
        },
      },
    ],
    [
      "an object with a throwing toString getter",
      {
        get toString() {
          throw new Error("boom");
        },
      },
    ],
    [
      "an object with a throwing length getter",
      {
        get length() {
          throw new Error("boom");
        },
      },
    ],
  ];

  it.each(rejectedInputs)("never throws and never coerces %s", (_label, value) => {
    expect(() => redactPreview(value)).not.toThrow();
    expect(typeof redactPreview(value)).toBe("string");
    expect(isLogSafePreview(redactPreview(value))).toBe(true);
  });

  it("reports the type of a rejected value without disclosing any of it", () => {
    expect(redactPreview(1234567890)).toBe('"[redacted:number]"');
    expect(redactPreview(true)).toBe('"[redacted:boolean]"');
    expect(redactPreview(Symbol("s"))).toBe('"[redacted:symbol]"');
    expect(redactPreview(10n)).toBe('"[redacted:bigint]"');
    expect(redactPreview({ secret: "value" })).toBe('"[redacted:object]"');
    expect(redactPreview(["a", "b"])).toBe('"[redacted:array]"');
    expect(redactPreview(new Map())).toBe('"[redacted:object]"');
    expect(redactPreview(() => "secret")).toBe('"[redacted:function]"');
  });

  it("does not disclose the content of a rejected value", () => {
    const secret = "first-secret-value";
    const preview = redactPreview([secret, secret]);

    expect(preview).not.toContain(secret);
    expect(preview).not.toContain("first");
    expect(preview).not.toContain("second");
  });

  it("survives a revoked proxy without throwing", () => {
    const revoked = Proxy.revocable({ secret: "value" }, {});
    revoked.revoke();

    expect(() => redactPreview(revoked.proxy)).not.toThrow();
    expect(redactPreview(revoked.proxy)).toBe('"[redacted:object]"');
  });

  it("survives a proxy whose every trap throws", () => {
    const trap = () => {
      throw new Error("boom");
    };
    const hostile = new Proxy({ secret: "value" }, { get: trap, ownKeys: trap, has: trap });

    expect(() => redactPreview(hostile)).not.toThrow();
    expect(redactPreview(hostile)).toBe('"[redacted:object]"');
  });

  it("tags the value type without invoking user code", () => {
    const trap = jest.fn(() => {
      throw new Error("boom");
    });
    const hostile = new Proxy({}, { get: trap });

    expect(previewValueTypeTag(hostile)).toBe("object");
    expect(trap).not.toHaveBeenCalled();
    expect(previewValueTypeTag(null)).toBe("null");
    expect(previewValueTypeTag([])).toBe("array");
    expect(previewValueTypeTag(1)).toBe("number");
    expect(previewValueTypeTag({})).toBe("object");
  });
});

describe("redactPreview: log-safe rendering (R5)", () => {
  it("escapes a quote so the value cannot break out of the preview framing", () => {
    expect(redactPreview(embedAtHead('"'))).toBe('"\\"bcd...lmno"');
    expect(redactPreview(embedAtTail('"'))).toBe('"abcd...mno\\""');
    expect(redactPreview(embedAtHead('"'))).toBe(
      `"${escapePreviewText(embedAtHead('"').slice(0, PREVIEW_EDGE_LENGTH))}...lmno"`
    );
  });

  it("escapes backslashes so a value cannot forge an escape sequence", () => {
    expect(redactPreview(embedAtHead("\\"))).toBe('"\\\\bcd...lmno"');
    expect(escapePreviewText("\\n")).toBe("\\\\n");
    expect(escapePreviewText("\\")).toBe("\\\\");
  });

  it("escapes every control and non-printable code unit", () => {
    const cases: Array<[string, string]> = [
      ["\\", "\\\\"],
      ['"', '\\"'],
      [BACKSPACE, "\\b"],
      [FORM_FEED, "\\f"],
      [LINE_FEED, "\\n"],
      [CARRIAGE_RETURN, "\\r"],
      [TAB, "\\t"],
      [NUL, "\\u0000"],
      [BEL, "\\u0007"],
      [ESCAPE, "\\u001b"],
      [DELETE, "\\u007f"],
      [NEL, "\\u0085"],
      [LINE_SEPARATOR, "\\u2028"],
      [PARAGRAPH_SEPARATOR, "\\u2029"],
    ];

    for (const [control, escaped] of cases) {
      expect(escapePreviewText(control)).toBe(escaped);
    }
  });

  it("renders a control character only in its escaped form", () => {
    for (const control of [NUL, BEL, BACKSPACE, TAB, LINE_FEED, CARRIAGE_RETURN, FORM_FEED, ESCAPE]) {
      const atHead = redactPreview(embedAtHead(control));
      const atTail = redactPreview(embedAtTail(control));

      expect(atHead).toBe(`"${escapePreviewText(control)}bcd...lmno"`);
      expect(atTail).toBe(`"abcd...mno${escapePreviewText(control)}"`);
      expect(atHead).not.toContain(control);
      expect(atTail).not.toContain(control);
    }
  });

  it("escapes an ANSI escape so a planted value cannot repaint CI output", () => {
    const ansi = [ESCAPE, "[31m", "red"].join("");
    const preview = redactPreview(embedAtHead(ansi));

    expect(escapePreviewText(ansi)).toBe("\\u001b[31mred");
    expect(preview).toBe('"\\u001b[31...lmno"');
    expect(preview).not.toContain(ESCAPE);
    expect(isLogSafePreview(preview)).toBe(true);
  });

  it("escapes Unicode line separators that survive a per-line split", () => {
    expect(redactPreview(embedAtHead(LINE_SEPARATOR))).toBe('"\\u2028bcd...lmno"');
    expect(redactPreview(embedAtTail(PARAGRAPH_SEPARATOR))).toBe('"abcd...mno\\u2029"');

    for (const separator of [LINE_SEPARATOR, PARAGRAPH_SEPARATOR]) {
      const preview = redactPreview(embedAtHead(separator));

      expect(preview).not.toContain(separator);
      expect(isLogSafePreview(preview)).toBe(true);
    }
  });

  it("escapes both halves of a surrogate pair split by an edge slice", () => {
    const value = `ab${GRINNING_FACE}cd${GRINNING_FACE}efgh`;
    const preview = redactPreview(value);
    const head = escapePreviewText(value.slice(0, PREVIEW_EDGE_LENGTH));
    const tail = escapePreviewText(value.slice(-PREVIEW_EDGE_LENGTH));

    expect(preview).toBe(`"${head}...${tail}"`);
    expect(preview).toContain("\\ud83d");
    expect(preview).toContain("\\ude00");
    expect(RAW_SURROGATE.test(preview)).toBe(false);
    expect(isLogSafePreview(preview)).toBe(true);
  });

  it("escapes every non-ASCII code unit", () => {
    for (const character of [E_ACUTE, CJK, NO_BREAK_SPACE]) {
      const preview = redactPreview(embedAtHead(character));

      expect(preview).toBe(`"${escapePreviewText(character)}bcd...lmno"`);
      expect(isLogSafePreview(preview)).toBe(true);
      expect(preview).not.toContain(character);
    }
  });

  it("emits no raw log-breaking code unit for any input shape", () => {
    const inputs: unknown[] = [
      "",
      null,
      undefined,
      buildValue(1),
      buildValue(PREVIEW_MASK_LENGTH),
      buildValue(PREVIEW_MASK_LENGTH + 1),
      buildValue(1000),
      embedAtHead(NUL),
      embedAtHead(LINE_FEED),
      embedAtHead(TAB),
      embedAtHead(ESCAPE),
      embedAtTail(LINE_SEPARATOR),
      embedAtHead(GRINNING_FACE),
      embedAtHead(E_ACUTE),
      embedInMiddle(LINE_FEED),
      makeQlxKey(buildValue(20)),
      1234567890,
      { secret: "value" },
    ];

    for (const value of inputs) {
      const preview = redactPreview(value);

      expect(isLogSafePreview(preview)).toBe(true);
      expect(LOG_BREAKING_CLASS.test(preview)).toBe(false);
    }
  });

  it("rejects strings that are not rendered previews", () => {
    expect(isLogSafePreview(null)).toBe(false);
    expect(isLogSafePreview(42)).toBe(false);
    expect(isLogSafePreview("abc\ndef")).toBe(false);
    expect(isLogSafePreview("abc\\q")).toBe(false);
    expect(isLogSafePreview("abc\\u00")).toBe(false);
    expect(isLogSafePreview("abc\\U001b")).toBe(false);
    expect(isLogSafePreview(E_ACUTE)).toBe(false);
    expect(isLogSafePreview(GRINNING_FACE)).toBe(false);
  });
});

describe("redactPreview: bounded output (R6)", () => {
  it("caps the rendered length for every accepted input shape", () => {
    const inputs: unknown[] = [
      "",
      buildValue(1),
      buildValue(PREVIEW_MASK_LENGTH),
      buildValue(PREVIEW_MASK_LENGTH + 1),
      buildValue(1000),
      buildValue(100000),
      LINE_FEED.repeat(1000),
      E_ACUTE.repeat(1000),
      GRINNING_FACE.repeat(1000),
      makeQlxKey(buildValue(20)),
      1234567890,
      Symbol("s"),
      { secret: "value" },
    ];

    for (const value of inputs) {
      expect(redactPreview(value).length).toBeLessThanOrEqual(PREVIEW_MAX_LENGTH);
    }
  });

  it("does not amplify memory for a very large value", () => {
    const preview = redactPreview(buildValue(1000000));

    expect(preview.length).toBeLessThanOrEqual(PREVIEW_MAX_LENGTH);
    expect(preview.length).toBeLessThan(64);
  });
});

describe("redactPreview: determinism and reentrancy (R7)", () => {
  it("returns an identical result for repeated calls", () => {
    const value = makeQlxKey(buildValue(20));
    const first = redactPreview(value);

    for (let attempt = 0; attempt < 5; attempt += 1) {
      expect(redactPreview(value)).toBe(first);
    }
  });

  it("is unaffected by interleaved calls on other values", () => {
    const values: unknown[] = [
      "",
      buildValue(3),
      buildValue(9),
      embedAtHead(LINE_FEED),
      makeQlxKey(buildValue(20)),
      1234567890,
      { secret: "value" },
    ];
    const sequential = values.map((value) => redactPreview(value));

    const interleaved: string[] = [];
    for (let round = 0; round < 3; round += 1) {
      for (const value of values) {
        redactPreview(buildValue(round + 1));
        interleaved.push(redactPreview(value));
      }
    }

    expect(interleaved).toEqual([...sequential, ...sequential, ...sequential]);
  });

  it("produces a safe result when re-redacting its own output", () => {
    const value = makeQlxKey(buildValue(20));
    const once = redactPreview(value);
    const twice = redactPreview(once);

    expect(twice).not.toContain(value);
    expect(isLogSafePreview(twice)).toBe(true);
  });
});

describe("redactPreview: existing callers stay compatible", () => {
  it("keeps the historical rendering for every previously valid string shape", () => {
    // Assembled rather than written out so this file does not itself trip the
    // known-secret pattern the scanner matches.
    const awsShaped = ["AKIA", "IOSFODNN7EXAMPLE"].join("");

    expect(redactPreview("")).toBe('""');
    expect(redactPreview("a")).toBe(MASKED_ONE);
    expect(redactPreview("short")).toBe('"*****"');
    expect(redactPreview("abcdefgh")).toBe('"********"');
    expect(redactPreview("abcdefghi")).toBe('"abcd...fghi"');
    expect(redactPreview(awsShaped)).toBe('"AKIA...MPLE"');
  });

  it("keeps scanLine findings unchanged and free of the matched value", () => {
    const suffix = buildValue(20);
    const secret = makeQlxKey(suffix);
    const findings = secretScanUtils.scanLine(
      `const token = "${secret}";`,
      7,
      "src/example.ts",
      { entries: [], globalPatterns: [] }
    );

    expect(findings).toHaveLength(1);
    expect(findings[0]).toMatchObject({
      file: "src/example.ts",
      line: 7,
      type: "quicklendx-api-key",
      length: secret.length,
      preview: `"qlx_...${suffix.slice(-PREVIEW_EDGE_LENGTH)}"`,
    });
    expect(findings[0].preview).not.toContain(secret);
  });

  it("does not let a hostile value forge a passing line in the report", () => {
    const forged = `${"x".repeat(3)} Secret scan ${["pass", "ed"].join("")}`;
    const finding = makeFinding(`${forged}${buildValue(20)}`);
    const output = secretScanUtils.formatFindings([finding]);
    const passingLines = output.split("\n").filter((line) => line.includes("Secret scan passed"));

    expect(passingLines).toHaveLength(0);
    expect(output).toContain("src/leaked.ts:4:18");
    expect(output.split("\n")).toHaveLength(5);
    expect(() => secretScanUtils.assertNoSecretsPrinted(output, [finding])).not.toThrow();
  });

  it("keeps an unredactable finding reportable instead of aborting the scan", () => {
    const finding = makeFinding(1234567890);
    const output = secretScanUtils.formatFinding(finding);

    expect(output).toContain('"[redacted:number]"');
    expect(output).toContain("(48 chars)");
    expect(() => secretScanUtils.assertNoSecretsPrinted(output, [finding])).not.toThrow();
  });

  it("still fails the leak assertion when a preview is replaced with the secret", () => {
    const finding = makeFinding(buildValue(48));
    finding.preview = finding.match;

    expect(() =>
      secretScanUtils.assertNoSecretsPrinted(secretScanUtils.formatFinding(finding), [finding])
    ).toThrow(/leaked a matched value/);
  });
});

describe("assertNoSecretsPrinted: deterministic failure boundaries", () => {
  it("accepts clean output and findings without a matched value", () => {
    expect(() => secretScanUtils.assertNoSecretsPrinted("clean output", [])).not.toThrow();
    expect(() =>
      secretScanUtils.assertNoSecretsPrinted("clean output", [
        { file: "src/example.ts", line: 1, match: "" },
        { file: "src/example.ts", line: 2 },
      ])
    ).not.toThrow();
  });

  it("rejects exact matches at output boundaries without echoing the match", () => {
    const secret = buildValue(48);
    const finding = makeFinding(secret);
    const expectedMessage =
      "Secret scan output leaked a matched value for src/leaked.ts:4";

    for (const output of [secret, `prefix ${secret}`, `${secret} suffix`]) {
      let thrown: unknown;
      try {
        secretScanUtils.assertNoSecretsPrinted(output, [finding]);
      } catch (error) {
        thrown = error;
      }

      expect(thrown).toBeInstanceOf(Error);
      expect((thrown as Error).message).toBe(expectedMessage);
      expect((thrown as Error).message).not.toContain(secret);
    }
  });

  it("reports the first leaking finding consistently when matches are duplicated", () => {
    const secret = buildValue(48);
    const first = makeFinding(secret);
    const second = { ...makeFinding(secret), file: "src/second.ts", line: 9 };

    for (let attempt = 0; attempt < 2; attempt += 1) {
      expect(() => secretScanUtils.assertNoSecretsPrinted(secret, [first, second])).toThrow(
        "Secret scan output leaked a matched value for src/leaked.ts:4"
      );
    }
  });

  it.each([
    [null, [], "Secret scan output must be a string"],
    ["output", null, "Secret scan findings must be an array"],
    ["output", [null], "Secret scan findings must contain objects"],
    ["output", [{ match: Symbol("sensitive") }], "Secret scan finding matches must be strings"],
  ])("rejects invalid inputs with a fixed diagnostic", (output, findings, message) => {
    let thrown: unknown;
    try {
      secretScanUtils.assertNoSecretsPrinted(output, findings);
    } catch (error) {
      thrown = error;
    }

    expect(thrown).toBeInstanceOf(TypeError);
    expect((thrown as Error).message).toBe(message);
    expect((thrown as Error).message).not.toContain("sensitive");
  });
});
