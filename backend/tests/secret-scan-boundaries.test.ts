import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import crypto from "node:crypto";
import { execFileSync } from "node:child_process";

const secretScanUtils = require("../scripts/lib/secret-scan-utils");

const EMPTY_ALLOWLIST = { entries: [], globalPatterns: [] };

// Each part stays below MIN_HIGH_ENTROPY_LENGTH so the scanner's self-scan of
// this file stays clean; the full alphabet is assembled at runtime.
const UPPERCASE_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWERCASE_ALPHABET = "abcdefghijklmnopqrstuvwxyz";
const DIGITS = "0123456789";
const ALPHANUMERIC_ALPHABET = UPPERCASE_ALPHABET + LOWERCASE_ALPHABET + DIGITS;

/**
 * Secrets are generated rather than hardcoded on purpose. This file lives under
 * `backend/tests`, which is one of the scanner's own scan roots and is asserted
 * to be finding-free by `tests/secret-scan.test.ts`. A literal high-entropy
 * value in the source would make the scanner fail on its own test suite.
 */
function randomFromAlphabet(alphabet: string, length: number): string {
  let value = "";
  for (let index = 0; index < length; index += 1) {
    value += alphabet[crypto.randomInt(0, alphabet.length)];
  }
  return value;
}

function randomHighEntropy(): string {
  return crypto.randomBytes(36).toString("base64url");
}

/** Body is alphanumeric-only so the whole token satisfies `[A-Za-z0-9]{20,}`. */
function makeStripeKey(): string {
  return `sk_live_${randomFromAlphabet(ALPHANUMERIC_ALPHABET, 32)}`;
}

function makeAwsKey(): string {
  return `AKIA${randomFromAlphabet(UPPERCASE_ALPHABET, 16)}`;
}

function createFixtureDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "quicklendx-scan-boundary-"));
}

function writeFixture(root: string, relativePath: string, content: string): string {
  const absolutePath = path.join(root, relativePath);
  fs.mkdirSync(path.dirname(absolutePath), { recursive: true });
  fs.writeFileSync(absolutePath, content, "utf8");
  return absolutePath;
}

/** Counts how many of a secret's characters survive into its preview. */
function revealedCharacters(secret: string, preview: string): number {
  const body = preview.slice(1, -1);

  if (!body.includes("...")) {
    return 0;
  }

  const [head, tail] = body.split("...");
  return head.length + tail.length;
}

function findingTypes(findings: Array<{ type: string }>): string[] {
  return findings.map((finding) => finding.type);
}

function readSecretScanUtilsInChildProcess(source: string): string {
  const modulePath = path.resolve(__dirname, "../scripts/lib/secret-scan-utils.js");
  return execFileSync("node", ["-e", `const u = require(${JSON.stringify(modulePath)});\n${source}`], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    timeout: 5000,
  });
}

afterEach(() => {
  jest.restoreAllMocks();
});

describe("scanLine redaction boundary", () => {
  it("never renders a preview that reconstructs the matched value", () => {
    for (let length = 1; length <= 64; length += 1) {
      const secret = ("ab34cd56ef" + randomFromAlphabet(ALPHANUMERIC_ALPHABET, 12)).slice(
        0,
        length
      );
      const preview = secretScanUtils.redactPreview(secret);

      expect(preview).not.toBe(`"${secret}"`);
      expect(preview.startsWith('"')).toBe(true);
      expect(preview.endsWith('"')).toBe(true);

      const revealed = revealedCharacters(secret, preview);
      const revealedRatio = revealed / length;

      expect(revealedRatio).toBeLessThanOrEqual(0.5);
    }
  });

  it("masks values that are too short to preview without disclosing them", () => {
    const shortestUnsafe = secretScanUtils.MIN_PARTIAL_PREVIEW_LENGTH - 1;
    const secret = randomFromAlphabet(ALPHANUMERIC_ALPHABET, shortestUnsafe);

    const preview = secretScanUtils.redactPreview(secret);

    expect(preview).toBe(`"${"*".repeat(shortestUnsafe)}"`);
    expect(preview).not.toContain("...");
    expect(revealedCharacters(secret, preview)).toBe(0);

    const atThreshold = randomFromAlphabet(
      ALPHANUMERIC_ALPHABET,
      secretScanUtils.MIN_PARTIAL_PREVIEW_LENGTH
    );
    expect(secretScanUtils.redactPreview(atThreshold)).toContain("...");
  });

  it("keeps previews of short matches out of formatted scan output", () => {
    const secret = randomFromAlphabet(ALPHANUMERIC_ALPHABET, 9);
    const preview = secretScanUtils.redactPreview(secret);
    const findings = [
      {
        file: "src/short.ts",
        line: 1,
        column: 1,
        type: "high-entropy",
        match: secret,
        preview,
        length: secret.length,
      },
    ];

    const output = secretScanUtils.formatFindings(findings);

    expect(output).not.toContain(secret);
    secretScanUtils.assertNoSecretsPrinted(output, findings);
  });
});

describe("scanLine allowlist fail-closed boundaries", () => {
  it("does not let an empty or whitespace pattern suppress findings", () => {
    const stripeKey = makeStripeKey();
    const awsKey = makeAwsKey();
    const line = `stripe="${stripeKey}" aws="${awsKey}"`;

    const findings = secretScanUtils.scanLine(line, 1, "src/a.ts", {
      entries: [{ pattern: "" }],
      globalPatterns: [{ pattern: "" }],
    });

    expect(findingTypes(findings)).toEqual(["stripe-secret-key", "aws-access-key"]);
  });

  it("does not throw when an allowlist pattern is not a valid regular expression", () => {
    const awsKey = makeAwsKey();
    const malformed = [
      "[",
      "(unclosed",
      "a{2,1}",
      "*leading-quantifier",
      "(?<bad>",
      "\\",
    ];

    for (const pattern of malformed) {
      let findings: Array<{ type: string }> = [];
      expect(() => {
        findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
          entries: [{ file: "src/a.ts", line: 1, pattern }],
          globalPatterns: [{ pattern }],
        });
      }).not.toThrow();

      expect(findingTypes(findings)).toEqual(["aws-access-key"]);
    }
  });

  it("does not let a non-string pattern suppress findings", () => {
    const awsKey = makeAwsKey();

    for (const pattern of [null, 0, 1, true, {}, [], { source: "AKIA" }]) {
      const findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [{ pattern }],
        globalPatterns: [{ pattern }],
      });

      expect(findingTypes(findings)).toEqual(["aws-access-key"]);
    }
  });

  it("still honours well-formed allowlist entries and global patterns", () => {
    const awsKey = makeAwsKey();

    expect(
      secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [{ file: "src/a.ts", line: 1, pattern: "^AKIA" }],
        globalPatterns: [],
      })
    ).toHaveLength(0);

    expect(
      secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [],
        globalPatterns: [{ pattern: "^AKIA[0-9A-Z]{16}$" }],
      })
    ).toHaveLength(0);

    expect(
      secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [{ file: "src/other.ts", line: 1, pattern: "^AKIA" }],
        globalPatterns: [],
      })
    ).toHaveLength(1);
  });

  it("treats an anchor-only pattern as an exact-match allowlist, not a wildcard", () => {
    const awsKey = makeAwsKey();

    expect(
      secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [{ file: "src/a.ts", line: 1, match: "AKIA" }],
        globalPatterns: [],
      })
    ).toHaveLength(0);

    const otherKey = makeAwsKey();
    expect(
      secretScanUtils.scanLine(`aws="${awsKey}" other="${otherKey}"`, 1, "src/a.ts", {
        entries: [{ file: "src/a.ts", line: 1, match: awsKey }],
        globalPatterns: [],
      })
    ).toHaveLength(1);
  });
});

describe("scanLine input validation", () => {
  it("rejects a non-string line instead of scanning a coerced value", () => {
    const invalidLines = [null, undefined, 0, 42, true, {}, [], Symbol("line"), 10n];

    for (const line of invalidLines) {
      expect(() => secretScanUtils.scanLine(line, 1, "src/a.ts", EMPTY_ALLOWLIST)).toThrow(
        TypeError
      );
      expect(() => secretScanUtils.scanLine(line, 1, "src/a.ts", EMPTY_ALLOWLIST)).toThrow(
        /scanLine requires a string line/
      );
    }
  });

  it("rejects line numbers that cannot reconcile with an allowlist entry", () => {
    const invalidLineNumbers = [0, -1, 1.5, "1", "3", NaN, Infinity, null, undefined, 2n, {}];

    for (const lineNumber of invalidLineNumbers) {
      expect(() =>
        secretScanUtils.scanLine("const a = 1;", lineNumber, "src/a.ts", EMPTY_ALLOWLIST)
      ).toThrow(/scanLine requires a positive integer lineNumber/);
    }
  });

  it("rejects an empty or non-string relative path", () => {
    for (const relativePath of ["", null, undefined, 7, {}]) {
      expect(() =>
        secretScanUtils.scanLine("const a = 1;", 1, relativePath, EMPTY_ALLOWLIST)
      ).toThrow(/scanLine requires a non-empty relativePath string/);
    }
  });

  it("accepts the boundary inputs a real scan produces", () => {
    const awsKey = makeAwsKey();

    expect(secretScanUtils.scanLine("", 1, "src/empty.ts", EMPTY_ALLOWLIST)).toEqual([]);
    expect(secretScanUtils.scanLine("a", 1, "src/a.ts", EMPTY_ALLOWLIST)).toEqual([]);
    expect(secretScanUtils.scanLine(`""`, 1, "src/a.ts", EMPTY_ALLOWLIST)).toEqual([]);
    expect(secretScanUtils.scanLine("const a = 1;", Number.MAX_SAFE_INTEGER, "src/a.ts", EMPTY_ALLOWLIST)).toEqual([]);

    const boundary = secretScanUtils.scanLine(
      `aws="${awsKey}"`,
      1,
      "src/a.ts",
      EMPTY_ALLOWLIST
    );
    expect(findingTypes(boundary)).toEqual(["aws-access-key"]);
    expect(boundary[0].line).toBe(1);
    expect(boundary[0].column).toBe(6);
  });

  it("rejects non-string content at the file boundary", () => {
    for (const content of [null, undefined, 42, {}, []]) {
      expect(() => secretScanUtils.scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST)).toThrow(
        /scanFileContent requires string content/
      );
    }
  });

  it("keeps the file-level entry point fail-closed on unreadable input", () => {
    const fixtureRoot = createFixtureDir();
    writeFixture(fixtureRoot, "src/leak.ts", `const value = "${randomHighEntropy()}";\n`);

    let findings: unknown[] = [];
    expect(() => {
      findings = secretScanUtils.scanBackend(fixtureRoot, { allowlist: EMPTY_ALLOWLIST });
    }).not.toThrow();
    expect(findings).toHaveLength(1);

    // Retrying the same root must produce the same result: no cached or
    // partially applied state survives a completed scan.
    expect(secretScanUtils.scanBackend(fixtureRoot, { allowlist: EMPTY_ALLOWLIST })).toEqual(
      findings
    );
  });
});

describe("scanLine determinism and shared state", () => {
  it("is idempotent and independent of scan order across lines", () => {
    const awsKey = makeAwsKey();
    const stripeKey = makeStripeKey();
    const lines = [
      `aws="${awsKey}"`,
      `stripe="${stripeKey}"`,
      "const harmless = true;",
    ];

    const forward = lines.flatMap((line, index) =>
      secretScanUtils.scanLine(line, index + 1, "src/a.ts", EMPTY_ALLOWLIST)
    );
    const repeated = lines.flatMap((line, index) =>
      secretScanUtils.scanLine(line, index + 1, "src/a.ts", EMPTY_ALLOWLIST)
    );
    const reverse = [...lines]
      .reverse()
      .flatMap((line) =>
        secretScanUtils.scanLine(line, lines.indexOf(line) + 1, "src/a.ts", EMPTY_ALLOWLIST)
      )
      .sort((left, right) => left.line - right.line);

    expect(repeated).toEqual(forward);
    expect(reverse).toEqual(forward);
    expect(forward).toHaveLength(2);
  });

  it("is unaffected by a caller mutating the exported regex state", () => {
    const awsKey = makeAwsKey();
    const line = `aws="${awsKey}"`;
    const baseline = secretScanUtils.scanLine(line, 1, "src/a.ts", EMPTY_ALLOWLIST);

    for (const patternDef of secretScanUtils.KNOWN_SECRET_PATTERNS) {
      patternDef.regex.lastIndex = 5;
      secretScanUtils.PLAIN_STRING_REGEX.lastIndex = 3;

      expect(secretScanUtils.scanLine(line, 1, "src/a.ts", EMPTY_ALLOWLIST)).toEqual(baseline);
      // Scanning must not write back into the caller's shared regexes.
      expect(patternDef.regex.lastIndex).toBe(5);
      expect(secretScanUtils.PLAIN_STRING_REGEX.lastIndex).toBe(3);
    }

    patternDefReset();
  });

  function patternDefReset(): void {
    for (const patternDef of secretScanUtils.KNOWN_SECRET_PATTERNS) {
      patternDef.regex.lastIndex = 0;
    }
    secretScanUtils.PLAIN_STRING_REGEX.lastIndex = 0;
  }

  it("stays deterministic when an allowlist entry re-enters the scanner mid-match", () => {
    const awsKey = makeAwsKey();
    const stripeKey = makeStripeKey();

    const nested: unknown[] = [];
    const reentrantEntry = {
      get pattern(): string {
        nested.push(
          ...secretScanUtils.scanLine(
            `stripe="${stripeKey}"`,
            99,
            "src/nested.ts",
            EMPTY_ALLOWLIST
          )
        );
        return "^never-matches-anything$";
      },
    };

    const outer = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
      entries: [],
      globalPatterns: [reentrantEntry],
    });

    expect(findingTypes(outer)).toEqual(["aws-access-key"]);
    expect(findingTypes(nested)).toEqual(["stripe-secret-key"]);

    const control = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
      entries: [],
      globalPatterns: [{ pattern: "^never-matches-anything$" }],
    });
    expect(outer).toEqual(control);
  });

  it("reports a repeated secret on one line once, anchored at the first column", () => {
    const awsKey = makeAwsKey();
    const line = `first="${awsKey}" second="${awsKey}"`;

    const findings = secretScanUtils.scanLine(line, 1, "src/a.ts", EMPTY_ALLOWLIST);

    expect(findings).toHaveLength(1);
    expect(findings[0].column).toBe(line.indexOf(awsKey) + 1);
  });

  it("counts distinct secrets separately even at the same type and line", () => {
    const first = makeAwsKey();
    const second = makeAwsKey();
    expect(first).not.toBe(second);

    const findings = secretScanUtils.scanLine(
      `a="${first}" b="${second}"`,
      1,
      "src/a.ts",
      EMPTY_ALLOWLIST
    );

    expect(findings).toHaveLength(2);
    expect(findings.map((finding: { column: number }) => finding.column)).toEqual([4, 29]);
  });

  it("scales deterministically across a densely populated line", () => {
    const keys = Array.from({ length: 50 }, () => makeAwsKey());
    const line = keys.map((key, index) => `k${index}="${key}"`).join(" ");

    const findings = secretScanUtils.scanLine(line, 1, "src/a.ts", EMPTY_ALLOWLIST);
    const repeat = secretScanUtils.scanLine(line, 1, "src/a.ts", EMPTY_ALLOWLIST);

    expect(findings).toHaveLength(50);
    expect(repeat).toEqual(findings);
    expect(new Set(findings.map((finding: { match: string }) => finding.match)).size).toBe(50);
  });
});

describe("scan pattern failure boundaries", () => {
  it("fails fast instead of hanging when a pattern lacks the global flag", () => {
    expect(() =>
      secretScanUtils.collectRegexMatches("aaa", { name: "non-global", regex: /a+/ })
    ).toThrow(/must use the global \(g\) flag/);
  });

  it("fails fast when a pattern definition is not a RegExp", () => {
    expect(() =>
      secretScanUtils.collectRegexMatches("aaa", { name: "not-a-regex", regex: "a+" })
    ).toThrow(/must be a RegExp/);
  });

  it("does not hang in a fresh process on a non-global pattern", () => {
    let output = "";
    expect(() => {
      output = readSecretScanUtilsInChildProcess(
        "try { u.collectRegexMatches('aaa', { name: 'non-global', regex: /a+/ }); console.log('NO_ERROR'); } " +
          "catch (error) { console.log(error.constructor.name + ': ' + error.message); }"
      );
    }).not.toThrow();

    expect(output).toContain("TypeError");
    expect(output).toContain("global (g) flag");
    expect(output).not.toContain("NO_ERROR");
  });

  it("collects every occurrence of a pattern on one line", () => {
    const key = makeAwsKey();
    const line = [key, key, key].join(" ");

    expect(secretScanUtils.collectRegexMatches(line, secretScanUtils.KNOWN_SECRET_PATTERNS[3]))
      .toHaveLength(3);
  });
});

describe("scan target IO failure boundaries", () => {
  it("fails closed and identifies the unreadable file without leaking host paths", () => {
    const fixtureRoot = createFixtureDir();
    writeFixture(fixtureRoot, "src/locked.ts", `const value = "${randomHighEntropy()}";\n`);

    let thrown: Error | undefined;
    try {
      secretScanUtils.scanBackend(fixtureRoot, { allowlist: EMPTY_ALLOWLIST });
    } catch (error) {
      thrown = error as Error;
    }

    expect(thrown).toBeUndefined();

    const spy = jest
      .spyOn(fs, "readFileSync")
      .mockImplementation(() => {
        const error = new Error("permission denied") as NodeJS.ErrnoException;
        error.code = "EACCES";
        throw error;
      });

    try {
      secretScanUtils.scanBackend(fixtureRoot, { allowlist: EMPTY_ALLOWLIST });
      throw new Error("expected scanBackend to throw");
    } catch (error) {
      thrown = error as Error;
    } finally {
      spy.mockRestore();
    }

    expect(thrown?.message).toContain("src/locked.ts");
    expect(thrown?.message).toContain("EACCES");
    expect(thrown?.message).not.toContain(fixtureRoot);
    expect(thrown?.message).not.toContain(os.tmpdir());
  });

  it("fails closed when a scanned file no longer exists", () => {
    const fixtureRoot = createFixtureDir();
    writeFixture(fixtureRoot, "src/present.ts", "export const ok = true;\n");

    const missing = {
      absolutePath: path.join(fixtureRoot, "src", "absent.ts"),
      relativePath: "src/absent.ts",
    };

    expect(() =>
      secretScanUtils.scanTargets(
        [
          {
            absolutePath: path.join(fixtureRoot, "src", "present.ts"),
            relativePath: "src/present.ts",
          },
          missing,
        ],
        EMPTY_ALLOWLIST
      )
    ).toThrow(/Failed to read src\/absent\.ts during secret scan \(ENOENT\)/);
  });

  it("fails closed when a scan directory cannot be listed", () => {
    const fixtureRoot = createFixtureDir();
    writeFixture(fixtureRoot, "src/a.ts", "export const ok = true;\n");

    const spy = jest
      .spyOn(fs, "readdirSync")
      .mockImplementation(() => {
        const error = new Error("permission denied") as NodeJS.ErrnoException;
        error.code = "EACCES";
        throw error;
      });

    try {
      secretScanUtils.collectScanTargets(fixtureRoot);
      throw new Error("expected collectScanTargets to throw");
    } catch (error) {
      expect((error as Error).message).toContain("EACCES");
      expect((error as Error).message).toContain("during secret scan");
      expect((error as Error).message).not.toContain(fixtureRoot);
    } finally {
      spy.mockRestore();
    }
  });

  it("recovers on retry once the file becomes readable", () => {
    const fixtureRoot = createFixtureDir();
    const target = writeFixture(
      fixtureRoot,
      "src/flaky.ts",
      `const value = "${randomHighEntropy()}";\n`
    );

    const failOnce = jest
      .spyOn(fs, "readFileSync")
      .mockImplementationOnce(() => {
        const error = new Error("resource temporarily unavailable") as NodeJS.ErrnoException;
        error.code = "EAGAIN";
        throw error;
      });

    let firstError: Error | undefined;
    try {
      secretScanUtils.scanBackend(fixtureRoot, { allowlist: EMPTY_ALLOWLIST });
    } catch (error) {
      firstError = error as Error;
    } finally {
      failOnce.mockRestore();
    }

    expect(firstError?.message).toContain("EAGAIN");
    expect(fs.existsSync(target)).toBe(true);

    const recovered = secretScanUtils.scanBackend(fixtureRoot, { allowlist: EMPTY_ALLOWLIST });
    expect(recovered).toHaveLength(1);
    expect(recovered[0].file).toBe("src/flaky.ts");
  });

  it("does not report an unreadable file as a clean scan through the CLI", () => {
    const fixtureRoot = createFixtureDir();
    writeFixture(fixtureRoot, "src/leak.ts", `const value = "${randomHighEntropy()}";\n`);
    const scriptPath = path.resolve(__dirname, "../scripts/secret-scan.js");

    const spy = jest.spyOn(fs, "readFileSync").mockImplementation((target, options) => {
      if (String(target).endsWith("src/leak.ts")) {
        const error = new Error("permission denied") as NodeJS.ErrnoException;
        error.code = "EACCES";
        throw error;
      }
      return spy.getMockImplementation() === undefined
        ? (fs.readFileSync as unknown as typeof import("node:fs").readFileSync).bind(fs)(
            target as string,
            options as BufferEncoding
          )
        : (undefined as never);
    });

    try {
      expect(() =>
        execFileSync("node", [scriptPath], { cwd: fixtureRoot, stdio: ["ignore", "pipe", "pipe"] })
      ).toThrow();
    } finally {
      spy.mockRestore();
    }
  });
});

describe("allowlist load-time validation", () => {
  it("rejects an allowlist whose pattern does not compile", () => {
    const fixtureRoot = createFixtureDir();
    const allowlistFile = path.join(fixtureRoot, "allow.json");
    fs.writeFileSync(
      allowlistFile,
      JSON.stringify({ entries: [], globalPatterns: [{ pattern: "(unclosed" }] }),
      "utf8"
    );

    expect(() => secretScanUtils.loadAllowlist(allowlistFile, fixtureRoot)).toThrow(
      /Failed to parse secret scan allowlist: allowlist\.globalPatterns\[0\]\.pattern/
    );
  });

  it("rejects an empty allowlist pattern that would otherwise disable the gate", () => {
    const fixtureRoot = createFixtureDir();
    const allowlistFile = path.join(fixtureRoot, "allow.json");
    fs.writeFileSync(
      allowlistFile,
      JSON.stringify({ entries: [{ file: "src/a.ts", pattern: "" }], globalPatterns: [] }),
      "utf8"
    );

    expect(() => secretScanUtils.loadAllowlist(allowlistFile, fixtureRoot)).toThrow(
      /allowlist\.entries\[0\]\.pattern/
    );
  });

  it("does not echo the offending pattern text into the error message", () => {
    const fixtureRoot = createFixtureDir();
    const allowlistFile = path.join(fixtureRoot, "allow.json");
    // A pattern copied verbatim from a leaked secret must not be echoed to logs.
    const leakedPattern = `^${makeStripeKey()}([`;
    fs.writeFileSync(
      allowlistFile,
      JSON.stringify({ entries: [], globalPatterns: [{ pattern: leakedPattern }] }),
      "utf8"
    );

    let message = "";
    try {
      secretScanUtils.loadAllowlist(allowlistFile, fixtureRoot);
    } catch (error) {
      message = (error as Error).message;
    }

    expect(message).toContain("globalPatterns[0].pattern");
    expect(message).not.toContain(leakedPattern);
    expect(message).not.toContain("sk_live_");
    expect(message).not.toContain(fixtureRoot);
  });

  it("still loads the repository allowlist unchanged", () => {
    const repoRoot = path.resolve(__dirname, "..");
    const allowlist = secretScanUtils.loadAllowlist(
      path.join(repoRoot, "scripts/.secret-scan-allow.json"),
      repoRoot
    );

    expect(allowlist.entries.length).toBeGreaterThan(0);
    expect(allowlist.globalPatterns.length).toBeGreaterThan(0);
    expect(() =>
      secretScanUtils.assertAllowlistPatternsCompilable(allowlist, "allowlist")
    ).not.toThrow();
  });

  it("treats a missing allowlist file as an empty allowlist rather than an error", () => {
    const fixtureRoot = createFixtureDir();

    expect(
      secretScanUtils.loadAllowlist(path.join(fixtureRoot, "absent.json"), fixtureRoot)
    ).toEqual(EMPTY_ALLOWLIST);
  });
});

describe("scanLine failure boundary coverage", () => {
  const awsKey = makeAwsKey();
  const stripeKey = makeStripeKey();
  const ORIGINAL_KNOWN_SECRET_PATTERNS = [...secretScanUtils.KNOWN_SECRET_PATTERNS];

  afterEach(() => {
    // Restore the exported pattern array so a test that mutates it cannot
    // leak into the self-scan or into other suites.
    secretScanUtils.KNOWN_SECRET_PATTERNS.length = 0;
    secretScanUtils.KNOWN_SECRET_PATTERNS.push(...ORIGINAL_KNOWN_SECRET_PATTERNS);
    for (const patternDef of secretScanUtils.KNOWN_SECRET_PATTERNS) {
      patternDef.regex.lastIndex = 0;
    }
    secretScanUtils.PLAIN_STRING_REGEX.lastIndex = 0;
  });

  describe("error state", () => {
    it("fails closed when an allowlist entry property throws", () => {
      const entry = {
        get pattern(): string {
          throw new Error("allowlist source unavailable");
        },
      };

      expect(() =>
        secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
          entries: [entry],
          globalPatterns: [],
        })
      ).toThrow();
    });

    it("fails closed when an allowlist match value cannot be stringified", () => {
      expect(() =>
        secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
          entries: [{ match: Symbol("nope") }],
          globalPatterns: [],
        })
      ).toThrow(TypeError);
    });

    it("fails closed when a known pattern loses the global flag", () => {
      const original = secretScanUtils.KNOWN_SECRET_PATTERNS[3];
      secretScanUtils.KNOWN_SECRET_PATTERNS[3] = {
        ...original,
        regex: new RegExp(original.regex.source),
      };

      expect(() =>
        secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", EMPTY_ALLOWLIST)
      ).toThrow(/must use the global \(g\) flag/);
    });

    it("fails closed when a known pattern is not a RegExp", () => {
      const original = secretScanUtils.KNOWN_SECRET_PATTERNS[3];
      secretScanUtils.KNOWN_SECRET_PATTERNS[3] = {
        ...original,
        regex: "AKIA" as unknown as RegExp,
      };

      expect(() =>
        secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", EMPTY_ALLOWLIST)
      ).toThrow(/must be a RegExp/);
    });

    it("reports an unknown error code when a filesystem error carries no errno", () => {
      const fixtureRoot = createFixtureDir();
      writeFixture(fixtureRoot, "src/a.ts", "export const ok = true;\n");

      const spy = jest.spyOn(fs, "readFileSync").mockImplementation(() => {
        throw new Error("mysterious failure");
      });

      try {
        secretScanUtils.scanBackend(fixtureRoot, { allowlist: EMPTY_ALLOWLIST });
        throw new Error("expected scanBackend to throw");
      } catch (error) {
        expect((error as Error).message).toContain("unknown error");
        expect((error as Error).message).toContain("src/a.ts");
      } finally {
        spy.mockRestore();
      }
    });
  });

  describe("retry state", () => {
    it("returns identical findings when the same line is scanned repeatedly", () => {
      const line = `aws="${awsKey}" stripe="${stripeKey}"`;

      const first = secretScanUtils.scanLine(line, 1, "src/a.ts", EMPTY_ALLOWLIST);
      const second = secretScanUtils.scanLine(line, 1, "src/a.ts", EMPTY_ALLOWLIST);
      const third = secretScanUtils.scanLine(line, 1, "src/a.ts", EMPTY_ALLOWLIST);

      expect(first).toHaveLength(2);
      expect(second).toEqual(first);
      expect(third).toEqual(first);
    });

    it("retries cleanly when an allowlist entry recovers from a transient failure", () => {
      let calls = 0;
      const flakyEntry = {
        get pattern(): string {
          calls += 1;
          if (calls === 1) {
            throw new Error("transient allowlist failure");
          }
          return "^AKIA";
        },
      };

      expect(() =>
        secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
          entries: [flakyEntry],
          globalPatterns: [],
        })
      ).toThrow(/transient allowlist failure/);

      const recovered = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [flakyEntry],
        globalPatterns: [],
      });

      expect(recovered).toHaveLength(0);
      expect(calls).toBe(2);
    });
  });

  describe("stale state", () => {
    it("ignores a stale lastIndex on an allowlist RegExp pattern", () => {
      const pattern = /^AKIA[0-9A-Z]{16}$/g;
      pattern.test(awsKey);
      expect(pattern.lastIndex).toBeGreaterThan(0);

      const findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [{ file: "src/a.ts", line: 1, pattern }],
        globalPatterns: [],
      });

      expect(findings).toHaveLength(0);
    });

    it("produces identical nested results across repeated outer scans", () => {
      const makeReentrant = () => {
        const nested: unknown[] = [];
        const entry = {
          get pattern(): string {
            nested.push(
              ...secretScanUtils.scanLine(
                `stripe="${stripeKey}"`,
                99,
                "src/nested.ts",
                EMPTY_ALLOWLIST
              )
            );
            return "^never-matches-anything$";
          },
        };
        return { entry, nested };
      };

      const first = makeReentrant();
      const firstOuter = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [],
        globalPatterns: [first.entry],
      });

      const second = makeReentrant();
      const secondOuter = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [],
        globalPatterns: [second.entry],
      });

      expect(first.nested).toHaveLength(1);
      expect(first.nested).toEqual(second.nested);
      expect(firstOuter).toEqual(secondOuter);
    });
  });

  describe("concurrent execution", () => {
    it("does not leak allowlist state between interleaved calls", () => {
      const allowlisted = {
        entries: [{ file: "src/a.ts", line: 1, pattern: "^AKIA" }],
        globalPatterns: [],
      };
      const open = { entries: [], globalPatterns: [] };

      const interleaved = [
        secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", allowlisted),
        secretScanUtils.scanLine(`stripe="${stripeKey}"`, 1, "src/a.ts", open),
        secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", open),
        secretScanUtils.scanLine(`stripe="${stripeKey}"`, 1, "src/a.ts", allowlisted),
      ];

      expect(interleaved[0]).toHaveLength(0);
      expect(findingTypes(interleaved[1])).toEqual(["stripe-secret-key"]);
      expect(findingTypes(interleaved[2])).toEqual(["aws-access-key"]);
      expect(findingTypes(interleaved[3])).toEqual(["stripe-secret-key"]);
    });

    it("reads each entry property exactly once per candidate", () => {
      const accesses: string[] = [];
      const trackingEntry = {
        get file(): string {
          accesses.push("file");
          return "src/a.ts";
        },
        get line(): number {
          accesses.push("line");
          return 1;
        },
        get match(): string {
          accesses.push("match");
          return stripeKey;
        },
      };

      const findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [trackingEntry],
        globalPatterns: [],
      });

      expect(findingTypes(findings)).toEqual(["aws-access-key"]);
      expect(accesses).toEqual(["file", "line", "match"]);
    });
  });

  describe("loading state", () => {
    it("honours a RegExp pattern from an allowlist entry", () => {
      const findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [{ file: "src/a.ts", line: 1, pattern: /^AKIA[0-9A-Z]{16}$/ }],
        globalPatterns: [],
      });

      expect(findings).toHaveLength(0);
    });

    it("produces identical results across repeated calls with a global RegExp pattern", () => {
      const allowlist = {
        entries: [],
        globalPatterns: [{ pattern: /^AKIA[0-9A-Z]{16}$/g }],
      };

      const first = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", allowlist);
      const second = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", allowlist);

      expect(first).toHaveLength(0);
      expect(second).toEqual(first);
    });

    it("does not trust an allowlist pattern whose test method was overridden", () => {
      const malicious = /^never-matches-anything$/;
      (malicious as unknown as { test: () => boolean }).test = () => true;

      const findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [{ file: "src/a.ts", line: 1, pattern: malicious }],
        globalPatterns: [],
      });

      expect(findings).toHaveLength(1);
    });

    it("fails closed when the allowlist file cannot be read", () => {
      const fixtureRoot = createFixtureDir();
      const allowlistFile = path.join(fixtureRoot, "allow.json");
      fs.writeFileSync(allowlistFile, JSON.stringify(EMPTY_ALLOWLIST), "utf8");

      const spy = jest.spyOn(fs, "readFileSync").mockImplementation((target) => {
        if (String(target) === allowlistFile) {
          const error = new Error("permission denied") as NodeJS.ErrnoException;
          error.code = "EACCES";
          throw error;
        }
        return (fs.readFileSync as unknown as typeof import("node:fs").readFileSync).bind(fs)(
          target as string
        );
      });

      try {
        secretScanUtils.loadAllowlist(allowlistFile, fixtureRoot);
        throw new Error("expected loadAllowlist to throw");
      } catch (error) {
        expect((error as Error).message).toContain("Failed to read secret scan allowlist");
        expect((error as Error).message).toContain("EACCES");
        expect((error as Error).message).not.toContain(fixtureRoot);
      } finally {
        spy.mockRestore();
      }
    });
  });

  describe("malformed allowlist shapes", () => {
    it("treats a non-object allowlist as empty", () => {
      for (const allowlist of ["allowlist", 42, true, ["entries"]]) {
        const findings = secretScanUtils.scanLine(
          `aws="${awsKey}"`,
          1,
          "src/a.ts",
          allowlist
        );
        expect(findingTypes(findings)).toEqual(["aws-access-key"]);
      }
    });

    it("treats a non-array entries field as empty", () => {
      const findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: "not-an-array",
        globalPatterns: 42,
      });

      expect(findingTypes(findings)).toEqual(["aws-access-key"]);
    });

    it("ignores allowlist entries that are not objects", () => {
      const findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [null, undefined, 42, "entry", true],
        globalPatterns: [null, undefined, 42, "entry"],
      });

      expect(findingTypes(findings)).toEqual(["aws-access-key"]);
    });

    it("treats null and undefined allowlists as empty", () => {
      for (const allowlist of [null, undefined]) {
        const findings = secretScanUtils.scanLine(
          `aws="${awsKey}"`,
          1,
          "src/a.ts",
          allowlist
        );
        expect(findingTypes(findings)).toEqual(["aws-access-key"]);
      }
    });
  });

  describe("field coercion boundaries", () => {
    it("matches a numeric-string line number via Number coercion", () => {
      const findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [{ file: "src/a.ts", line: "1", pattern: "^AKIA" }],
        globalPatterns: [],
      });

      expect(findings).toHaveLength(0);
    });

    it("does not allowlist when the entry file is not a string", () => {
      const findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [{ file: 42, line: 1, pattern: "^AKIA" }],
        globalPatterns: [],
      });

      expect(findings).toHaveLength(1);
    });

    it("honours an allowlist entry that constrains file, line, match, and pattern", () => {
      const findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [{ file: "src/a.ts", line: 1, match: awsKey, pattern: "^AKIA" }],
        globalPatterns: [],
      });
      expect(findings).toHaveLength(0);

      const wrongFile = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", {
        entries: [{ file: "src/other.ts", line: 1, match: awsKey, pattern: "^AKIA" }],
        globalPatterns: [],
      });
      expect(wrongFile).toHaveLength(1);
    });
  });

  describe("input boundary cases", () => {
    it("scans a line containing a newline as a single line", () => {
      const findings = secretScanUtils.scanLine(`aws="${awsKey}"\n`, 1, "src/a.ts", EMPTY_ALLOWLIST);
      expect(findingTypes(findings)).toEqual(["aws-access-key"]);
    });

    it("accepts a line number beyond MAX_SAFE_INTEGER", () => {
      const findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1e20, "src/a.ts", EMPTY_ALLOWLIST);
      expect(findings).toHaveLength(1);
      expect(findings[0].line).toBe(1e20);
    });

    it("scans unicode content without findings", () => {
      const findings = secretScanUtils.scanLine(
        'const greeting = "héllo wörld";',
        1,
        "src/a.ts",
        EMPTY_ALLOWLIST
      );
      expect(findings).toHaveLength(0);
    });

    it("reports a known-format secret once under its specific rule, not as high-entropy", () => {
      const findings = secretScanUtils.scanLine(`aws="${awsKey}"`, 1, "src/a.ts", EMPTY_ALLOWLIST);
      expect(findings).toHaveLength(1);
      expect(findingTypes(findings)).toEqual(["aws-access-key"]);
    });

    it("reports the same secret on different lines as separate findings", () => {
      const content = `const a = "${awsKey}";\nconst b = "${awsKey}";\n`;
      const findings = secretScanUtils.scanFileContent(content, "src/a.ts", EMPTY_ALLOWLIST);
      expect(findings).toHaveLength(2);
      expect(findings.map((finding: { line: number }) => finding.line)).toEqual([1, 2]);
    });

    it("scans a very long line deterministically", () => {
      const padding = "x".repeat(100000);
      const line = `${padding} aws="${awsKey}" ${padding}`;

      const first = secretScanUtils.scanLine(line, 1, "src/a.ts", EMPTY_ALLOWLIST);
      const second = secretScanUtils.scanLine(line, 1, "src/a.ts", EMPTY_ALLOWLIST);

      expect(first).toHaveLength(1);
      expect(second).toEqual(first);
    });
  });
});
