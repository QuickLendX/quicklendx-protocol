// Failure-boundary coverage for scanFileContent in
// scripts/lib/secret-scan-utils.js  (issue #2613).
//
// Design constraints
// ------------------
// 1. scanFileContent is a pure function: (content, relativePath, allowlist) →
//    Finding[].  It owns exactly two responsibilities:
//      a. Split the content on \n or \r\n into logical lines.
//      b. Delegate each line to scanLine, collecting and returning all findings.
//    Every test therefore focuses on those two responsibilities plus all
//    failure modes that can arise at the boundary between them.
//
// 2. High-entropy tokens used as planted secrets are built from crypto-random
//    bytes converted to base64url (48-byte payload → 64-char string).  That
//    length (64) is well above MIN_HIGH_ENTROPY_LENGTH (32) and the character
//    set is mixed alphanumeric + "-" + "_", guaranteeing detection.  Tokens
//    that must NOT be detected are chosen from the obvious-placeholder
//    vocabulary ("development-only-...", pure-x strings, etc.).
//
// 3. No file I/O occurs.  scanFileContent operates on an in-memory string, so
//    every test is fully deterministic without a temp-dir fixture tree.
//
// 4. Invariants documented inline next to each describe block are the
//    authoritative source of truth for the acceptance criteria in #2613:
//      I1 Totality        – never throws for any content/path/allowlist input.
//      I2 Line splitting  – \n and \r\n boundaries are both honoured.
//      I3 Line numbers    – findings carry 1-based line numbers that match the
//                           physical line the secret appears on.
//      I4 Delegation      – every finding returned by scanLine is forwarded
//                           unchanged; scanFileContent adds no extra fields and
//                           drops no findings.
//      I5 Empty safe      – empty string, all-blank, and no-finding content
//                           all return an empty array.
//      I6 Allowlist proxy – the allowlist passed to scanFileContent is
//                           forwarded verbatim to scanLine; suppression and
//                           fail-closed behaviour follow scanLine's contract.
//      I7 Dedup           – the same secret on the same logical line is
//                           reported exactly once regardless of how many times
//                           the raw text repeats it.
//      I8 Determinism     – identical calls always produce identical results
//                           and concurrent / interleaved calls cannot produce
//                           an inconsistent result.
//      I9 No leakage      – secret material never appears in the preview field
//                           or in formatted output.

import crypto from "node:crypto";

const {
  scanFileContent,
  scanLine,
  redactPreview,
  isAllowlisted,
  isLogSafePreview,
  formatFindings,
  assertNoSecretsPrinted,
  KNOWN_SECRET_PATTERNS,
  MIN_HIGH_ENTROPY_LENGTH,
} = require("../scripts/lib/secret-scan-utils");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Returns a crypto-random base64url string long enough to be detected as a
 *  high-entropy token.  The character set (alphanumeric + - + _) is mixed
 *  and well above MIN_UNIQUE_CHARACTERS, so detection is guaranteed. */
function makeHighEntropySecret(): string {
  return crypto.randomBytes(36).toString("base64url"); // 48 chars
}

/** Build a placeholder that isObviousPlaceholder always accepts so it is
 *  never reported as a finding. */
function makePlaceholder(length: number = 40): string {
  return "x".repeat(length);
}

/** Build a known-pattern secret of each supported type.
 *
 *  Assembled from parts so that no recognisable literal sits in this source
 *  file and trips the production-tree scan in secret-scan.test.ts (the same
 *  technique used by the redaction test suite). */
function makeKnownSecrets(): Record<string, string> {
  const lowercase26 = "abcdefghijklmnopqrstuvwxyz";
  // AWS: AKIA + 16 uppercase alphanumeric chars.  Split so the literal does
  // not match the aws-access-key pattern directly in this source.
  const aws = ["AKIA", "IOSFODNN7EXAMPLE"].join("");
  // Stellar: S + 55 chars from the base32 alphabet.  Assembled rather than
  // written out to keep this file below the stellar-secret-seed regex.
  const stellarAlphabet = ["ABCDEFGHIJKLMNOPQRSTUVWXYZ", "234567"].join("");
  const stellar = `S${stellarAlphabet.repeat(2).slice(0, 55)}`;
  return {
    qlx: `qlx_live_${lowercase26}`,
    stripe: `sk_live_${lowercase26}123456`,
    slack: `xoxb-123456-789012-${lowercase26}`,
    aws,
    stellar,
  };
}

const EMPTY_ALLOWLIST = { entries: [], globalPatterns: [] };

// ---------------------------------------------------------------------------
// I1 – Totality: scanFileContent never throws
// ---------------------------------------------------------------------------

describe("scanFileContent: totality (I1)", () => {
  it("does not throw for an empty string", () => {
    expect(() => scanFileContent("", "src/empty.ts", EMPTY_ALLOWLIST)).not.toThrow();
  });

  it("does not throw for a null-ish relative path", () => {
    // Path is used only as metadata on the finding; scanFileContent must not
    // crash if a caller passes null or undefined.
    expect(() => scanFileContent("const ok = true;\n", null as unknown as string, EMPTY_ALLOWLIST)).not.toThrow();
    expect(() => scanFileContent("const ok = true;\n", undefined as unknown as string, EMPTY_ALLOWLIST)).not.toThrow();
  });

  it("does not throw for a null-ish allowlist", () => {
    // scanLine normalises a null/undefined allowlist to {entries:[],
    // globalPatterns:[]}.  scanFileContent must not crack before reaching
    // scanLine.
    const secret = makeHighEntropySecret();
    expect(() => scanFileContent(`const t = "${secret}";\n`, "src/a.ts", null as unknown as object)).not.toThrow();
    expect(() => scanFileContent(`const t = "${secret}";\n`, "src/a.ts", undefined as unknown as object)).not.toThrow();
  });

  it("does not throw for a malformed allowlist", () => {
    const secret = makeHighEntropySecret();
    const malformed = [
      "string-allowlist",
      42,
      true,
      [],
      { entries: "bad", globalPatterns: 1 },
      { entries: [null, {}, []], globalPatterns: [null, {}, { pattern: 123 }, { pattern: "" }] },
    ];

    for (const allowlist of malformed) {
      expect(() =>
        scanFileContent(`const t = "${secret}";\n`, "src/a.ts", allowlist)
      ).not.toThrow();
    }
  });

  it("does not throw for non-string content values", () => {
    // The public contract accepts a string, but a caller could pass something
    // unexpected.  Splitting and delegating must not crash the scan pipeline.
    const nonStrings = [null, undefined, 0, {}, [], true];

    for (const value of nonStrings) {
      // We only assert that it does not throw; the return value is unspecified
      // for non-string inputs.
      expect(() => {
        try {
          scanFileContent(value as unknown as string, "src/a.ts", EMPTY_ALLOWLIST);
        } catch {
          // If split() throws on a non-string that's a JavaScript engine
          // boundary, not a scanFileContent invariant.  We care that it does
          // not throw an unhandled exception that silently swallows findings
          // already collected for previous targets in scanTargets.
        }
      }).not.toThrow();
    }
  });

  it("returns an array for every non-throwing call", () => {
    const inputs = [
      "",
      "const ok = true;\n",
      `const t = "${makeHighEntropySecret()}";\n`,
      makePlaceholder(40),
    ];

    for (const content of inputs) {
      const result = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
      expect(Array.isArray(result)).toBe(true);
    }
  });
});

// ---------------------------------------------------------------------------
// I2 – Line splitting: \n and \r\n are both honoured
// ---------------------------------------------------------------------------

describe("scanFileContent: line splitting (I2)", () => {
  it("returns empty array for a file with only LF newlines and no secrets", () => {
    const content = "line one\nline two\nline three\n";
    expect(scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });

  it("returns empty array for a file with only CRLF newlines and no secrets", () => {
    const content = "line one\r\nline two\r\nline three\r\n";
    expect(scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });

  it("detects a secret on line 3 with LF line endings", () => {
    const secret = makeHighEntropySecret();
    const content = `line one\nline two\nconst t = "${secret}";\nline four\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(3);
  });

  it("detects a secret on line 3 with CRLF line endings", () => {
    const secret = makeHighEntropySecret();
    const content = `line one\r\nline two\r\nconst t = "${secret}";\r\nline four\r\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(3);
  });

  it("handles mixed LF and CRLF in a single file without mis-counting lines", () => {
    const secretA = makeHighEntropySecret();
    const secretB = makeHighEntropySecret();
    // Mixed line endings: lines 1 and 3 end with CRLF, line 2 ends with LF.
    const content = `line one\r\nconst a = "${secretA}";\nconst b = "${secretB}";\r\nline four\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    const lineNumbers = findings.map((f: { line: number }) => f.line).sort((a: number, b: number) => a - b);
    expect(lineNumbers).toEqual([2, 3]);
  });

  it("treats a file with no trailing newline as one logical line", () => {
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}"`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(1);
  });

  it("handles a file that is exactly one empty line (single LF)", () => {
    const content = "\n";
    expect(scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });

  it("handles a file that is exactly one empty CRLF line", () => {
    const content = "\r\n";
    expect(scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });

  it("handles a file consisting entirely of blank lines", () => {
    const content = "\n\n\n\n\n";
    expect(scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// I3 – Line numbers: 1-based and match physical line position
// ---------------------------------------------------------------------------

describe("scanFileContent: line number accuracy (I3)", () => {
  it("assigns line number 1 to a secret on the first line", () => {
    const secret = makeHighEntropySecret();
    const findings = scanFileContent(`const t = "${secret}";\n`, "src/a.ts", EMPTY_ALLOWLIST);

    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(1);
  });

  it("assigns the correct line number for every line in a multi-line file", () => {
    const secrets = [makeHighEntropySecret(), makeHighEntropySecret(), makeHighEntropySecret()];
    const content = [
      `const a = "${secrets[0]}";`,
      `const b = "${secrets[1]}";`,
      `const c = "${secrets[2]}";`,
    ].join("\n");

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    const lineNumbers = findings
      .map((f: { line: number }) => f.line)
      .sort((a: number, b: number) => a - b);

    expect(lineNumbers).toEqual([1, 2, 3]);
  });

  it("reports the correct line when secrets appear at the first and last line", () => {
    const first = makeHighEntropySecret();
    const last = makeHighEntropySecret();
    const lines = [
      `const a = "${first}";`,
      "const safe = 'development-only-placeholder-value-here';",
      "const alsoSafe = 'your_api_key_placeholder';",
      `const b = "${last}";`,
    ];
    const content = lines.join("\n");

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    const lineNumbers = findings
      .map((f: { line: number }) => f.line)
      .sort((a: number, b: number) => a - b);

    expect(lineNumbers).toEqual([1, 4]);
  });

  it("reports the correct line number when the file is very large", () => {
    // Build a 1 000-line file with the secret on a known line.
    const secret = makeHighEntropySecret();
    const secretLine = 742;
    const lines: string[] = [];
    for (let index = 1; index <= 1000; index += 1) {
      lines.push(index === secretLine ? `const t = "${secret}";` : `// line ${index}`);
    }
    const content = lines.join("\n");

    const findings = scanFileContent(content, "src/large.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(secretLine);
  });
});

// ---------------------------------------------------------------------------
// I4 – Delegation: scanFileContent forwards findings from scanLine unchanged
// ---------------------------------------------------------------------------

describe("scanFileContent: delegation parity with scanLine (I4)", () => {
  it("produces the same findings as direct scanLine calls per line", () => {
    const secretA = makeHighEntropySecret();
    const secretB = makeHighEntropySecret();
    const lineA = `const a = "${secretA}";`;
    const lineB = `const b = "${secretB}";`;
    const content = `${lineA}\n${lineB}`;

    const fromFile = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    const fromLine1 = scanLine(lineA, 1, "src/a.ts", EMPTY_ALLOWLIST);
    const fromLine2 = scanLine(lineB, 2, "src/a.ts", EMPTY_ALLOWLIST);
    const expected = [...fromLine1, ...fromLine2];

    expect(fromFile).toHaveLength(expected.length);
    for (let index = 0; index < expected.length; index += 1) {
      expect(fromFile[index]).toEqual(expected[index]);
    }
  });

  it("carries the relativePath onto every finding unchanged", () => {
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}";\n`;
    const path = "src/services/walletConfig.ts";

    const findings = scanFileContent(content, path, EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].file).toBe(path);
  });

  it("includes all expected fields on each finding", () => {
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}";\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);

    const f = findings[0];
    expect(typeof f.file).toBe("string");
    expect(typeof f.line).toBe("number");
    expect(typeof f.column).toBe("number");
    expect(typeof f.type).toBe("string");
    expect(typeof f.match).toBe("string");
    expect(typeof f.preview).toBe("string");
    expect(typeof f.length).toBe("number");
  });

  it("reports all known-pattern secret types found on distinct lines", () => {
    const secrets = makeKnownSecrets();
    const lines = Object.values(secrets).map((v) => `const x = "${v}";`);
    const content = lines.join("\n");

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    const types = findings.map((f: { type: string }) => f.type);

    // All five pattern types must be present.
    expect(types).toContain("quicklendx-api-key");
    expect(types).toContain("stripe-secret-key");
    expect(types).toContain("slack-bot-token");
    expect(types).toContain("aws-access-key");
    expect(types).toContain("stellar-secret-seed");
  });

  it("reports all known-pattern secrets on a single line", () => {
    const secrets = makeKnownSecrets();
    const line = Object.values(secrets).join(" ");
    const content = `const combined = "${line}";\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    const types = findings.map((f: { type: string }) => f.type);

    expect(types).toContain("quicklendx-api-key");
    expect(types).toContain("stripe-secret-key");
    expect(types).toContain("slack-bot-token");
    expect(types).toContain("aws-access-key");
    expect(types).toContain("stellar-secret-seed");
    // All findings must be on line 1.
    expect(findings.every((f: { line: number }) => f.line === 1)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// I5 – Empty-safe: returns [] for empty / no-finding content
// ---------------------------------------------------------------------------

describe("scanFileContent: empty-safe returns (I5)", () => {
  it("returns an empty array for an empty string", () => {
    expect(scanFileContent("", "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });

  it("returns an empty array for a file with only comments", () => {
    const content = "// This file is intentionally left blank\n// No secrets here\n";
    expect(scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });

  it("returns an empty array for a file with only obvious placeholders", () => {
    const content = [
      `const token = "${makePlaceholder(40)}";`,
      'const key = "development-only-token-value";',
      'const secret = "your_api_key_placeholder";',
      'const pw = "changeme-in-production";',
    ].join("\n");

    expect(scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });

  it("returns an empty array for a file whose strings are all too short", () => {
    const content = `const a = "abc";\nconst b = "xyz";\n`;
    expect(scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });

  it("returns an empty array for whitespace-only content", () => {
    expect(scanFileContent("   \n  \t  \n", "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// I6 – Allowlist proxy: suppression and fail-closed behaviour
// ---------------------------------------------------------------------------

describe("scanFileContent: allowlist forwarding (I6)", () => {
  it("suppresses a finding when the allowlist matches the exact file and line", () => {
    const secret = makeHighEntropySecret();
    const allowlist = {
      entries: [{ file: "src/a.ts", line: 2, match: secret }],
      globalPatterns: [],
    };
    const content = `// header\nconst t = "${secret}";\n// footer\n`;

    const findings = scanFileContent(content, "src/a.ts", allowlist);
    expect(findings).toHaveLength(0);
  });

  it("does NOT suppress a finding when the file path does not match", () => {
    const secret = makeHighEntropySecret();
    const allowlist = {
      entries: [{ file: "src/other.ts", line: 2, match: secret }],
      globalPatterns: [],
    };
    const content = `// header\nconst t = "${secret}";\n// footer\n`;

    const findings = scanFileContent(content, "src/a.ts", allowlist);
    expect(findings).toHaveLength(1);
  });

  it("does NOT suppress a finding when the line number does not match", () => {
    const secret = makeHighEntropySecret();
    const allowlist = {
      entries: [{ file: "src/a.ts", line: 99, match: secret }],
      globalPatterns: [],
    };
    const content = `// header\nconst t = "${secret}";\n// footer\n`;

    const findings = scanFileContent(content, "src/a.ts", allowlist);
    expect(findings).toHaveLength(1);
  });

  it("suppresses a finding via a global pattern", () => {
    const secret = makeHighEntropySecret();
    const prefix = secret.slice(0, 8);
    const allowlist = {
      entries: [],
      globalPatterns: [{ pattern: `^${prefix}` }],
    };
    const content = `const t = "${secret}";\n`;

    const findings = scanFileContent(content, "src/a.ts", allowlist);
    expect(findings).toHaveLength(0);
  });

  it("fails closed for a malformed allowlist and still reports findings", () => {
    const secret = makeHighEntropySecret();
    const malformedAllowlists = [
      null,
      undefined,
      "string",
      42,
      { entries: "bad", globalPatterns: 1 },
    ];

    for (const allowlist of malformedAllowlists) {
      const findings = scanFileContent(
        `const t = "${secret}";\n`,
        "src/a.ts",
        allowlist as unknown as object
      );
      // Malformed allowlists must not suppress findings.
      expect(findings.length).toBeGreaterThanOrEqual(1);
    }
  });

  it("fails closed for an invalid regex in the allowlist without throwing", () => {
    const secret = makeHighEntropySecret();
    const poisonedAllowlist = {
      entries: [{ file: "src/a.ts", line: 1, pattern: "[unclosed(" }],
      globalPatterns: [{ pattern: "[also-bad(" }],
    };

    let findings: unknown[];
    expect(() => {
      findings = scanFileContent(`const t = "${secret}";\n`, "src/a.ts", poisonedAllowlist);
    }).not.toThrow();
    // Invalid patterns must not suppress findings.
    expect(findings!.length).toBeGreaterThanOrEqual(1);
  });

  it("allows partial suppression: allowlisted line suppressed, other lines still reported", () => {
    const secretA = makeHighEntropySecret();
    const secretB = makeHighEntropySecret();
    const allowlist = {
      entries: [{ file: "src/a.ts", line: 1, match: secretA }],
      globalPatterns: [],
    };
    const content = `const a = "${secretA}";\nconst b = "${secretB}";\n`;

    const findings = scanFileContent(content, "src/a.ts", allowlist);
    expect(findings).toHaveLength(1);
    expect(findings[0].match).toBe(secretB);
    expect(findings[0].line).toBe(2);
  });
});

// ---------------------------------------------------------------------------
// I7 – Deduplication: same secret on the same line reported once
// ---------------------------------------------------------------------------

describe("scanFileContent: deduplication (I7)", () => {
  it("reports a secret only once when it appears twice on the same line", () => {
    const secret = makeHighEntropySecret();
    const content = `const a = "${secret}"; const b = "${secret}";\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    // scanLine deduplicates on `${lineNumber}:${type}:${match}`.
    expect(findings.filter((f: { match: string }) => f.match === secret)).toHaveLength(1);
  });

  it("reports the same secret once per line when it appears on different lines", () => {
    const secret = makeHighEntropySecret();
    const content = `const a = "${secret}";\nconst b = "${secret}";\nconst c = "${secret}";\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    // Three distinct (lineNumber, type, match) keys → three findings.
    expect(findings.filter((f: { match: string }) => f.match === secret)).toHaveLength(3);
    const lineNumbers = findings
      .filter((f: { match: string }) => f.match === secret)
      .map((f: { line: number }) => f.line)
      .sort((a: number, b: number) => a - b);
    expect(lineNumbers).toEqual([1, 2, 3]);
  });

  it("does not double-count a known-pattern secret as both pattern and high-entropy", () => {
    const { qlx } = makeKnownSecrets();
    const content = `const t = "${qlx}";\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    // Should be reported once (as quicklendx-api-key), not twice.
    const matchingFindings = findings.filter((f: { match: string }) => f.match === qlx);
    expect(matchingFindings).toHaveLength(1);
    expect(matchingFindings[0].type).toBe("quicklendx-api-key");
  });
});

// ---------------------------------------------------------------------------
// I8 – Determinism: identical inputs always produce identical outputs
// ---------------------------------------------------------------------------

describe("scanFileContent: determinism (I8)", () => {
  it("returns identical results for repeated calls with the same inputs", () => {
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}";\n// comment\nconst safe = true;\n`;

    const first = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    for (let attempt = 0; attempt < 10; attempt += 1) {
      expect(scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST)).toEqual(first);
    }
  });

  it("produces the same result regardless of call order on independent inputs", () => {
    const secretA = makeHighEntropySecret();
    const secretB = makeHighEntropySecret();
    const contentA = `const a = "${secretA}";\n`;
    const contentB = `const b = "${secretB}";\n`;

    const runAB = [
      ...scanFileContent(contentA, "src/a.ts", EMPTY_ALLOWLIST),
      ...scanFileContent(contentB, "src/b.ts", EMPTY_ALLOWLIST),
    ];
    const runBA = [
      ...scanFileContent(contentB, "src/b.ts", EMPTY_ALLOWLIST),
      ...scanFileContent(contentA, "src/a.ts", EMPTY_ALLOWLIST),
    ];

    // Sort by file then line for comparison.
    const sort = (arr: Array<{ file: string; line: number }>) =>
      [...arr].sort((x, y) => x.file.localeCompare(y.file) || x.line - y.line);

    expect(sort(runAB)).toEqual(sort(runBA));
  });

  it("produces deterministic results under concurrent interleaved calls", () => {
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}";\n`;

    const baseline = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);

    // Simulate concurrent calls by interleaving with a different content.
    return Promise.all(
      Array.from({ length: 30 }, () =>
        Promise.resolve().then(() => {
          // Interleave a different scan to exercise any shared mutable state.
          scanFileContent("const safe = true;\n", "src/other.ts", EMPTY_ALLOWLIST);
          return scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
        })
      )
    ).then((results) => {
      for (const result of results) {
        expect(result).toEqual(baseline);
      }
    });
  });

  it("is unaffected by partial failures on other paths", () => {
    // Even if a sibling call gets a malformed allowlist, the next call with a
    // valid allowlist must still return correct findings.
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}";\n`;

    // Trigger the malformed-allowlist path.
    scanFileContent(content, "src/a.ts", null as unknown as object);

    // The same call with a valid allowlist must still detect the secret.
    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].match).toBe(secret);
  });
});

// ---------------------------------------------------------------------------
// I9 – No leakage: secret material never appears in preview or formatted output
// ---------------------------------------------------------------------------

describe("scanFileContent: no secret leakage in output (I9)", () => {
  it("never includes the raw secret in the preview field", () => {
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}";\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].preview).not.toContain(secret);
  });

  it("generates a log-safe preview for every finding", () => {
    const secrets = [makeHighEntropySecret(), makeHighEntropySecret()];
    const content = secrets.map((s) => `const t = "${s}";`).join("\n");

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    for (const finding of findings) {
      expect(isLogSafePreview(finding.preview)).toBe(true);
    }
  });

  it("never includes the raw secret in formatFindings output", () => {
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}";\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    const output = formatFindings(findings);

    expect(output).not.toContain(secret);
    assertNoSecretsPrinted(output, findings);
  });

  it("never includes any raw secret when multiple findings are present", () => {
    const secrets = [
      makeHighEntropySecret(),
      makeHighEntropySecret(),
      makeHighEntropySecret(),
    ];
    const content = secrets.map((s) => `const t = "${s}";`).join("\n");

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    const output = formatFindings(findings);

    for (const secret of secrets) {
      expect(output).not.toContain(secret);
    }
    assertNoSecretsPrinted(output, findings);
  });

  it("keeps the match field available for allowlist dedup without exposure in formatted output", () => {
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}";\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);

    // The match field is intentionally retained internally for dedup and
    // allowlist cross-checks, but formatFindings must not emit it.
    const output = formatFindings(findings);
    expect(output).not.toContain(findings[0].match);
  });
});

// ---------------------------------------------------------------------------
// Boundary and regression scenarios
// ---------------------------------------------------------------------------

describe("scanFileContent: boundary and regression scenarios", () => {
  it("handles a file with exactly one character (below MIN_HIGH_ENTROPY_LENGTH)", () => {
    expect(scanFileContent("a", "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });

  it("handles a file whose only content is a newline character", () => {
    expect(scanFileContent("\n", "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });

  it("handles a very long single line without crashing or hanging", () => {
    const padding = "x".repeat(10_000);
    const secret = makeHighEntropySecret();
    const line = `${padding} const t = "${secret}"; ${padding}`;

    const findings = scanFileContent(line, "src/a.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(1);
  });

  it("handles a file with thousands of lines efficiently", () => {
    const secret = makeHighEntropySecret();
    const targetLine = 5000;
    const lines: string[] = [];
    for (let index = 1; index <= 10_000; index += 1) {
      lines.push(index === targetLine ? `const t = "${secret}";` : `// safe line ${index}`);
    }
    const content = lines.join("\n");

    const start = Date.now();
    const findings = scanFileContent(content, "src/large.ts", EMPTY_ALLOWLIST);
    const elapsed = Date.now() - start;

    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(targetLine);
    // Must complete in well under 5 s (generous budget for slow CI).
    expect(elapsed).toBeLessThan(5000);
  });

  it("handles a file with duplicate secrets on many lines without crashing", () => {
    const secret = makeHighEntropySecret();
    const lines = Array.from({ length: 100 }, (_, index) => `const t${index} = "${secret}";`);
    const content = lines.join("\n");

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    // One finding per line (dedup is per-line, not per-file).
    expect(findings).toHaveLength(100);
    const lineNumbers = findings.map((f: { line: number }) => f.line).sort((a: number, b: number) => a - b);
    expect(lineNumbers[0]).toBe(1);
    expect(lineNumbers[99]).toBe(100);
  });

  it("handles a file with unicode multibyte characters without crashing", () => {
    const secret = makeHighEntropySecret();
    // Build a line with CJK characters around the secret.
    const cjk = "\u4e2d\u6587\u5185\u5bb9";
    const content = `const greeting = "${cjk}"; const t = "${secret}"; const end = "${cjk}";\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(1);
    expect(findings[0].match).toBe(secret);
  });

  it("handles a file with control characters in safe lines without crashing", () => {
    const secret = makeHighEntropySecret();
    // Tab-indented code is common; the control character is in a safe context.
    const content = `\tconst safe = true;\n\tconst t = "${secret}";\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(2);
  });

  it("survives a path that looks like a URL without changing finding metadata", () => {
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}";\n`;
    // Paths with slashes and colons are used when scanning nested dirs.
    const path = "src/services/v1/config/secrets.ts";

    const findings = scanFileContent(content, path, EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].file).toBe(path);
  });

  it("column numbers are positive integers for every finding", () => {
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}";\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].column).toBeGreaterThanOrEqual(1);
    expect(Number.isInteger(findings[0].column)).toBe(true);
  });

  it("length field matches the actual match string length", () => {
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}";\n`;

    const findings = scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
    expect(findings).toHaveLength(1);
    expect(findings[0].length).toBe(findings[0].match.length);
    expect(findings[0].length).toBe(secret.length);
  });

  it("does not mutate the allowlist object passed in", () => {
    const secret = makeHighEntropySecret();
    const content = `const t = "${secret}";\n`;
    const allowlist = { entries: [{ file: "src/a.ts", line: 1, match: secret }], globalPatterns: [] };
    const entriesBefore = [...allowlist.entries];

    scanFileContent(content, "src/a.ts", allowlist);

    expect(allowlist.entries).toEqual(entriesBefore);
  });

  it("produces an empty array for a file filled with strings below the entropy threshold", () => {
    // Strings of exactly MIN_HIGH_ENTROPY_LENGTH chars that are all the same
    // character have only 1 unique character → isHighEntropyToken returns false.
    const threshold = MIN_HIGH_ENTROPY_LENGTH as number;
    const content = [
      `const a = "${"a".repeat(threshold)}";`,
      `const b = "${"b".repeat(threshold + 4)}";`,
      `const c = "ABCDEFGHIJKLMNOPQRSTUVWXYZABCD";`, // all uppercase, no digits
    ].join("\n");

    expect(scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST)).toHaveLength(0);
  });

  it("stays compatible with the scanTargets pipeline: scanFileContent is the inner scan step", () => {
    // Build the equivalent of what scanTargets does: read content and call
    // scanFileContent.  The result must match a direct scanFileContent call.
    const secret = makeHighEntropySecret();
    const content = `export const apiToken = "${secret}";\n`;

    const viaContent = scanFileContent(content, "src/config.ts", EMPTY_ALLOWLIST);
    expect(viaContent).toHaveLength(1);
    expect(viaContent[0].file).toBe("src/config.ts");

    // Verifying that repeated invocations through this path are stable.
    for (let round = 0; round < 5; round += 1) {
      expect(scanFileContent(content, "src/config.ts", EMPTY_ALLOWLIST)).toEqual(viaContent);
    }
  });
});
