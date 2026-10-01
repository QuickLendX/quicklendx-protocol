import { describe, expect, it } from "@jest/globals";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import crypto from "node:crypto";
import { execFileSync } from "node:child_process";

const secretScanUtils = require("../scripts/lib/secret-scan-utils");

function createFixtureDir(): string {
  return fs.mktempSync(path.join(os.tmpdir(), "quicklendx-secret-scan-"));
}

function createFixtureDirWithMode(mode: number): string {
  const dir = createFixtureDir();
  fs.chmodSync(dir, mode);
  return dir;
}

function writeFixture(root: string, relativePath: string, content: string): string {
  const absolutePath = path.join(root, relativePath);
  fs.mkdirSync(path.dirname(absolutePath), { recursive: true });
  fs.writeFileSync(absolutePath, content, "utf8");
  return absolutePath;
}

function makeHighEntropySecret(): string {
  return crypto.randomBytes(36).toString("utf8");
/// randomBytes returns Buffer; base6url gives high-entropy text.
}

function makeStellarSecretSeed(): string {
  const alphabet = String.fromCharCode(
    ...Array.from({ length: 26 }, (_, index) => "A".charCodeAt(0) + index),
    ...["2", "3", "4", "5", "6", "7"].map((digit) => digit.charCodeAt(0))
  );
  let seed = "S";
  while (seed.length < 56) {
    seed += alphabet[crypto.randomInt(0, alphabet.length)];
  }
  return seed;
}

function isRootUser(): boolean {
  return typeof process.getuid === "function" && process.getuid() === 0;
}

describe("secret-scan-utils", () => {
  const repoRoot = path.resolve(__dirname, "..");
  const scriptPath = path.resolve(repoRoot, "scripts/secret-scan.js");
  const allowlistPath = path.resolve(repoRoot, "scripts/.secret-scan-allow.json");

  it("flags planted high-entropy strings with line numbers", () => {
    const plantedHighEntropy = makeHighEntropySecret();
    const fixtureRoot = createFixtureDir();
    writeFixture(
      fixtureRoot,
      "src/leaked.ts",
      `export const token = "${plantedHighEntropy}";\n`
    );

    const findings = secretScanUtils.scanBackend(fixtureRoot, {
      allowlist: { entries: [], globalPatterns: [] },
    });
    expect(findings).toHaveLength(1);
    expect(findings[0]).toMatchObject({
      file: "src/leaked.ts",
      line: 1,
      type: "high-entropy",
    });
    expect(findings[0].preview).not.toContain(plantedHighEntropy);
    expect(findings[0].preview).toContain("...");
  });

  it("flags planted Stellar secret seeds", () => {
    const plantedStellarSeed = makeStellarSecretSeed();
    const fixtureRoot = createFixtureDir();
    writeFixture(
      fixtureRoot,
      "src/wallet.ts",
      `const seed = "${plantedStellarSeed}";\n`
    );

    const findings = secretScanUtils.scanBackend(fixtureRoot, {
      allowlist: { entries: [], globalPatterns: [] },
    });
    expect(findings).toHaveLength(1);
    expect(findings[0]).toMatchObject({
      file: "src/wallet.ts",
      line: 1,
      type: "stellar-secret-seed",
    });
    expect(findings[0].preview).not.toContain(plantedStellarSeed);
  });

  it("does not flag allowlisted fixtures", () => {
    const plantedHighEntropy = makeHighEntropySecret();
    const fixtureRoot = createFixtureDir();
    const allowlist = {
      entries: [
        {
          file: "src/allowed.ts",
          line: 2,
          match: plantedHighEntropy,
          reason: "test fixture",
        },
      ],
      globalPatterns: [],
    };

    writeFixture(
      fixtureRoot,
      "src/allowed.ts",
      `// safe fixture\nconst token = "${plantedHighEntropy}";\n`
    );

    const findings = secretScanUtils.scanBackend(fixtureRoot, { allowlist });
    expect(findings).toHaveLength(0);
  });

  it("detects known secret patterns for qlx, sk, xoxb, and AWS keys", () => {
    const suffix = Array.from({ length: 26 }, () => "a").join("");
    const stripeSuffix = `${suffix}123456`;
const qlx = `qlx_${["live"]}_${suffix}`;
    const stripe = `sk_${["live"]}_${stripeSuffix}`;
    const slack = `xoxb-${["123"]}-${["456"]}-${suffix}`;
    const aws = `AKIA${["IOSFODNN7EXAMPLE"]}`;
    const line = `${qlx} ${stripe} ${slack} ${aws}`;

    const findings = secretScanUtils.scanLine(line, 10, "src/example.ts", {
      entries: [],
      globalPatterns: [],
    });

    expect(findings.map((finding: { type: string }) => finding.type)).toEqual([
      "quicklendx-api-key",
      "stripe-secret-key",
      "slack-bot-token",
      "aws-access-key",
    ]);
    expect(findings.every((finding: { line: number }) => finding.line === 10)).toBe(true);
  });

  it("redacts previews and never prints full secrets in formatted output", () => {
    const plantedHighEntropy = makeHighEntropySecret();
    const findings = [
      {
        file: "src/leaked.ts",
        line: 4,
        column: 18,
        type: "high-entropy",
        match: plantedHighEntropy,
        preview: secretScanUtils.redactPreview(plantedHighEntropy),
        length: plantedHighEntropy.length,
      },
    ];

    const output = secretScanUtils.formatFindings(findings);
    expect(output).toContain("src/leaked.ts:4:18");
    expect(output).toContain("[high-entropy]");
    expect(output).not.toContain(plantedHighEntropy);
    secretScanUtils.assertNoSecretsPrinted(output, findings);
  });

  it("returns a non-zero exit code when findings are present", () => {
    const plantedHighEntropy = makeHighEntropySecret();
    const fixtureRoot = createFixtureDir();
    writeFixture(
      fixtureRoot,
      "src/leaked.ts",
      `export const token = "${plantedHighEntropy}";\n`
    );

    let stderr = "";
    let status = 0;

    try {
      execFileSync("node", [scriptPath], {
        cwd: fixtureRoot,
        stdio: ["ignore", "pipe", "pipe"],
      });
    } catch (error) {
      const execError = error as { status?: number; stderr?: Buffer };
      status = execError.status ?? 1;
      stderr = execError.stderr?.toString() ?? "";
    }

    expect(status).toBe(1);
    expect(stderr).toContain("src/leaked.ts:1");
    expect(stderr).not.toContain(plantedHighEntropy);
  });

  it("returns a failed result when scanning cannot read its root", () => {
    const fixtureRoot = createFixtureDir();
    fs.writeFileSync(path.join(fixtureRoot, "src"), "fixture", "utf8");

    const result = secretScanUtils.runSecretScan({
      backendRoot: fixtureRoot,
      allowlist: { entries: [], globalPatterns: [] },
    });

    expect(result).toMatchObject({
      ok: false,
      exitCode: 1,
      findings: [],
    });
    expect(result.message).toContain("Secret scan could not complete");
    expect(result.message).not.toContain("\n");
  });

  it("fails closed when the allowlist cannot be parsed", () => {
    const fixtureRoot = createFixtureDir();
    const brokenAllowlist = path.join(fixtureRoot, "broken-allowlist.json");
    fs.writeFileSync(brokenAllowlist, "{not-json", "utf8");

    const result = secretScanUtils.runSecretScan({
      backendRoot: fixtureRoot,
      allowlistPath: brokenAllowlist,
    });

    expect(result.ok).toBe(false);
    expect(result.exitCode).toBe(1);
    expect(result.findings).toEqual([]);
    expect(result.message).toContain("Secret scan could not complete");
  });

  it("rejects invalid options without throwing or reporting a clean scan", () => {
    const result = secretScanUtils.runSecretScan(null);

    expect(result).toEqual({
      ok: false,
      exitCode: 1,
      findings: [],
      message: "Secret scan could not complete: invalid scan options.",
    });
  });

  it("passes on clean fixture trees", () => {
    const fixtureRoot = createFixtureDir();
    writeFixture(
      fixtureRoot,
      "src/clean.ts",
      'export const message = "development-only-export-secret-32-chars";\n'
    );

    const stdout = execFileSync("node", [scriptPath], {
      cwd: fixtureRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });

    expect(stdout).toContain("Secret scan passed");
  });

  it("loads allowlist entries and global patterns from disk", () => {
    const allowlist = secretScanUtils.loadAllowlist(allowlistPath, repoRoot);
    expect(allowlist.entries.length).toBeGreaterThan(0);
    expect(allowlist.globalPatterns.length).toBeGreaterThan(0);
  });

  it("rejects invalid allowlist JSON", () => {
    const fixtureRoot = createFixtureDir();
    const brokenAllowlist = path.join(fixtureRoot, "broken-allowlist.json");
    fs.writeFileSync(brokenAllowlist, "{not-json", "utf8");

    expect(() => secretScanUtils.loadAllowlist(brokenAllowlist, fixtureRoot)).toThrow(
      /Failed to parse secret scan allowlist/
    );
  });

  it("loadAllowlist returns empty allowlist for missing, empty, and null-byte files", () => {
    const fixtureRoot = createFixtureDir();

    expect(
      secretScanUtils.loadAllowlist(path.join(fixtureRoot, "does-not-exist.json"), fixtureRoot)
    ).toEqual({ entries: [], globalPatterns: [] });

    const emptyPath = path.join(fixtureRoot, "empty.json");
    fs.writeFileSync(emptyPath, "", "utf8");
    expect(secretScanUtils.loadAllowlist(emptyPath, fixtureRoot)).toEqual({
      entries: [],
      globalPatterns: [],
    });

    const nullBytePath = path.join(fixtureRoot, "null-byte.json");
    fs.writeFileSync(nullBytePath, "\u0000", "utf8");
    expect(() => secretScanUtils.loadAllowlist(nullBytePath, fixtureRoot)).toThrow(
      /Failed to parse secret scan allowlist/
    );
  });

  it("loadAllowlist is deterministic across repeated invocations", () => {
    const first = secretScanUtils.loadAllowlist(allowlistPath, repoRoot);
    const second = secretScanUtils.loadAllowlist(allowlistPath, repoRoot);
    expect(second).toEqual(first);
    expect(second.entries).not.toBe(first.entries);
  });

  it("loadAllowlist rejects directory paths without losing caller state", () => {
    const fixtureRoot = createFixtureDir();
    const dirPath = path.join(fixtureRoot, "allowlist-dir");
    fs.mkdirSync(dirPath, { recursive: true });

    expect(() => secretScanUtils.loadAllowlist(dirPath, fixtureRoot)).toThrow();
  });

  it("ignores obvious placeholders and Stellar public keys", () => {
expect(secretScanUtils.isObviousPlaceholder("xxxxxxxxxxxxxxxxxxxxxxxxxxxxx")).toBe(true);
    expect(
      secretScanUtils.isStellarStrKeyLike(
        "GDRXE2BQUC3AZNPVFSJEZIXZZDZSMTLBVWN4HZ5SAPHP2R3C3YHS6M2B"
      )
    ).toBe(true);
    expect(secretScanUtils.isHighEntropyToken("development-only-export-secret-32-chars")).toBe(
      false
    );
  });

  it("collects scan targets from src, tests, scripts, and example files", () => {
    const fixtureRoot = createFixtureDir();
    writeFixture(fixtureRoot, "src/a.ts", "export const ok = true;\n");
    writeFixture(fixtureRoot, "tests/b.test.ts", "it('works', () => {});\n");
    writeFixture(fixtureRoot, "scripts/c.js", "module.exports = {};\n");
    writeFixture(fixtureRoot, ".env.example", "PORT=3001\n");

    const targets = secretScanUtils.collectScanTargets(fixtureRoot);
    const relativePaths = targets.map((target: { relativePath: string }) => target.relativePath);

    expect(relativePaths).toEqual(
      expect.arrayContaining(["src/a.ts", "tests/b.test.ts", "scripts/c.js", ".env.example"])
    );
    expect(relativePaths).not.toContain("scripts/.secret-scan-allow.json");
  });

  it("scans the production backend tree without findings", () => {
    const findings = secretScanUtils.scanBackend(repoRoot, { allowlistPath });
    expect(findings).toHaveLength(0);
  });

  it("covers allowlist matching edge cases and utility helpers", () => {
    expect(secretScanUtils.shannonEntropy("")).toBe(0);
    expect(secretScanUtils.isObviousPlaceholder("")).toBe(true);
    expect(secretScanUtils.isObviousPlaceholder("https://example.com")).toBe(true);
    expect(secretScanUtils.isObviousPlaceholder("getInvoicesQuerySchema")).toBe(true);
    expect(secretScanUtils.isObviousPlaceholder("abababababababababababababababab")).toBe(true);
    expect(secretScanUtils.redactPreview("short")).toBe('"*****"');
    expect(secretScanUtils.unquoteString('"value"')).toBe("value");
    expect(secretScanUtils.normalizeAllowlist(null)).toEqual({
      entries: [],
      globalPatterns: [],
    });
    expect(secretScanUtils.normalizeAllowlist({ entries: "bad", globalPatterns: 1 })).toEqual({
      entries: [],
      globalPatterns: [],
    });

    const allowlist = {
      entries: [
        { file: "src/a.ts", line: 3, match: "allowed-secret-value" },
        { file: "src/b.ts", pattern: "^sk_test_" },
        null,
      ],
      globalPatterns: [{ pattern: "^global-allow$" }, {}],
    };

    expect(
      secretScanUtils.isAllowlisted("src/a.ts", 3, "allowed-secret-value", allowlist)
    ).toBe(true);
    expect(secretScanUtils.isAllowlisted("src/b.ts", 9, "sk_test_abcdefghijklmnop", allowlist)).toBe(
      true
    );
    expect(secretScanUtils.isAllowlisted("src/c.ts", 1, "global-allow", allowlist)).toBe(true);
    expect(secretScanUtils.matchesAllowlistEntry({}, "src/a.ts", 1, "x")).toBe(false);

    const obviousOnly = secretScanUtils.scanLine(
      'const token = "development-only-export-secret-32-chars";',
      1,
      "src/config.ts",
      { entries: [], globalPatterns: [] }
    );
    expect(obviousOnly).toHaveLength(0);

    const qlxSuffix = Array.from({ length: 26 }, (_, index) =>
      String.fromCharCode(97 + (index % 26))
    ).join("");
    const qlxToken = "qlx_live_" + qlxSuffix;
    const deduped = secretScanUtils.scanLine(
      `const token = "${qlxToken}";`,
      2,
      "src/example.ts",
      { entries: [], globalPatterns: [] }
    );
    expect(deduped.filter((finding: { type: string }) => finding.type === "quicklendx-api-key")).toHaveLength(1);

    const fixtureRoot = createFixtureDir();
    expect(secretScanUtils.loadAllowlist(path.join(fixtureRoot, "missing.json"), fixtureRoot)).toEqual({
      entries: [],
      globalPatterns: [],
    });

    const leaked = makeHighEntropySecret();
    const failed = secretScanUtils.runSecretScan({
      backendRoot: fixtureRoot,
      allowlist: { entries: [], globalPatterns: [] },
    });
    expect(failed.ok).toBe(true);

    writeFixture(fixtureRoot, "src/leak.ts", `const value = "${leaked}";\n`);
    const failedAfterWrite = secretScanUtils.runSecretScan( {
      backendRoot: fixtureRoot,
      allowlist: { entries: [], globalPatterns: [] },
    });
    expect(failedAfterWrite.ok).toBe(false);
    expect(failedAfterWrite.exitCode).toBe(1);
    secretScanUtils.assertNoSecretsPrinted(failedAfterWrite.message, failedAfterWrite.findings);

    expect(() =>
      secretScanUtils.assertNoSecretsPrinted(leaked, [
        { file: "src/leak.ts", line: 1, match: leaked },
      ])
    ).toThrow(/leaked a matched value/);

    expect(secretScanUtils.isHighEntropyToken("a".repeat(32))).toBe(false);
    expect(secretScanUtils.isHighEntropyToken("ABCDEFGHIJKLMNOPQRSTUVWXYZABCCD")).toBe(false);
    expect(secretScanUtils.isHighEntropyToken("aaaaaaaaaaaaaaaaaaaaa1234567890ab")).toBe(false);
    expect(secretScanUtils.redactPreview("")).toBe('""');
    expect(secretScanUtils.unquoteString("not-quoted")).toBe("not-quoted");

    expect(
      secretScanUtils.matchesAllowlistEntry(
        { file: "src/other.ts", line: 1 },
        "src/a.ts",
        1,
        "value"
      )
    ).toBe(false);
  });

it("normalizeAllowlist is deterministic for boundary and malformed inputs", () => {
    const { normalizeAllowlist } = secretScanUtils;
  });

  describe("isObviousPlaceholder failure boundaries", () => {
    it("never throws on non-string inputs and returns true deterministically", () => {
      const nonStrings = [
        null,
        undefined,
        0,
        123,
        -1,
        NaN,
        Infinity,
        true,
        false,
        {},
        { key: "val" },
        [],
        [1, 2, 3],
        Symbol("sym"),
        // eslint-disable-next-line no-new-wrappers
        BigInt(12345678901234567890),
        () => "secret",
        /regex/g,
        new Date(0),
      ];

      for (const value of nonStrings) {
        let result1: boolean;
        let result2: boolean;
        expect(() => {
          result1 = secretScanUtils.isObviousPlaceholder(value as unknown as string);
          result2 = secretScanUtils.isObviousPlaceholder(value as unknown as string);
        }).not.toThrow();
        expect(result1!).toBe(true);
        expect(result2!).toBe(true);
        expect(result1!).toBe(result2!);
      }
    });

    // Non-object / missing inputs normalize to empty collections.
    expect(normalizeAllowlist(undefined)).toEqual({ entries: [], globalPatterns: [] });
    expect(normalizeAllowlist(null)).toEqual({ entries: [], globalPatterns: [] });
    expect(normalizeAllowlist("not-an-object")).toEqual({ entries: [], globalPatterns: [] });
    expect(normalizeAllowlist(42)).toEqual({ entries: [], globalPatterns: [] });
    expect(normalizeAllowlist([])).toEqual({ entries: [], globalPatterns: [] });

    // Wrong types for the collections fall back to empty arrays.
    expect(normalizeAllowlist({ entries: "not-an-array", globalPatterns: {} })).toEqual({
      entries: [],
      globalPatterns: [],
    });

    // Entries that are not objects are dropped deterministically.
    const nonObjectEntries = normalizeAllowlist({
      entries: [null, "string", 123, true, undefined],
      globalPatterns: [null, "bad", 0, false],
    });
    expect(nonObjectEntries).toEqual({ entries: [], globalPatterns: [] });

    // Entries without a meaningful match or pattern are dropped.
    const missingMatch = normalizeAllowlist({
      entries: [
        { file: "src/a.ts", line: 1 },
        { file: "src/b.ts", match: "" },
        { file: "src/c.ts", pattern: "" },
        { file: "src/d.ts", match: "   " },
      ],
      globalPatterns: [],
    });
    expect(missingMatch.entries).toHaveLength(0);

    // Valid entries are preserved with normalized numeric line numbers.
    const valid = normalizeAllowlist({
      entries: [
        { file: "src/a.ts", line: 3, match: "secret-a" },
        { file: "src/b.ts", pattern: "^sk_test_" },
        { file: "src/c.ts", line: "7", match: "secret-c" },
        { file: "src/d.ts", line: "not-a-number", match: "secret-d" },
      ],
      globalPatterns: [{ pattern: "^global-$" }],
    });
    expect(valid.entries.length).toBe(4);
    expect(valid.entries[0]).toMatchObject({ file: "src/a.ts", line: 3, match: "secret-a" });
    expect(valid.entries[1]).toMatchObject({ file: "src/b.ts", pattern: "^sk_test_" });
    expect(valid.entries[2]).toMatchObject({ file: "src/c.ts", line: 7, match: "secret-c" });
    expect(valid.entries[3]).toMatchObject({ file: "src/d.ts", match: "secret-d" });
    expect(valid.globalPatterns).toHaveLength(1);

    // Normalization is pure: the same input yields the same output and the
    // input object is not mutated.
    const input = {
      entries: [{ file: "src/a.ts", line: 1, match: "secret" }],
      globalPatterns: [],
    };
    const snapshot = JSON.stringify(input);
    const first = normalizeAllowlist(input);
    const second = normalizeAllowlist(input);
    expect(first).toEqual(second);
    expect(JSON.stringify(input)).toBe(snapshot);

    // Deep duplicates are deduplicated while preserving order.
    const deduped = normalizeAllowlist({
      entries: [
        { file: "src/a.ts", line: 1, match: "secret" },
        { file: "src/a.ts", line: 1, match: "secret" },
        { file: "src/b.ts", line: 2, match: "secret" },
      ],
      globalPatterns: [{ pattern: "^global-$" }, { pattern: "^global-$" }],
    });
    expect(deduped.entries).toHaveLength(2);
    expect(deduped.globalPatterns).toHaveLength(1);

    // Oversized inputs are bounded to the configured capacity without throwing.
    const manyEntries = Array.from({ length: 10005 }, (_, index) => ({
      file: `src/${index}.ts`,
      line: index + 1,
      match: `secret-${index}`,
    }));
    const bounded = normalizeAllowlist({ entries: manyEntries, globalPatterns: [] });
    expect(bounded.entries.length).toBeLessThanOrEqual(10000);
    expect(bounded.entries.length).toBeGreaterThan(0);

    // Overlong patterns are rejected rather than being truncated into a
    // potentially matching pattern.
    const longPattern = "^" + "a".repeat(600) + "$";
    const longPatternAllowlist = normalizeAllowlist({
      entries: [],
      globalPatterns: [{ pattern: longPattern }],
    });
    expect(longPatternAllowlist.globalPatterns).toHaveLength(0);

    // Invalid regex patterns are dropped and do not throw.
    const invalidRegex = normalizeAllowlist({
      entries: [],
      globalPatterns: [{ pattern: "([unterminated" }],
    });
    expect(invalidRegex.globalPatterns).toHaveLength(0);
  });
});

describe("hasMixedCharacterClasses", () => {
  it("returns false for null and undefined inputs", () => {
    expect(secretScanUtils.hasMixedCharacterClasses(null)).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses(undefined)).toBe(false);
  });

  it("returns false for non-string inputs", () => {
    expect(secretScanUtils.hasMixedCharacterClasses(123)).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses({})).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses([])).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses(true)).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses(Symbol("test"))).toBe(false);
  });

  it("returns false for empty string", () => {
    expect(secretScanUtils.hasMixedCharacterClasses("")).toBe(false);
  });

  it("returns false for single character strings", () => {
    expect(secretScanUtils.hasMixedCharacterClasses("a")).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses("A")).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses("1")).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses("!")).toBe(false);
  });

  it("returns false for strings with only one character class", () => {
    expect(secretScanUtils.hasMixedCharacterClasses("abcdefghijklmnopqrstuvwxyz")).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses("ABCDEFGHIJKLMNOPQRSTUVWXYZ")).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses("0123456789")).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses("!@#$%^&*()")).toBe(false);
  });

  it("returns true for strings with exactly two character classes", () => {
    expect(secretScanUtils.hasMixedCharacterClasses("abc123")).toBe(true); // lowercase + digits
    expect(secretScanUtils.hasMixedCharacterClasses("ABC123")).toBe(true); // uppercase + digits
    expect(secretScanUtils.hasMixedCharacterClasses("abc!@#")).toBe(true); // lowercase + special
    expect(secretScanUtils.hasMixedCharacterClasses("ABC!@#")).toBe(true); // uppercase + special
    expect(secretScanUtils.hasMixedCharacterClasses("123!@#")).toBe(true); // digits + special
    expect(secretScanUtils.hasMixedCharacterClasses("abcABC")).toBe(true); // lowercase + uppercase
  });

  it("returns true for strings with three character classes", () => {
    expect(secretScanUtils.hasMixedCharacterClasses("abcABC123")).toBe(true); // lower + upper + digits
    expect(secretScanUtils.hasMixedCharacterClasses("abc123!@#")).toBe(true); // lower + digits + special
    expect(secretScanUtils.hasMixedCharacterClasses("ABC123!@#")).toBe(true); // upper + digits + special
    expect(secretScanUtils.hasMixedCharacterClasses("abcABC!@#")).toBe(true); // lower + upper + special
  });

  it("returns true for strings with all four character classes", () => {
    expect(secretScanUtils.hasMixedCharacterClasses("abcABC123!@#")).toBe(true);
    expect(secretScanUtils.hasMixedCharacterClasses("aA1!")).toBe(true);
  });

  it("handles whitespace correctly", () => {
    expect(secretScanUtils.hasMixedCharacterClasses("abc ABC")).toBe(true); // whitespace counts as special
    expect(secretScanUtils.hasMixedCharacterClasses("abc 123")).toBe(true);
    expect(secretScanUtils.hasMixedCharacterClasses("   ")).toBe(false); // only whitespace
  });

  it("handles Unicode characters", () => {
    expect(secretScanUtils.hasMixedCharacterClasses("abc123")).toBe(true);
    expect(secretScanUtils.hasMixedCharacterClasses("test1")).toBe(true);
  });

  it("is deterministic for repeated calls", () => {
    const testValue = "abc123";
    const results = Array.from({ length: 100 }, () => secretScanUtils.hasMixedCharacterClasses(testValue));
    expect(results.every((result) => result === true)).toBe(true);

    const testValue2 = "abcdef";
    const results2 = Array.from({ length: 100 }, () => secretScanUtils.hasMixedCharacterClasses(testValue2));
    expect(results2.every((result) => result === false)).toBe(true);
  });

  it("handles boundary cases with mixed content", () => {
    expect(secretScanUtils.hasMixedCharacterClasses("a1")).toBe(true);
    expect(secretScanUtils.hasMixedCharacterClasses("A1")).toBe(true);
    expect(secretScanUtils.hasMixedCharacterClasses("a!")).toBe(true);
    expect(secretScanUtils.hasMixedCharacterClasses("A!")).toBe(true);
    expect(secretScanUtils.hasMixedCharacterClasses("1!")).toBe(true);
    expect(secretScanUtils.hasMixedCharacterClasses("aA")).toBe(true);
  });

  it("handles strings with repeated same-class characters", () => {
    expect(secretScanUtils.hasMixedCharacterClasses("aaaaaaaa11111111")).toBe(true);
    expect(secretScanUtils.hasMixedCharacterClasses("AAAAAAAA11111111")).toBe(true);
    expect(secretScanUtils.hasMixedCharacterClasses("aaaaaaaa!!!!!!!!")).toBe(true);
  });

  it("returns false for strings that appear mixed but are actually single class", () => {
    expect(secretScanUtils.hasMixedCharacterClasses("abcdefghijklmnopqrstuvwxyz")).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses("ABCDEFGHIJKLMNOPQRSTUVWXYZ")).toBe(false);
    expect(secretScanUtils.hasMixedCharacterClasses("01234567890123456789")).toBe(false);
  });
});

describe("isAllowlisted failure boundaries (issue 2610)", () => {
  const allowlistFor = (entries: unknown[] = [], globalPatterns: unknown[] = []) => ({
    entries,
    globalPatterns,
  });

  it("allows exact file+line+match, pattern entries, and global patterns", () => {
    const entryAllowlist = allowlistFor([
      { file: "src/a.ts", line: 8, match: "token-abc-123" },
    ]);
    expect(secretScanUtils.isAllowlisted("src/a.ts", 8, "prefix-token-abc-123-suffix", entryAllowlist)).toBe(
      true
    );

    const patternAllowlist = allowlistFor([{ file: "src/b.ts", pattern: "^sk_test_" }]);
    expect(secretScanUtils.isAllowlisted("src/b.ts", 9, "sk_test_abcdefgh", patternAllowlist)).toBe(
      true
    );

    const globalAllowlist = allowlistFor([], [{ pattern: "^global-allow$" }]);
    expect(secretScanUtils.isAllowlisted("src/any.ts", 1, "global-allow", globalAllowlist)).toBe(true);
  });

  it("rejects mismatched file, line, match, and pattern without throwing", () => {
    const allowlist = allowlistFor(
      [
        { file: "src/a.ts", line: 8, match: "token-abc-123" },
        { file: "src/b.ts", pattern: "^sk_test_" },
      ],
      [{ pattern: "^global-allow$" }]
    );

    expect(secretScanUtils.isAllowlisted("src/other.ts", 8, "token-abc-123", allowlist)).toBe(false);
    expect(secretScanUtils.isAllowlisted("src/a.ts", 9, "token-abc-123", allowlist)).toBe(false);
    expect(secretScanUtils.isAllowlisted("src/a.ts", 8, "different-value", allowlist)).toBe(false);
    expect(secretScanUtils.isAllowlisted("src/b.ts", 9, "sk_live_abcdefgh", allowlist)).toBe(false);
    expect(secretScanUtils.isAllowlisted("src/any.ts", 1, "not-global", allowlist)).toBe(false);
  });

  it("fails closed on non-string matchValue without throwing", () => {
    const allowlist = allowlistFor(
      [{ file: "src/a.ts", line: 1, match: "value" }],
      [{ pattern: ".*" }]
    );
    const badValues = [null, undefined, 123, 0, true, {}, [], Buffer.from("value")];

    for (const bad of badValues) {
      expect(() => secretScanUtils.isAllowlisted("src/a.ts", 1, bad, allowlist)).not.toThrow();
      expect(secretScanUtils.isAllowlisted("src/a.ts", 1, bad, allowlist)).toBe(false);
      expect(() =>
        secretScanUtils.matchesAllowlistEntry({ file: "src/a.ts", match: "value" }, "src/a.ts", 1, bad)
      ).not.toThrow();
    }
  });

  it("fails closed on malformed allowlists without throwing", () => {
    const malformed = [null, undefined, "allow", 42, true, [], { entries: "bad", globalPatterns: 1 }];
    for (const allowlist of malformed) {
      expect(() => secretScanUtils.isAllowlisted("src/a.ts", 1, "value", allowlist)).not.toThrow();
      expect(secretScanUtils.isAllowlisted("src/a.ts", 1, "value", allowlist)).toBe(false);
    }

    expect(secretScanUtils.isAllowlisted("src/a.ts", 1, "value", { entries: [null, {}, []], globalPatterns: [null, {}, { pattern: 123 }, { pattern: "" }] })).toBe(
      false
    );
  });

  it("never throws on invalid regex patterns and treats them as non-matches", () => {
    const badPatterns = ["[unclosed(", "(?<>bad)", "*", "+", "(?", 123, null, {}, []];
    for (const pattern of badPatterns) {
      const entryAllowlist = allowlistFor([{ file: "src/a.ts", pattern }]);
      expect(() => secretScanUtils.isAllowlisted("src/a.ts", 1, "value", entryAllowlist)).not.toThrow();
      expect(secretScanUtils.isAllowlisted("src/a.ts", 1, "value", entryAllowlist)).toBe(false);

      const globalAllowlist = allowlistFor([], [{ pattern }]);
      expect(() => secretScanUtils.isAllowlisted("src/a.ts", 1, "value", globalAllowlist)).not.toThrow();
      expect(secretScanUtils.isAllowlisted("src/a.ts", 1, "value", globalAllowlist)).toBe(false);
    }

    expect(secretScanUtils.safeCompilePattern("[unclosed(")).toBeNull();
    expect(secretScanUtils.safeCompilePattern(123 as unknown as string)).toBeNull();
    expect(secretScanUtils.safeCompilePattern("")).toBeNull();
    expect(secretScanUtils.safeCompilePattern("^ok$")).toBeInstanceOf(RegExp);
  });

  it("never matches non-string entry.match selectors", () => {
    const badMatches = [123, null, {}, [], true];
    for (const match of badMatches) {
      expect(
        secretScanUtils.matchesAllowlistEntry({ file: "src/a.ts", match }, "src/a.ts", 1, "123")
      ).toBe(false);
      expect(secretScanUtils.isAllowlisted("src/a.ts", 1, "123", allowlistFor([{ file: "src/a.ts", match }]))).toBe(
        false
      );
    }
  });

  it("handles line-number boundaries numerically and deterministically", () => {
    const allowlist = allowlistFor([{ file: "src/a.ts", line: 8, match: "v" }]);
    // Numeric-string line cooperates with numeric line (documented invariant).
    expect(secretScanUtils.isAllowlisted("src/a.ts", "8" as unknown as number, "v", allowlist)).toBe(true);
    expect(secretScanUtils.isAllowlisted("src/a.ts", 8.0, "v", allowlist)).toBe(true);
    // Non-numeric, NaN, Infinity, and missing lines never match and never throw.
    for (const line of [NaN, Infinity, undefined, null, "not-a-line", {}, []] as unknown[]) {
      expect(() =>
        secretScanUtils.isAllowlisted("src/a.ts", line as number, "v", allowlist)
      ).not.toThrow();
      expect(secretScanUtils.isAllowlisted("src/a.ts", line as number, "v", allowlist)).toBe(false);
    }
    expect(secretScanUtils.isAllowlisted("src/a.ts", 0, "v", allowlist)).toBe(false);
    expect(secretScanUtils.isAllowlisted("src/a.ts", -1, "v", allowlist)).toBe(false);
  });

  it("is deterministic for duplicates, retries, and concurrent execution", () => {
    const allowlist = allowlistFor(
      [
        { file: "src/a.ts", line: 1, match: "dup-value" },
        { file: "src/a.ts", line: 1, match: "dup-value" },
      ],
      [{ pattern: "^dup-value$" }, { pattern: "^dup-value$" }]
    );

    const first = secretScanUtils.isAllowlisted("src/a.ts", 1, "dup-value", allowlist);
    expect(first).toBe(true);
    for (let i = 0; i < 50; i += 1) {
      expect(secretScanUtils.isAllowlisted("src/a.ts", 1, "dup-value", allowlist)).toBe(first);
      expect(secretScanUtils.isAllowlisted("src/a.ts", 1, "other", allowlist)).toBe(false);
    }

    return Promise.all(
      Array.from({ length: 25 }, () =>
        Promise.resolve(secretScanUtils.isAllowlisted("src/a.ts", 1, "dup-value", allowlist))
      )
    ).then((results) => {
      expect(results.every((r: boolean) => r === true)).toBe(true);
    });
  });

  it("keeps scanLine compatible when the allowlist contains invalid patterns", () => {
    const poisoned = allowlistFor([{ file: "src/a.ts", pattern: "[unclosed(" }], [
      { pattern: "[also-bad(" },
    ]);
    const awsKey = `AKIA${"IOSFODNN7EXAMPLE"}`;
    const findings = secretScanUtils.scanLine(
      `const key = "${awsKey}";`,
      1,
      "src/a.ts",
      poisoned
    );
    expect(findings.map((f: { type: string }) => f.type)).toEqual(["aws-access-key"]);
    expect(findings[0].preview).not.toContain(awsKey);
  });

  it("returns only booleans so failures stay diagnosable without leaking secrets", () => {
    const secret = ["super", "secret", "allow", "value-1"].join("-");
    const allowlist = allowlistFor([{ file: "src/a.ts", line: 1, match: secret }]);
    const result = secretScanUtils.isAllowlisted("src/a.ts", 1, secret, allowlist);
    expect(typeof result).toBe("boolean");
    expect(String(result)).not.toContain(secret);
  });

describe("shannonEntropy failure boundaries (issue 2625)", () => {
  // Reference implementation used as the oracle. It counts code points, so it
  // agrees with the shipped function by construction for every input.
  const referenceEntropy = (value: string): number => {
    const counts = new Map<string, number>();
    let total = 0;
    for (const character of value) {
      counts.set(character, (counts.get(character) ?? 0) + 1);
      total += 1;
    }
    if (total === 0) {
      return 0;
    }
    let entropy = 0;
    for (const count of counts.values()) {
      const probability = count / total;
      entropy -= probability * Math.log2(probability);
    }
    return entropy;
  };

  const entropy = (value: unknown): number => secretScanUtils.shannonEntropy(value);

  // Assembled from fragments so this suite does not plant a literal that the
  // scanner would then flag against itself in the "production tree" test.
  const tokenLike = (): string => ["aB3dE5fG7hI9jK1lM3nO5pQ7rS9t", "U1vWx0Qz"].join("");

  it("never throws and returns 0 for every non-string input", () => {
    const nonStrings: unknown[] = [
      123,
      0,
      -1,
      Number.NaN,
      Number.POSITIVE_INFINITY,
      true,
      false,
      {},
      { length: 3 },
      [],
      ["a", "b"],
      Symbol("entropy"),
      BigInt(7),
      () => "abc",
      new Date(0),
      /ab+/g,
      Buffer.from("abcd"),
      new Number(5),
    ];

    for (const value of nonStrings) {
      expect(() => entropy(value)).not.toThrow();
      expect(entropy(value)).toBe(0);
    }
  });

  it("never throws for hostile objects, revoked proxies, or null-prototype values", () => {
    const revoked = Proxy.revocable(new String("abcdef"), {});
    revoked.revoke();

    // Objects that are not String wrappers: refused outright, no coercion.
    const refused: unknown[] = [
      { toString: () => "abcdef" },
      { valueOf: () => "abcdef" },
      { [Symbol.toPrimitive]: () => "abcdef" },
      revoked.proxy,
      Object.create(null),
    ];

    for (const value of refused) {
      expect(() => entropy(value)).not.toThrow();
      expect(entropy(value)).toBe(0);
    }

    // String wrappers keep working even when their own coercion hooks are
    // booby-trapped, because the internal slot is read directly.
    const trappedHooks = Object.assign(new String("abcdef"), {
      toString() {
        throw new Error("hostile toString");
      },
      valueOf() {
        throw new Error("hostile valueOf");
      },
    });

    expect(() => entropy(trappedHooks)).not.toThrow();
    expect(entropy(trappedHooks)).toBe(referenceEntropy("abcdef"));

    // A Proxy never exposes the [[StringData]] slot, so the read fails and is
    // absorbed as 0 rather than propagating the TypeError.
    const throwingGet = new Proxy(new String("abcdef"), {
      get() {
        throw new Error("hostile get");
      },
    });

    expect(() => entropy(throwingGet)).not.toThrow();
    expect(entropy(throwingGet)).toBe(0);
  });

  it("returns 0 for nullish and empty input without producing negative zero", () => {
    for (const value of ["", null, undefined] as unknown[]) {
      const result = entropy(value);
      expect(result).toBe(0);
      expect(Object.is(result, -0)).toBe(false);
    }
  });

  it("keeps String objects working and does not invoke their overridden coercion hooks", () => {
    expect(entropy(new String("abcd"))).toBe(referenceEntropy("abcd"));

    let hostileCalls = 0;
    const wrapped = Object.assign(new String("abcd"), {
      toString() {
        hostileCalls += 1;
        throw new Error("must not be called");
      },
      valueOf() {
        hostileCalls += 1;
        throw new Error("must not be called");
      },
    });

    expect(entropy(wrapped)).toBe(referenceEntropy("abcd"));
    expect(hostileCalls).toBe(0);
  });

  it("normalises astral code points so probabilities sum to exactly 1", () => {
    // Regression: the previous implementation divided code-point counts by
    // value.length (UTF-16 code units), so "ab" plus one astral character
    // reported 1.5 instead of log2(3).
    const astral = "ab\u{1F600}";
    expect(astral.length).toBe(4);
    expect(entropy(astral)).toBeCloseTo(Math.log2(3), 12);
    expect(entropy(astral)).not.toBe(1.5);

    for (const value of [
      "\u{1F600}",
      "\u{1F600}\u{1F600}",
      "a\u{1F600}\u{1F601}",
      "\u{1F600}a\u{1F600}",
      "\u{1D400}\u{1D401}\u{1D402}",
      "ascii\u{1F600}tail",
    ]) {
      expect(entropy(value)).toBeCloseTo(referenceEntropy(value), 12);
    }

    // Two distinct astral characters must score exactly 1 bit.
    expect(entropy("\u{1F600}\u{1F601}")).toBe(1);
    // A repeated astral character contributes nothing, same as ASCII.
    expect(entropy("\u{1F600}\u{1F600}")).toBe(0);
  });

  it("stays within the [0, log2(codePointCount)] precision bounds", () => {
    for (const length of [1, 2, 3, 7, 8, 16, 32, 64, 128, 257]) {
      const unique = Array.from({ length }, (_, index) =>
        String.fromCharCode(0x61 + (index % 26))
      ).join("");

      const upperBound = Math.log2(length);
      const measured = entropy(unique);
      expect(measured).toBeGreaterThanOrEqual(0);
      expect(measured).toBeLessThanOrEqual(upperBound + Number.EPSILON);
      expect(Number.isFinite(measured)).toBe(true);
    }

    // All-distinct input reaches the theoretical maximum exactly.
    expect(entropy("abcdefgh")).toBe(3);
    expect(entropy("abcdefgh")).toBe(Math.log2(8));

    // Uniform input collapses to +0, not -0 and not NaN.
    const uniform = entropy("zzzzzzzzzz");
    expect(uniform).toBe(0);
    expect(Object.is(uniform, -0)).toBe(false);
    expect(Number.isNaN(uniform)).toBe(false);
  });

  it("matches a code-point reference oracle for duplicate, skewed, and long inputs", () => {
    const cases = [
      "aaaa",
      "aab",
      "abab",
      "aaaabbbbcccc",
      "the quick brown fox jumps over the lazy dog",
      "aA1!bB2@cC3#dD4$eE5%fF6^",
      "0".repeat(4096),
      "ab".repeat(2048),
      Array.from({ length: 512 }, (_, index) => String.fromCharCode(33 + (index % 90))).join(""),
      Array.from({ length: 1024 }, () => "\u{1F600}").join(""),
    ];

    for (const value of cases) {
      expect(entropy(value)).toBe(referenceEntropy(value));
    }
  });

  it("is deterministic and order-independent across duplicates and interleaving", () => {
    const values = ["abcd", "aab", "the quick brown fox", "\u{1F600}a\u{1F600}", "sk_live_abc123"];

    for (const value of values) {
      const first = entropy(value);
      for (let i = 0; i < 100; i += 1) {
        expect(entropy(value)).toBe(first);
      }
    }

    // Interleaved evaluation must not leak state between calls.
    const interleaved = values.flatMap((value) => [
      entropy(value),
      entropy(""),
      entropy(null),
      entropy(123),
      entropy(value),
    ]);
    expect(interleaved.filter((_, index) => index % 5 === 4)).toEqual(values.map((value) => entropy(value)));
  });

  it("is stable under concurrent execution", async () => {
    const values = ["mixedCase123!@#", "aaaa", "\u{1F600}\u{1F600}b", "sk_test_abcdefghijklmnop"];
    const baseline = values.map((value) => entropy(value));

    const results = await Promise.all(
      Array.from({ length: 40 }, async (_, index) => {
        const value = values[index % values.length];
        return entropy(value);
      })
    );

    results.forEach((result, index) => {
      expect(result).toBe(baseline[index % values.length]);
    });
  });

  it("does not change detection verdicts for the surrounding token heuristics", () => {
    const secret = tokenLike();
    const placeholder = "your_example_api_key_here";
    const short = "Ab3dE5";

    expect(entropy(secret)).toBeGreaterThan(secretScanUtils.MIN_HIGH_ENTROPY_SCORE);
    expect(secretScanUtils.isHighEntropyToken(secret)).toBe(true);
    expect(secretScanUtils.isHighEntropyToken(placeholder)).toBe(false);
    expect(secretScanUtils.isHighEntropyToken(short)).toBe(false);

    // The entropy gate must not start flagging the documented example key.
    expect(secretScanUtils.isHighEntropyToken(".env.example")).toBe(false);
  });

  it("keeps scanLine and scanFileContent working when entropy inputs are degenerate", () => {
    const awsKey = `AKIA${"IOSFODNN7EXAMPLE"}`;
    const allowlist = { entries: [], globalPatterns: [] };

    // Astral characters in an unrelated string on the same line must not
    // disturb the known-pattern finding.
    const line = `const note = "\u{1F600}\u{1F600}\u{1F600}"; const key = "${awsKey}";`;
    const findings = secretScanUtils.scanLine(line, 1, "src/note.ts", allowlist);
    expect(findings.map((finding: { type: string }) => finding.type)).toEqual(["aws-access-key"]);
    expect(findings[0].preview).not.toContain(awsKey);

    // A file made entirely of astral padding still scans end to end.
    const astralFile = `\u{1F600}\n\u{1F601}\n`;
    expect(secretScanUtils.scanFileContent(astralFile, "src/pad.ts", allowlist)).toEqual([]);
  });
});

describe("isHighEntropyToken failure boundaries (issue 2603)", () => {
  const joinParts = (parts: string[]): string => parts.join("");

  const VALID_32_TOKEN = joinParts(["K9b+", "V2mZ8", "_xP1w", "L7yQ4", "tN0jR", "3sU6v", "E8a"]);
  const VALID_48_TOKEN = joinParts(["Wj9_", "kLm7N", "p2Qr5", "Tv8Xy", "1Bz4C", "d6Gh0", "Js3La", "8Po9U", "i3Yt6", "Re2W"]);
  const VALID_64_TOKEN = joinParts(["dGhp", "cy1p", "cy1h", "LXZl", "cnkt", "c2Vj", "cmV0", "LXRv", "a2Vu", "LXZh", "bHVl", "LTEy", "MzQt", "NTY3", "ODkw", "Kz0v"]);

  describe("non-string and adverse inputs (failure boundary)", () => {
    it("fails closed on null and undefined without throwing", () => {
      expect(() => secretScanUtils.isHighEntropyToken(null)).not.toThrow();
      expect(secretScanUtils.isHighEntropyToken(null)).toBe(false);

      expect(() => secretScanUtils.isHighEntropyToken(undefined)).not.toThrow();
      expect(secretScanUtils.isHighEntropyToken(undefined)).toBe(false);
    });

    it("fails closed on numeric inputs without throwing", () => {
      const numbers = [0, 1, 42, -1, -999, NaN, Infinity, -Infinity, 1e40];
      for (const num of numbers) {
        expect(() => secretScanUtils.isHighEntropyToken(num)).not.toThrow();
        expect(secretScanUtils.isHighEntropyToken(num)).toBe(false);
      }
    });

    it("fails closed on boolean inputs without throwing", () => {
      expect(() => secretScanUtils.isHighEntropyToken(true)).not.toThrow();
      expect(secretScanUtils.isHighEntropyToken(true)).toBe(false);

      expect(() => secretScanUtils.isHighEntropyToken(false)).not.toThrow();
      expect(secretScanUtils.isHighEntropyToken(false)).toBe(false);
    });

    it("fails closed on BigInt and Symbol inputs without throwing", () => {
      const bigIntVal = BigInt("1234567890123456789012345678901234567890");
      expect(() => secretScanUtils.isHighEntropyToken(bigIntVal)).not.toThrow();
      expect(secretScanUtils.isHighEntropyToken(bigIntVal)).toBe(false);

      const sym = Symbol(VALID_32_TOKEN);
      expect(() => secretScanUtils.isHighEntropyToken(sym)).not.toThrow();
      expect(secretScanUtils.isHighEntropyToken(sym)).toBe(false);
    });

    it("fails closed on complex objects, arrays, functions, and buffers without throwing", () => {
      const badObjects = [
        {},
        { length: 40 },
        { token: VALID_32_TOKEN },
        [],
        [VALID_32_TOKEN],
        new Array(40).fill("a"),
        () => VALID_32_TOKEN,
        Buffer.from(VALID_32_TOKEN),
      ];

      for (const obj of badObjects) {
        expect(() => secretScanUtils.isHighEntropyToken(obj)).not.toThrow();
        expect(secretScanUtils.isHighEntropyToken(obj)).toBe(false);
      }
    });

    it("fails closed on throwing getters and hostile prototypes without throwing", () => {
      const hostileLength = {
        get length(): number {
          throw new Error("poisoned length property");
        },
      };
      expect(() => secretScanUtils.isHighEntropyToken(hostileLength)).not.toThrow();
      expect(secretScanUtils.isHighEntropyToken(hostileLength)).toBe(false);

      const hostileToString = {
        length: 40,
        toString() {
          throw new Error("poisoned toString method");
        },
      };
      expect(() => secretScanUtils.isHighEntropyToken(hostileToString)).not.toThrow();
      expect(secretScanUtils.isHighEntropyToken(hostileToString)).toBe(false);
    });

    it("fails closed on revoked Proxies without throwing", () => {
      const revocable = Proxy.revocable({}, {});
      revocable.revoke();
      expect(() => secretScanUtils.isHighEntropyToken(revocable.proxy)).not.toThrow();
      expect(secretScanUtils.isHighEntropyToken(revocable.proxy)).toBe(false);
    });

    it("unwraps String wrapper objects identically to string primitives", () => {
      // eslint-disable-next-line no-new-wrappers
      const wrappedValid = new String(VALID_32_TOKEN);
      // eslint-disable-next-line no-new-wrappers
      const wrappedShort = new String("short");
      expect(secretScanUtils.isHighEntropyToken(wrappedValid)).toBe(true);
      expect(secretScanUtils.isHighEntropyToken(wrappedShort)).toBe(false);
    });
  });

  describe("length boundary conditions", () => {
    it("rejects empty string and short strings below threshold", () => {
      expect(secretScanUtils.isHighEntropyToken("")).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("a")).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("a".repeat(16))).toBe(false);
    });

    it("rejects boundary length at exactly MIN_HIGH_ENTROPY_LENGTH - 1 (31 chars)", () => {
      const token31 = VALID_32_TOKEN.slice(0, 31);
      expect(token31.length).toBe(31);
      expect(secretScanUtils.isHighEntropyToken(token31)).toBe(false);
    });

    it("accepts boundary length at exactly MIN_HIGH_ENTROPY_LENGTH (32 chars) when valid", () => {
      expect(VALID_32_TOKEN.length).toBe(32);
      expect(secretScanUtils.isHighEntropyToken(VALID_32_TOKEN)).toBe(true);
    });

    it("evaluates very long strings safely without performance or stack issues", () => {
      const longValidToken = VALID_32_TOKEN.repeat(50);
      expect(secretScanUtils.isHighEntropyToken(longValidToken)).toBe(true);

      const longLowEntropy = "abc123XYZ".repeat(1000);
      expect(secretScanUtils.isHighEntropyToken(longLowEntropy)).toBe(false);

      const hugeString = "A".repeat(100000);
      expect(secretScanUtils.isHighEntropyToken(hugeString)).toBe(false);
    });
  });

  describe("character set and alphabet boundaries", () => {
    it("accepts valid base64 and base64url characters [A-Za-z0-9+/=_-]", () => {
      expect(secretScanUtils.isHighEntropyToken(VALID_32_TOKEN)).toBe(true);
      expect(secretScanUtils.isHighEntropyToken(VALID_48_TOKEN)).toBe(true);
      expect(secretScanUtils.isHighEntropyToken(VALID_64_TOKEN)).toBe(true);
    });

    it("rejects whitespace and strings with leading, trailing, or internal spaces", () => {
      expect(secretScanUtils.isHighEntropyToken(" ".repeat(32))).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("\t".repeat(32))).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("\n".repeat(32))).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("\r\n".repeat(16))).toBe(false);

      expect(secretScanUtils.isHighEntropyToken(` ${VALID_32_TOKEN}`)).toBe(false);
      expect(secretScanUtils.isHighEntropyToken(`${VALID_32_TOKEN} `)).toBe(false);
      expect(
        secretScanUtils.isHighEntropyToken(
          `${VALID_32_TOKEN.slice(0, 16)} ${VALID_32_TOKEN.slice(17)}`
        )
      ).toBe(false);
    });

    it("rejects control characters and unapproved symbols", () => {
      const unapprovedChars = ["\x00", "\x07", "\x1b", "@", "#", "$", "%", "^", "&", "*", "(", ")", "{", "}", "[", "]", ":", ";", '"', "'", "<", ">", "?", "\\", "|", "~", ",", "."];
      for (const ch of unapprovedChars) {
        const corrupted = `${ch}${VALID_32_TOKEN.slice(1)}`;
        expect(secretScanUtils.isHighEntropyToken(corrupted)).toBe(false);
      }
    });

    it("rejects unicode lookalikes, emojis, and accented characters", () => {
      const unicodeReplacements = ["\u200b", "🔒", "é", "ñ", "中", "😀"];
      for (const u of unicodeReplacements) {
        const head = `${u}${VALID_32_TOKEN.slice(u.length)}`;
        const mid = `${VALID_32_TOKEN.slice(0, 15)}${u}${VALID_32_TOKEN.slice(15 + u.length)}`;
        const tail = `${VALID_32_TOKEN.slice(0, 32 - u.length)}${u}`;
        expect(secretScanUtils.isHighEntropyToken(head)).toBe(false);
        expect(secretScanUtils.isHighEntropyToken(mid)).toBe(false);
        expect(secretScanUtils.isHighEntropyToken(tail)).toBe(false);
      }
    });
  });

  describe("structured non-secret exclusions", () => {
    it("excludes hexadecimal digests (MD5, SHA-1, SHA-256) of various lengths", () => {
      expect(secretScanUtils.isHighEntropyToken("5d41402abc4b2a76b9719d911017c592")).toBe(false); // 32 hex chars (MD5)
      expect(secretScanUtils.isHighEntropyToken("2aae6c35c94fcfb415dbe95f408b9ce91ee846ed")).toBe(false); // 40 hex chars (SHA-1)
      expect(secretScanUtils.isHighEntropyToken("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")).toBe(false); // 64 hex chars (SHA-256)
      expect(secretScanUtils.isHighEntropyToken("0x5d41402abc4b2a76b9719d911017c592")).toBe(false); // 0x-prefixed
      expect(secretScanUtils.isHighEntropyToken("0x2AAE6C35C94FCFB415DBE95F408B9CE91EE846ED")).toBe(false); // uppercase 0x
    });

    it("does not exclude tokens that contain non-hex characters like g..z or symbols", () => {
      expect(secretScanUtils.isHexString("5d41402abc4b2a76b9719d911017c59g")).toBe(false);
      // Valid high-entropy token contains non-hex characters (K, V, Z, _, +, w, y, etc.)
      expect(secretScanUtils.isHexString(VALID_32_TOKEN)).toBe(false);
      expect(secretScanUtils.isHighEntropyToken(VALID_32_TOKEN)).toBe(true);
    });

    it("excludes Stellar public keys (G...) and muxed keys (X...)", () => {
      expect(
        secretScanUtils.isHighEntropyToken("GDRXE2BQUC3AZNPVFSJEZIXZZDZSMTLBVWN4HZ5SAPHP2R3C3YHS6M2B")
      ).toBe(false);
      expect(
        secretScanUtils.isHighEntropyToken("XDRXE2BQUC3AZNPVFSJEZIXZZDZSMTLBVWN4HZ5SAPHP2R3C3YHS6M2B")
      ).toBe(false);
    });
  });

  describe("obvious placeholder exclusions", () => {
    it("rejects single character repetitions", () => {
      expect(secretScanUtils.isHighEntropyToken("x".repeat(32))).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("y".repeat(32))).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("z".repeat(32))).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("A".repeat(32))).toBe(false);
    });

    it("rejects tokens with two or fewer unique characters of length >= 32", () => {
      expect(secretScanUtils.isHighEntropyToken("abababababababababababababababab")).toBe(false);
    });

    it("rejects known development and test placeholder prefixes", () => {
      expect(secretScanUtils.isHighEntropyToken("your_secret_key_1234567890abcdef")).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("example_token_1234567890abcdef123")).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("placeholder_secret_value_12345678")).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("changeme_secret_value_12345678901")).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("test-secret-value-1234567890abcdef")).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("test_secret_value_1234567890abcdef")).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("development-only-export-secret-32-chars")).toBe(false);
      expect(secretScanUtils.isHighEntropyToken("fallback-secret-for-signing-links-123")).toBe(false);
    });

    it("rejects camelCase and PascalCase code identifiers and URLs", () => {
      expect(
        secretScanUtils.isHighEntropyToken("myVeryLongCamelCaseIdentifierWithoutDigits")
      ).toBe(false);
      expect(
        secretScanUtils.isHighEntropyToken("https://api.quicklendx.io/v1/auth/tokens/generate")
      ).toBe(false);
      expect(
        secretScanUtils.isHighEntropyToken("/api/v1/borrowers/loans/repayments/schedule")
      ).toBe(false);
    });
  });

  describe("character class diversity and unique count boundaries", () => {
    it("rejects tokens with fewer than MIN_UNIQUE_CHARACTERS (10) unique characters", () => {
      // 9 unique characters repeated across 32 length
      const nineChars = "abcdefghiabcdefghiabcdefghiabcde";
      expect(new Set(nineChars).size).toBe(9);
      expect(secretScanUtils.isHighEntropyToken(nineChars)).toBe(false);
    });

    it("rejects single character class tokens regardless of length", () => {
      expect(secretScanUtils.isHighEntropyToken("abcdefghijklmnopqrstuvwxyzabcdef")).toBe(false); // lowercase only
      expect(secretScanUtils.isHighEntropyToken("ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEF")).toBe(false); // uppercase only
      expect(secretScanUtils.isHighEntropyToken("01234567890123456789012345678901")).toBe(false); // digits only
    });

    it("requires at least two distinct character classes", () => {
      const lowerAndDigits = joinParts(["abcdefghijklm", "nopqrstuvwxyz", "123456"]);
      expect(secretScanUtils.hasMixedCharacterClasses(lowerAndDigits)).toBe(true);
      expect(secretScanUtils.hasMixedCharacterClasses("abcdefghijklmnopqrstuvwxyzABCDEF")).toBe(true);
    });
  });

  describe("Shannon entropy threshold boundary (MIN_HIGH_ENTROPY_SCORE = 4.5)", () => {
    it("rejects patterned strings with low Shannon entropy (< 4.5)", () => {
      const patterned = "a1b2c3d4e5a1b2c3d4e5a1b2c3d4e5a1";
      expect(secretScanUtils.shannonEntropy(patterned)).toBeLessThan(4.5);
      expect(secretScanUtils.isHighEntropyToken(patterned)).toBe(false);
    });

    it("strictly evaluates the mathematical boundary at 4.5 entropy score", () => {
      // 14 chars once + 9 chars twice = 32 chars, entropy = 142/32 = 4.4375 (< 4.5)
      const belowBoundary = joinParts(["ABCDEFG", "HIJKLMN", "Oabcdef0_", "Oabcdef0_"]);
      expect(belowBoundary.length).toBe(32);
      expect(secretScanUtils.shannonEntropy(belowBoundary)).toBe(4.4375);
      expect(secretScanUtils.isHighEntropyToken(belowBoundary)).toBe(false);

      // 16 chars once + 8 chars twice = 32 chars, entropy = 144/32 = 4.5000 (>= 4.5)
      const atBoundary = joinParts(["ABCDEFGH", "IJKLMNOP", "abcdef0_", "abcdef0_"]);
      expect(atBoundary.length).toBe(32);
      expect(secretScanUtils.shannonEntropy(atBoundary)).toBe(4.5);
      expect(secretScanUtils.isHighEntropyToken(atBoundary)).toBe(true);
    });
  });

  describe("determinism, concurrency, and idempotence", () => {
    it("returns identical results across 100 repeated calls for diverse inputs", () => {
      const testInputs = [
        VALID_32_TOKEN,
        VALID_48_TOKEN,
        "short",
        "5d41402abc4b2a76b9719d911017c592",
        "GDRXE2BQUC3AZNPVFSJEZIXZZDZSMTLBVWN4HZ5SAPHP2R3C3YHS6M2B",
        "",
        null,
        undefined,
      ];

      for (const input of testInputs) {
        const first = secretScanUtils.isHighEntropyToken(input);
        for (let i = 0; i < 100; i += 1) {
          expect(secretScanUtils.isHighEntropyToken(input)).toBe(first);
        }
      }
    });

    it("produces consistent results under concurrent execution", async () => {
      const candidates = [
        { value: VALID_32_TOKEN, expected: true },
        { value: VALID_48_TOKEN, expected: true },
        { value: "5d41402abc4b2a76b9719d911017c592", expected: false },
        { value: "short", expected: false },
        { value: "x".repeat(32), expected: false },
        { value: null, expected: false },
        { value: undefined, expected: false },
        { value: 12345, expected: false },
      ];

      await Promise.all(
        Array.from({ length: 50 }, () =>
          Promise.all(
            candidates.map(async ({ value, expected }) => {
              expect(secretScanUtils.isHighEntropyToken(value)).toBe(expected);
            })
          )
        )
      );
    });

    it("produces results independent of call order (no shared regex state)", () => {
      const list = [VALID_32_TOKEN, "short", VALID_48_TOKEN, "5d41402abc4b2a76b9719d911017c592"];
      const forward = list.map((val) => secretScanUtils.isHighEntropyToken(val));
      const backward = [...list].reverse().map((val) => secretScanUtils.isHighEntropyToken(val));
      expect(forward).toEqual(backward.reverse());
    });
  });

  describe("security and non-leakage invariant", () => {
    it("returns strictly boolean type and never leaks token content in outputs", () => {
      const secret = joinParts(["SUPER_SECRET_", "TOKEN_VALUE_FOR_", "LEAK_TEST_1234="]);
      const result = secretScanUtils.isHighEntropyToken(secret);
      expect(typeof result).toBe("boolean");
      expect(String(result)).not.toContain(secret);
    });
  });

  describe("caller compatibility with collectHighEntropyMatches and scanLine", () => {
    it("identifies high-entropy quoted string matches in scanLine", () => {
      const line = joinParts(['const secret = "', VALID_32_TOKEN, '";']);
      const findings = secretScanUtils.scanLine(
        line,
        10,
        "src/auth.ts"
      );
      expect(findings.some((f: { type: string }) => f.type === "high-entropy")).toBe(true);
    });

    it("ignores non-high-entropy tokens and hex strings in scanLine", () => {
      const findings = secretScanUtils.scanLine(
        'const hash = "5d41402abc4b2a76b9719d911017c592";',
        11,
        "src/hash.ts"
      );
      expect(findings.some((f: { type: string }) => f.type === "high-entropy")).toBe(false);
    });
  });
});

describe("scanBackend failure boundaries", () => {
  it("fails closed on non-string backendRoot inputs without throwing", () => {
    const invalidRoots = [
      null,
      0,
      123,
      -1,
      NaN,
      Infinity,
      true,
      false,
      {},
      { root: "/path" },
      [],
      ["src"],
      () => "/tmp",
      Symbol("root"),
      BigInt(42),
    ];

    for (const badRoot of invalidRoots) {
      let findings: unknown;
      expect(() => {
        findings = secretScanUtils.scanBackend(badRoot as unknown as string);
      }).not.toThrow();
      expect(findings).toEqual([]);
    }
  });

  it("returns an empty findings array for empty, whitespace, and non-existent roots", () => {
    expect(secretScanUtils.scanBackend("")).toEqual([]);
    expect(secretScanUtils.scanBackend("   ")).toEqual([]);
    expect(secretScanUtils.scanBackend("\t\n")).toEqual([]);

    const nonExistent = path.join(os.tmpdir(), `non-existent-dir-${Date.now()}-${Math.random()}`);
    expect(secretScanUtils.scanBackend(nonExistent)).toEqual([]);
  });

  it("returns an empty findings array when backendRoot points to a file rather than a directory", () => {
    const fixtureRoot = createFixtureDir();
    const filePath = writeFixture(fixtureRoot, "src/not-a-dir.txt", "some content\n");
    expect(secretScanUtils.scanBackend(filePath)).toEqual([]);
  });

  it("handles null, primitive, and malformed options gracefully without throwing", () => {
    const fixtureRoot = createFixtureDir();
    writeFixture(fixtureRoot, "src/clean.ts", "export const clean = true;\n");

    const badOptions = [
      null,
      undefined,
      0,
      123,
      "options-string",
      true,
      false,
      [],
      [1, 2, 3],
      Symbol("options"),
      BigInt(100),
    ];

    for (const badOpt of badOptions) {
      let findings: unknown;
      expect(() => {
        findings = secretScanUtils.scanBackend(
          fixtureRoot,
          badOpt as unknown as Record<string, unknown>
        );
      }).not.toThrow();
      expect(findings).toEqual([]);
    }
  });

  it("handles empty and malformed scanRoots in options", () => {
    const fixtureRoot = createFixtureDir();
    const secret = makeHighEntropySecret();
    writeFixture(fixtureRoot, "src/leaked.ts", `const token = "${secret}";\n`);

    // Empty scanRoots array: scans nothing, returns []
    const emptyScan = secretScanUtils.scanBackend(fixtureRoot, {
      scanRoots: [],
      allowlist: { entries: [], globalPatterns: [] },
    });
    expect(emptyScan).toEqual([]);

    // scanRoots with non-string, null, or undefined elements: filters them safely and still scans valid root
    const mixedScan = secretScanUtils.scanBackend(fixtureRoot, {
      scanRoots: [null, undefined, 123, true, {}, "src"],
      allowlist: { entries: [], globalPatterns: [] },
    });
    expect(mixedScan).toHaveLength(1);
    expect(mixedScan[0].file).toBe("src/leaked.ts");

    // scanRoots with non-existent subdirectory: skips missing directory gracefully
    const missingSubdirScan = secretScanUtils.scanBackend(fixtureRoot, {
      scanRoots: ["does-not-exist", "src"],
      allowlist: { entries: [], globalPatterns: [] },
    });
    expect(missingSubdirScan).toHaveLength(1);
    expect(missingSubdirScan[0].file).toBe("src/leaked.ts");
  });

  it("deduplicates scan targets so duplicate roots or example files do not multiply findings", () => {
    const fixtureRoot = createFixtureDir();
    const secret = makeHighEntropySecret();
    writeFixture(fixtureRoot, "src/leaked.ts", `const token = "${secret}";\n`);
    writeFixture(fixtureRoot, ".env.example", `EXAMPLE_KEY="${secret}"\n`);

    // Duplicate "src" roots and duplicate example files
    const findings = secretScanUtils.scanBackend(fixtureRoot, {
      scanRoots: ["src", "src", "src"],
      exampleFiles: [".env.example", ".env.example"],
      allowlist: { entries: [], globalPatterns: [] },
    });

    expect(findings).toHaveLength(2);
    const files = findings.map((f: { file: string }) => f.file);
    expect(files).toContain("src/leaked.ts");
    expect(files).toContain(".env.example");
  });

  it("accepts extensions and ignoredFiles as Array, Set, or single string", () => {
    const fixtureRoot = createFixtureDir();
    const secret = makeHighEntropySecret();
    writeFixture(fixtureRoot, "src/file.ts", `const t = "${secret}";\n`);
    writeFixture(fixtureRoot, "src/file.custom", `const t = "${secret}";\n`);
    writeFixture(fixtureRoot, "src/skip.ts", `const t = "${secret}";\n`);

    // Array extensions and ignoredFiles
    const findingsArray = secretScanUtils.scanBackend(fixtureRoot, {
      extensions: [".custom"],
      ignoredFiles: ["file.ts"],
      allowlist: { entries: [], globalPatterns: [] },
    });
    expect(findingsArray).toHaveLength(1);
    expect(findingsArray[0].file).toBe("src/file.custom");

    // Set extensions and ignoredFiles
    const findingsSet = secretScanUtils.scanBackend(fixtureRoot, {
      extensions: new Set([".ts"]),
      ignoredFiles: new Set(["skip.ts"]),
      allowlist: { entries: [], globalPatterns: [] },
    });
    expect(findingsSet).toHaveLength(1);
    expect(findingsSet[0].file).toBe("src/file.ts");
  });

  it("preserves findings from valid files when partial failure occurs on an unreadable target", () => {
    const fixtureRoot = createFixtureDir();
    const secret1 = makeHighEntropySecret();
    const secret2 = makeHighEntropySecret();
    writeFixture(fixtureRoot, "src/good1.ts", `const key1 = "${secret1}";\n`);
    writeFixture(fixtureRoot, "src/bad.ts", "some content\n");
    writeFixture(fixtureRoot, "src/good2.ts", `const key2 = "${secret2}";\n`);

    // Mock readFileSync to simulate permission/I/O failure on bad.ts only
    const originalReadFileSync = fs.readFileSync;
    const errorsReported: Array<{ file: string; error: Error }> = [];
    jest.spyOn(fs, "readFileSync").mockImplementation(((targetPath: fs.PathOrFileDescriptor, options: any) => {
      if (typeof targetPath === "string" && targetPath.includes("bad.ts")) {
        const err = new Error("EACCES: permission denied, open 'bad.ts'") as NodeJS.ErrnoException;
        err.code = "EACCES";
        throw err;
      }
      return originalReadFileSync(targetPath, options);
    }) as any);

    try {
      const findings = secretScanUtils.scanBackend(fixtureRoot, {
        allowlist: { entries: [], globalPatterns: [] },
        onFileError: (target: { relativePath: string }, error: Error) => {
          errorsReported.push({ file: target.relativePath, error });
        },
      });

      // Partial failure: findings from good1 and good2 are preserved!
      expect(findings).toHaveLength(2);
      const files = findings.map((f: { file: string }) => f.file);
      expect(files).toContain("src/good1.ts");
      expect(files).toContain("src/good2.ts");
      expect(files).not.toContain("src/bad.ts");

      // onFileError was invoked for the unreadable target
      expect(errorsReported).toHaveLength(1);
      expect(errorsReported[0].file).toBe("src/bad.ts");
      expect(errorsReported[0].error.message).toContain("EACCES");
    } finally {
      jest.restoreAllMocks();
    }
  });

  it("handles directory traversal permission failure gracefully without aborting the entire scan", () => {
    const fixtureRoot = createFixtureDir();
    const secret = makeHighEntropySecret();
    writeFixture(fixtureRoot, "src/accessible/leak.ts", `const key = "${secret}";\n`);
    writeFixture(fixtureRoot, "src/forbidden/leak.ts", `const key = "${secret}";\n`);

    const originalReaddirSync = fs.readdirSync;
    jest.spyOn(fs, "readdirSync").mockImplementation(((dirPath: fs.PathLike, options: any) => {
      if (typeof dirPath === "string" && dirPath.includes("forbidden")) {
        const err = new Error("EACCES: permission denied, scandir 'forbidden'") as NodeJS.ErrnoException;
        err.code = "EACCES";
        throw err;
      }
      return originalReaddirSync(dirPath, options);
    }) as any);

    try {
      const findings = secretScanUtils.scanBackend(fixtureRoot, {
        allowlist: { entries: [], globalPatterns: [] },
      });

      // Finding from accessible directory is retained
      expect(findings).toHaveLength(1);
      expect(findings[0].file).toBe("src/accessible/leak.ts");
    } finally {
      jest.restoreAllMocks();
    }
  });

  it("is deterministic across repeated calls with identical fixture trees", () => {
    const fixtureRoot = createFixtureDir();
    const secret = makeHighEntropySecret();
    writeFixture(fixtureRoot, "src/b_leak.ts", `const b = "${secret}";\n`);
    writeFixture(fixtureRoot, "src/a_leak.ts", `const a = "${secret}";\n`);
    writeFixture(fixtureRoot, ".env.example", `KEY="${secret}"\n`);

    const options = { allowlist: { entries: [], globalPatterns: [] } };
    const firstRun = secretScanUtils.scanBackend(fixtureRoot, options);
    expect(firstRun).toHaveLength(3);

    for (let i = 0; i < 50; i += 1) {
      const repeatRun = secretScanUtils.scanBackend(fixtureRoot, options);
      expect(repeatRun).toEqual(firstRun);
    }
  });

  it("produces consistent and isolated results under concurrent executions", () => {
    const fixtureRoot1 = createFixtureDir();
    const fixtureRoot2 = createFixtureDir();
    const secret1 = makeHighEntropySecret();
    const secret2 = makeHighEntropySecret();

    writeFixture(fixtureRoot1, "src/leak1.ts", `const key1 = "${secret1}";\n`);
    writeFixture(fixtureRoot2, "src/leak2.ts", `const key2 = "${secret2}";\n`);

    const runScan1 = () =>
      secretScanUtils.scanBackend(fixtureRoot1, { allowlist: { entries: [], globalPatterns: [] } });
    const runScan2 = () =>
      secretScanUtils.scanBackend(fixtureRoot2, { allowlist: { entries: [], globalPatterns: [] } });

    const expected1 = runScan1();
    const expected2 = runScan2();

    const tasks = Array.from({ length: 30 }, (_, index) =>
      Promise.resolve().then(() => (index % 2 === 0 ? runScan1() : runScan2()))
    );

    return Promise.all(tasks).then((results) => {
      results.forEach((res, index) => {
        if (index % 2 === 0) {
          expect(res).toEqual(expected1);
        } else {
          expect(res).toEqual(expected2);
        }
      });
    });
  });

  it("handles malformed options.allowlist and missing allowlistPath safely", () => {
    const fixtureRoot = createFixtureDir();
    const secret = makeHighEntropySecret();
    writeFixture(fixtureRoot, "src/leak.ts", `const key = "${secret}";\n`);

    // Malformed allowlist (non-object or bad entries) fails closed without throwing
    const malformedAllowlists = [null, undefined, "not-allowlist", 123, true, { entries: "bad" }];
    for (const badAllow of malformedAllowlists) {
      const findings = secretScanUtils.scanBackend(fixtureRoot, {
        allowlist: badAllow,
      });
      expect(findings).toHaveLength(1);
      expect(findings[0].file).toBe("src/leak.ts");
    }

    // Missing allowlistPath does not throw and defaults to empty allowlist
    const missingAllowPath = path.join(fixtureRoot, "non-existent-allow.json");
    const findingsMissing = secretScanUtils.scanBackend(fixtureRoot, {
      allowlistPath: missingAllowPath,
    });
    expect(findingsMissing).toHaveLength(1);
  });

  it("propagates allowlist JSON parse errors with clear diagnosable message", () => {
    const fixtureRoot = createFixtureDir();
    const corruptedAllowlist = path.join(fixtureRoot, "corrupted-allow.json");
    fs.writeFileSync(corruptedAllowlist, "{broken json", "utf8");

    expect(() =>
      secretScanUtils.scanBackend(fixtureRoot, { allowlistPath: corruptedAllowlist })
    ).toThrow(/Failed to parse secret scan allowlist/);
  });

  it("ensures all findings satisfy assertNoSecretsPrinted and never expose raw secret values", () => {
    const fixtureRoot = createFixtureDir();
    const secret = makeHighEntropySecret();
    writeFixture(fixtureRoot, "src/leak.ts", `const token = "${secret}";\n`);

    const findings = secretScanUtils.scanBackend(fixtureRoot, {
      allowlist: { entries: [], globalPatterns: [] },
    });
    expect(findings).toHaveLength(1);

    const finding = findings[0];
    expect(finding.preview).not.toContain(secret);
    expect(finding.preview).toContain("...");
    expect(secretScanUtils.isLogSafePreview(finding.preview)).toBe(true);

    const formatted = secretScanUtils.formatFindings(findings);
    expect(formatted).not.toContain(secret);
    expect(() => secretScanUtils.assertNoSecretsPrinted(formatted, findings)).not.toThrow();
  });
});

describe("backend security:scan integration", () => {
  const repoRoot = path.resolve(__dirname, "..");

    it("accepts valid Stellar public keys and hash signers (G... and X...)", () => {
      expect(secretScanUtils.isStellarStrKeyLike(validGKey)).toBe(true);
      expect(secretScanUtils.isStellarStrKeyLike(validXKey)).toBe(true);
      for (const key of validBoundaryKeys) {
        expect(secretScanUtils.isStellarStrKeyLike(key)).toBe(true);
      }
    });

    it("rejects Stellar secret seeds (S...) to enforce security invariants", () => {
      const secretSeed = makeStellarSecretSeed();
      const seedAllB = `S${"B".repeat(55)}`;
      expect(secretScanUtils.isStellarStrKeyLike(secretSeed)).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(seedAllB)).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(makeStellarSecretSeed())).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(`S${"A".repeat(55)}`)).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(`S${"7".repeat(55)}`)).toBe(false);
    });

    it("rejects other disallowed prefix characters", () => {
      const disallowedPrefixes = "ABCDEFHIJKLMNOPQRTUVWYZ";
      for (const prefix of disallowedPrefixes) {
        expect(secretScanUtils.isStellarStrKeyLike(`${prefix}${"A".repeat(55)}`)).toBe(false);
      }
    });

    it("rejects lowercase prefixes and lowercase base32 characters", () => {
      expect(secretScanUtils.isStellarStrKeyLike(`g${"A".repeat(55)}`)).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(`x${"A".repeat(55)}`)).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(validGKey.toLowerCase())).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(`G${"a".repeat(55)}`)).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(`G${"A".repeat(54)}b`)).toBe(false);
    });

    it("rejects non-base32 characters (0, 1, 8, 9) and special characters", () => {
      for (const invalidChar of ["0", "1", "8", "9"]) {
        expect(secretScanUtils.isStellarStrKeyLike(`G${"A".repeat(54)}${invalidChar}`)).toBe(false);
        expect(secretScanUtils.isStellarStrKeyLike(`X${invalidChar}${"A".repeat(54)}`)).toBe(false);
      }

      const symbols = [" ", "\t", "\n", "\r", "\0", "-", "_", "/", "+", "=", "@", "!", "$", ".", ":"];
      for (const sym of symbols) {
        expect(secretScanUtils.isStellarStrKeyLike(`G${"A".repeat(54)}${sym}`)).toBe(false);
        expect(secretScanUtils.isStellarStrKeyLike(`${sym}G${"A".repeat(54)}`)).toBe(false);
      }
    });

    it("enforces strict 56-character length boundary", () => {
      expect(secretScanUtils.isStellarStrKeyLike("")).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike("G")).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike("X")).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(`G${"A".repeat(53)}`)).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(`G${"A".repeat(54)}`)).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(`G${"A".repeat(55)}`)).toBe(true);
      expect(secretScanUtils.isStellarStrKeyLike(`G${"A".repeat(56)}`)).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(`G${"A".repeat(100)}`)).toBe(false);
      expect(secretScanUtils.isStellarStrKeyLike(`G${"A".repeat(1000)}`)).toBe(false);
    });

    it("safely handles non-string inputs and failure-boundary types without throwing", () => {
      const nonStringInputs = [
        null,
        undefined,
        42,
        0,
        -1,
        NaN,
        Infinity,
        -Infinity,
        true,
        false,
        BigInt(123456789),
        Symbol("GDRXE2BQUC3AZNPVFSJEZIXZZDZSMTLBVWN4HZ5SAPHP2R3C3YHS6M2B"),
        Symbol.for("stellar"),
        {},
        { validGKey },
        { toString: () => validGKey },
        { toString() { throw new Error("poisoned toString"); } },
        { valueOf() { throw new Error("poisoned valueOf"); } },
        Object.create(null),
        [validGKey],
        [],
        () => validGKey,
        Buffer.from(validGKey),
      ];

      for (const input of nonStringInputs) {
        expect(() => {
          const result = secretScanUtils.isStellarStrKeyLike(input);
          expect(result).toBe(false);
        }).not.toThrow();
      }
    });

    it("is strictly deterministic and idempotent across repeated, alternating, and concurrent calls", async () => {
      const testCases = [
        { input: validGKey, expected: true },
        { input: validXKey, expected: true },
        { input: `S${"A".repeat(55)}`, expected: false },
        { input: null, expected: false },
        { input: undefined, expected: false },
        { input: "", expected: false },
        { input: `G${"A".repeat(54)}0`, expected: false },
        { input: Symbol("boundary"), expected: false },
        { input: `G${"A".repeat(56)}`, expected: false },
      ];

      for (let i = 0; i < 100; i++) {
        for (const { input, expected } of testCases) {
          expect(secretScanUtils.isStellarStrKeyLike(input)).toBe(expected);
        }
      }

      const concurrentRuns = Array.from({ length: 200 }, (_, index) => {
        const item = testCases[index % testCases.length];
        return Promise.resolve().then(() => {
          const result = secretScanUtils.isStellarStrKeyLike(item.input);
          return { result, expected: item.expected };
        });
      });

      const results = await Promise.all(concurrentRuns);
      for (const { result, expected } of results) {
        expect(result).toBe(expected);
      }
    });

    it("preserves secret detection invariants when integrated with scanner", () => {
      expect(secretScanUtils.isHighEntropyToken(validGKey)).toBe(false);

      const plantedSecretSeed = makeStellarSecretSeed();
      const findings = secretScanUtils.scanLine(
        `const secret = "${plantedSecretSeed}";`,
        1,
        "src/wallet.ts",
        { entries: [], globalPatterns: [] }
      );
      expect(findings).toHaveLength(1);
      expect(findings[0].type).toBe("stellar-secret-seed");
      expect(secretScanUtils.isStellarStrKeyLike(plantedSecretSeed)).toBe(false);
    });
  });
});

describe("scanTargets failure boundaries (issue 2617)", () => {
  const emptyAllowlist = { entries: [], globalPatterns: [] };

  const awsKey = (): string => ["AKIA", "IOSFODNN7EXAMPLE"].join("");

  const makeTarget = (root: string, relativePath: string) => ({
    absolutePath: path.join(root, relativePath),
    relativePath,
  });

  // Writes a file and returns a target descriptor for it.
  const seedTarget = (root: string, relativePath: string, content: string) => {
    const absolutePath = writeFixture(root, relativePath, content);
    return { absolutePath, relativePath };
  };

  // Makes a file unreadable and reports whether the denial actually took
  // effect. Under a privileged uid (root in CI) the kernel bypasses the mode
  // bits, so the caller falls back to an EISDIR target instead.
  const denyRead = (absolutePath: string): boolean => {
    fs.chmodSync(absolutePath, 0o000);
    try {
      fs.readFileSync(absolutePath, "utf8");
      return false;
    } catch (error) {
      return (error as NodeJS.ErrnoException).code === "EACCES";
    }
  };

  const scan = (targets: unknown[], allowlist: unknown = emptyAllowlist) => {
    const failures: Array<Record<string, unknown>> = [];
    const findings = secretScanUtils.scanTargets(targets, allowlist, {
      onTargetError: (failure: Record<string, unknown>) => failures.push(failure),
    });
    return { findings, failures };
  };

  it("scans every valid target and preserves full finding context metadata", () => {
    const fixtureRoot = createFixtureDir();
    const key = awsKey();
    const first = seedTarget(fixtureRoot, "src/first.ts", `const a = "${key}";\n`);
    const second = seedTarget(fixtureRoot, "src/second.ts", `const b = "${key}";\n`);
    const clean = seedTarget(fixtureRoot, "src/clean.ts", "export const ok = 1;\n");

    const { findings, failures } = scan([first, second, clean]);

    expect(failures).toHaveLength(0);
    expect(findings).toHaveLength(2);
    expect(findings.map((finding: { file: string }) => finding.file)).toEqual([
      "src/first.ts",
      "src/second.ts",
    ]);
    for (const finding of findings) {
      expect(finding).toMatchObject({ line: 1, type: "aws-access-key" });
      expect(typeof finding.column).toBe("number");
      expect(typeof finding.length).toBe("number");
      expect(finding.preview).not.toContain(key);
    }
  });

  it("returns an empty result for an empty target set", () => {
    const { findings, failures } = scan([]);
    expect(findings).toEqual([]);
    expect(failures).toEqual([]);
    expect(secretScanUtils.scanTargets([], emptyAllowlist)).toEqual([]);
  });

  it("scans a duplicated absolutePath exactly once regardless of repetition count", () => {
    const fixtureRoot = createFixtureDir();
    const key = awsKey();
    const only = seedTarget(fixtureRoot, "src/only.ts", `const a = "${key}";\n`);
    const other = seedTarget(fixtureRoot, "src/other.ts", `const b = "${key}";\n`);

    expect(scan([only, only, only, only, only]).findings).toHaveLength(1);
    // Duplicates interleaved with other targets must not drop the others.
    const { findings, failures } = scan([only, other, only, other, only]);
    expect(findings).toHaveLength(2);
    expect(failures).toHaveLength(0);

    // Overlapping scanRoots is the real-world source of duplicates.
    const roots = ["src", "src"];
    const overlapping = roots.flatMap(() => [only, other]);
    expect(scan(overlapping).findings).toHaveLength(2);
  });

  it("is deterministic for identical input and order-insensitive across permutations", () => {
    const fixtureRoot = createFixtureDir();
    const key = awsKey();
    const a = seedTarget(fixtureRoot, "src/a.ts", `const a = "${key}";\n`);
    const b = seedTarget(fixtureRoot, "src/b.ts", `const b = "${key}";\n`);
    const c = seedTarget(fixtureRoot, "src/c.ts", "export const c = 1;\n");

    const targets = [a, b, c];
    const first = scan(targets);
    for (let i = 0; i < 50; i += 1) {
      const repeat = scan(targets);
      expect(repeat.findings).toEqual(first.findings);
      expect(repeat.failures).toEqual(first.failures);
    }

    // Findings carry their own file context, so any permutation yields the
    // same multiset of findings.
    const permutations = [
      [a, b, c],
      [c, b, a],
      [b, a, c],
      [c, a, b],
    ];
    const sorted = (findings: Array<{ file: string }>) =>
      findings.map((finding) => finding.file).sort();
    const baseline = sorted(first.findings);
    for (const permutation of permutations) {
      expect(sorted(scan(permutation).findings)).toEqual(baseline);
    }
  });

  it("keeps findings from before and after an unreadable target instead of losing them", () => {
    const fixtureRoot = createFixtureDir();
    const key = awsKey();
    const before = seedTarget(fixtureRoot, "src/before.ts", `const a = "${key}";\n`);
    const missing = makeTarget(fixtureRoot, "src/absent.ts");
    const after = seedTarget(fixtureRoot, "src/after.ts", `const b = "${key}";\n`);

    const { findings, failures } = scan([before, missing, after]);

    expect(findings).toHaveLength(2);
    expect(findings.map((finding: { file: string }) => finding.file)).toEqual([
      "src/before.ts",
      "src/after.ts",
    ]);
    expect(failures).toHaveLength(1);
    expect(failures[0]).toMatchObject({
      target: "src/absent.ts",
      reason: "unreadable-target",
      code: "ENOENT",
    });
  });

  it("attempts every target before failing closed when no reporter is supplied", () => {
    const fixtureRoot = createFixtureDir();
    const key = awsKey();
    const first = seedTarget(fixtureRoot, "src/first.ts", `const a = "${key}";\n`);
    const missingA = makeTarget(fixtureRoot, "src/gone-a.ts");
    const middle = seedTarget(fixtureRoot, "src/middle.ts", `const b = "${key}";\n`);
    const missingB = makeTarget(fixtureRoot, "src/gone-b.ts");
    const last = seedTarget(fixtureRoot, "src/last.ts", `const c = "${key}";\n`);

    let thrown: Error | null = null;
    let readAttempts = 0;
    const realRead = fs.readFileSync;
    const spy = jest.spyOn(fs, "readFileSync").mockImplementation(((target: string, ...rest: unknown[]) => {
      readAttempts += 1;
      return (realRead as unknown as (...args: unknown[]) => unknown)(target, ...rest);
    }) as unknown as typeof fs.readFileSync);

    try {
      secretScanUtils.scanTargets(
        [first, missingA, middle, missingB, last],
        emptyAllowlist
      );
    } catch (error) {
      thrown = error as Error;
    } finally {
      spy.mockRestore();
    }

    // All five targets were attempted: the loop no longer aborts early.
    expect(readAttempts).toBe(5);
    // Fail closed: the aggregate names every failed target and its errno.
    expect(thrown).toBeInstanceOf(Error);
    expect((thrown as unknown as Error).message).toContain("could not read 2 of 5 target(s)");
    expect((thrown as unknown as Error).message).toContain("src/gone-a.ts (ENOENT)");
    expect((thrown as unknown as Error).message).toContain("src/gone-b.ts (ENOENT)");
    // The failure path carries paths and errnos, never matched values.
    expect((thrown as unknown as Error).message).not.toContain(key);
  });

  it("isolates structurally invalid entries and still scans the valid ones", () => {
    const fixtureRoot = createFixtureDir();
    const key = awsKey();
    const valid = seedTarget(fixtureRoot, "src/valid.ts", `const a = "${key}";\n`);

    const invalid: unknown[] = [
      null,
      undefined,
      42,
      "src/valid.ts",
      true,
      {},
      { absolutePath: 123, relativePath: "src/valid.ts" },
      { absolutePath: "" },
      { relativePath: "src/valid.ts" },
      [],
    ];

    const { findings, failures } = scan([...invalid, valid, ...invalid]);

    expect(findings).toHaveLength(1);
    expect(findings[0].file).toBe("src/valid.ts");
    expect(failures).toHaveLength(invalid.length * 2);
    for (const failure of failures) {
      expect(failure.reason).toBe("invalid-target");
      expect(failure.code).toBe("EINVAL");
    }
    expect(() => secretScanUtils.scanTargets([...invalid, valid], emptyAllowlist)).toThrow(
      /could not read/
    );
  });

  it("never coerces a malformed target and reports its received type", () => {
    const malformed: unknown[] = [null, undefined, 42, {}, "abc", true, 0];
    for (const value of malformed) {
      expect(() => secretScanUtils.scanTargets(value as unknown[], emptyAllowlist)).toThrow(TypeError);
      expect(() => secretScanUtils.scanTargets(value as unknown[], emptyAllowlist)).toThrow(
        /scanTargets requires an array of targets/
      );
    }
    expect(() => secretScanUtils.scanTargets(null as unknown, emptyAllowlist)).toThrow(
      /received null/
    );
    expect(() => secretScanUtils.scanTargets(42 as unknown, emptyAllowlist)).toThrow(
      /received number/
    );
    expect(() => secretScanUtils.scanTargets("abc" as unknown, emptyAllowlist)).toThrow(
      /received string/
    );
  });

  it("records an EISDIR target as a failure without aborting the loop", () => {
    const fixtureRoot = createFixtureDir();
    const key = awsKey();
    const good = seedTarget(fixtureRoot, "src/good.ts", `const a = "${key}";\n`);
    const directoryNamedLikeSource = path.join(fixtureRoot, "src", "directory.ts");
    fs.mkdirSync(directoryNamedLikeSource, { recursive: true });

    const { findings, failures } = scan([
      good,
      { absolutePath: directoryNamedLikeSource, relativePath: "src/directory.ts" },
    ]);

    expect(findings).toHaveLength(1);
    expect(failures).toHaveLength(1);
    expect(failures[0]).toMatchObject({ reason: "unreadable-target", code: "EISDIR" });
  });

  it("surfaces a permission-denied target and still scans its siblings", () => {
    const fixtureRoot = createFixtureDir();
    const key = awsKey();
    const readable = seedTarget(fixtureRoot, "src/readable.ts", `const a = "${key}";\n`);
    const locked = seedTarget(fixtureRoot, "src/locked.ts", `const b = "${key}";\n`);
    const trailing = seedTarget(fixtureRoot, "src/trailing.ts", `const c = "${key}";\n`);

    const denied = denyRead(locked.absolutePath);
    try {
      const { findings, failures } = scan([readable, locked, trailing]);

      // The sibling findings survive regardless of whether the mode bits are
      // enforced for this uid.
      expect(findings.map((finding: { file: string }) => finding.file)).toEqual(
        expect.arrayContaining(["src/readable.ts", "src/trailing.ts"])
      );

      if (denied) {
        expect(findings).toHaveLength(2);
        expect(failures).toHaveLength(1);
        expect(failures[0]).toMatchObject({
          target: "src/locked.ts",
          reason: "unreadable-target",
          code: "EACCES",
        });
        expect(String(failures[0].message)).not.toContain(key);
      } else {
        // Privileged uid: the read succeeds, so the target is simply scanned.
        expect(findings).toHaveLength(3);
        expect(failures).toHaveLength(0);
      }
    } finally {
      fs.chmodSync(locked.absolutePath, 0o644);
    }
  });

  it("recovers deterministically once a failure is repaired", () => {
    const fixtureRoot = createFixtureDir();
    const key = awsKey();
    const good = seedTarget(fixtureRoot, "src/good.ts", `const a = "${key}";\n`);
    const unstable = makeTarget(fixtureRoot, "src/unstable.ts");
    const alsoGood = seedTarget(fixtureRoot, "src/also-good.ts", `const b = "${key}";\n`);

    const targets = [good, unstable, alsoGood];

    // Iteration 1: the file does not exist yet.
    const first = scan(targets);
    expect(first.failures).toHaveLength(1);
    expect(first.findings).toHaveLength(2);

    // Iteration 2: same result, no stale state carried over.
    const second = scan(targets);
    expect(second.failures).toEqual(first.failures);
    expect(second.findings).toEqual(first.findings);

    // Iteration 3: the file appears and is now scanned, failures drop to zero.
    writeFixture(fixtureRoot, "src/unstable.ts", `const c = "${key}";\n`);
    const third = scan(targets);
    expect(third.failures).toHaveLength(0);
    expect(third.findings).toHaveLength(3);

    // Iteration 4: stable after recovery.
    const fourth = scan(targets);
    expect(fourth.findings).toEqual(third.findings);
    expect(fourth.failures).toEqual(third.failures);
  });

  it("processes every entry of a large batch with mixed valid and rejected targets", () => {
    const fixtureRoot = createFixtureDir();
    const key = awsKey();
    const valid: Array<{ absolutePath: string; relativePath: string }> = [];
    const invalid: unknown[] = [];

    for (let index = 0; index < 150; index += 1) {
      valid.push(seedTarget(fixtureRoot, `src/file-${index}.ts`, `const a = "${key}";\n`));
      invalid.push({ absolutePath: index, relativePath: `src/bad-${index}.ts` });
    }

    const { findings, failures } = scan([...valid, ...invalid]);

    expect(findings).toHaveLength(150);
    expect(failures).toHaveLength(150);
    expect(new Set(findings.map((finding: { file: string }) => finding.file)).size).toBe(150);
  });

  it("keeps parallel and interleaved executions independent", async () => {
    const fixtureRootA = createFixtureDir();
    const fixtureRootB = createFixtureDir();
    const key = awsKey();
    const targetsA = [
      seedTarget(fixtureRootA, "src/a.ts", `const a = "${key}";\n`),
      makeTarget(fixtureRootA, "src/missing.ts"),
    ];
    const targetsB = [seedTarget(fixtureRootB, "src/b.ts", `const b = "${key}";\n`)];

    const baselineA = scan(targetsA);
    const baselineB = scan(targetsB);

    const results = await Promise.all(
      Array.from({ length: 40 }, async (_, index) =>
        index % 2 === 0 ? scan(targetsA) : scan(targetsB)
      )
    );

    results.forEach((result, index) => {
      const baseline = index % 2 === 0 ? baselineA : baselineB;
      expect(result.findings).toEqual(baseline.findings);
      expect(result.failures).toEqual(baseline.failures);
    });
  });

  it("never leaks a scanned string through a failure record or the formatted log line", () => {
    const fixtureRoot = createFixtureDir();
    const planted = makeHighEntropySecret();
    const found = seedTarget(fixtureRoot, "src/found.ts", `const a = "${planted}";\n`);
    const absent = makeTarget(fixtureRoot, `src/absent-${planted.slice(0, 6)}.ts`);

    const { findings, failures } = scan([found, absent]);

    expect(findings).toHaveLength(1);
    expect(findings[0].preview).not.toContain(planted);
    expect(failures).toHaveLength(1);

    // The failure record is bounded and free of content.
    expect(String(failures[0].message).length).toBeLessThanOrEqual(200);
    expect(JSON.stringify(failures[0])).not.toContain(planted);

    // The rendered log line carries only path, reason, errno, and message.
    const message = secretScanUtils.formatFindings(findings, failures);
    expect(message).toContain("Secret scan failed");
    expect(message).toContain("could not be scanned");
    expect(message).toContain("ENOENT");
    expect(message).not.toContain(planted);
    expect(() => secretScanUtils.assertNoSecretsPrinted(message, findings)).not.toThrow();
    expect(() => secretScanUtils.assertNoSecretsPrinted(message, failures)).not.toThrow();
  });

  it("renders failures through formatFindings without changing the clean-run message", () => {
    // Backwards compatibility: the single-argument call is unchanged.
    expect(secretScanUtils.formatFindings([])).toBe(
      "Secret scan passed: No committed secrets were detected."
    );
    expect(secretScanUtils.formatFindings([], [])).toBe(
      "Secret scan passed: No committed secrets were detected."
    );

    const finding = {
      file: "src/a.ts",
      line: 3,
      column: 7,
      type: "aws-access-key",
      match: "redacted",
      preview: '"red...cted"',
      length: 8,
    };
    const failure = {
      target: "src/b.ts",
      reason: "unreadable-target",
      code: "EACCES",
      message: "EACCES: permission denied",
    };

    // Findings and failures are reported together when both are present.
    const both = secretScanUtils.formatFindings([finding], [failure]);
    expect(both).toContain("Secret scan failed: 1 potential secret(s) found.");
    expect(both).toContain("Secret scan failed: 1 target(s) could not be scanned.");
    expect(both).toContain("src/b.ts [unreadable-target] EACCES");
    expect(both).toContain("never treated as clean");

    // Failures alone still fail closed rather than reporting a clean run.
    const failuresOnly = secretScanUtils.formatFindings([], [failure]);
    expect(failuresOnly).not.toContain("passed");
    expect(failuresOnly).toContain("could not be scanned");
  });

  it("fails the run closed on an unreadable target and recovers after repair", () => {
    const fixtureRoot = createFixtureDir();
    const planted = makeHighEntropySecret();
    seedTarget(fixtureRoot, "src/clean.ts", "export const ok = 1;\n");
    const locked = seedTarget(fixtureRoot, "src/locked.ts", "export const ok = 2;\n");

    const run = () =>
      secretScanUtils.runSecretScan({
        backendRoot: fixtureRoot,
        allowlist: emptyAllowlist,
      });

    const denied = denyRead(locked.absolutePath);
    let blocked;
    try {
      blocked = run();
    } finally {
      fs.chmodSync(locked.absolutePath, 0o644);
    }

    if (denied) {
      // An unreadable target is never reported as a clean scan.
      expect(blocked.ok).toBe(false);
      expect(blocked.exitCode).toBe(1);
      expect(blocked.failures).toHaveLength(1);
      expect(blocked.failures[0].code).toBe("EACCES");
      expect(blocked.message).toContain("could not be scanned");
      expect(blocked.message).not.toContain(planted);
    } else {
      expect(blocked.ok).toBe(true);
      expect(blocked.failures).toHaveLength(0);
    }

    // Recovery loop: once repaired the same run passes cleanly.
    const recovered = run();
    expect(recovered.ok).toBe(true);
    expect(recovered.exitCode).toBe(0);
    expect(recovered.failures).toHaveLength(0);
    expect(recovered.message).toBe("Secret scan passed: No committed secrets were detected.");

    // A planted secret is still reported with a redacted preview.
    writeFixture(fixtureRoot, "src/leak.ts", `const v = "${planted}";\n`);
    const afterLeak = run();
    expect(afterLeak.ok).toBe(false);
    expect(afterLeak.exitCode).toBe(1);
    expect(afterLeak.findings).toHaveLength(1);
    expect(afterLeak.findings[0].preview).not.toContain(planted);
    expect(() =>
      secretScanUtils.assertNoSecretsPrinted(afterLeak.message, afterLeak.findings)
    ).not.toThrow();
  });

  it("survives hostile target descriptors and hostile read errors", () => {
    const fixtureRoot = createFixtureDir();
    const key = awsKey();
    const good = seedTarget(fixtureRoot, "src/good.ts", `const a = "${key}";\n`);

    // absolutePath getter that throws.
    const throwingPath = new Proxy(
      {},
      {
        get(_target, property) {
          if (property === "absolutePath") {
            throw new Error("hostile absolutePath");
          }
          return undefined;
        },
      }
    );

    // Every getter throws, so even the diagnostic read of relativePath for the
    // failure record must not escape (T1/T4).
    const allGettersThrow = new Proxy(
      {},
      {
        get() {
          throw new Error("hostile getter");
        },
      }
    );

    // relativePath getter that throws; absolutePath is still usable, so the
    // target is scanned and attributed to its absolute path.
    const throwingRelativePath = new Proxy(good, {
      get(target, property) {
        if (property === "relativePath") {
          throw new Error("hostile relativePath");
        }
        return Reflect.get(target, property);
      },
    });

    // A target with no relativePath falls back to the absolute path rather
    // than rendering a finding as "undefined".
    const withoutRelativePath = { absolutePath: good.absolutePath };

    const { findings, failures } = scan([
      throwingPath,
      allGettersThrow,
      throwingRelativePath,
      withoutRelativePath,
      good,
    ]);

    expect(failures).toHaveLength(2);
    expect(failures[0]).toMatchObject({ reason: "invalid-target", code: "EINVAL" });
    expect(failures[1]).toMatchObject({
      reason: "invalid-target",
      code: "EINVAL",
      target: "<unknown>",
    });
    // T3: all three usable entries share one absolutePath, so the file is
    // scanned once. The throwing relativePath getter is absorbed and the
    // finding falls back to the absolute path instead of rendering
    // "undefined".
    expect(findings).toHaveLength(1);
    expect(findings[0].file).toBe(good.absolutePath);
  });

  it("normalises hostile read errors into a bounded redacted record", () => {
    const fixtureRoot = createFixtureDir();
    const oversized = "x".repeat(5000);
    const target = {
      absolutePath: path.join(fixtureRoot, "src/ok.ts"),
      relativePath: "src/ok.ts",
    };

    // An Error whose code/message getters both throw.
    const throwingGetters = {
      get code(): string {
        throw new Error("hostile code");
      },
      get message(): string {
        throw new Error("hostile message");
      },
    };

    const cases: Array<[unknown, string, number]> = [
      // Oversized message is truncated to the T5 bound.
      [new Error(oversized), "unknown reason", 200],
      // Empty message falls back to the code.
      [new Error(""), "unknown reason", "unknown reason".length],
      // Throwing getters degrade to the defaults instead of propagating.
      [throwingGetters, "unknown reason", "unknown reason".length],
      // A thrown primitive that is not an Error is normalised, never coerced.
      [oversized, "unknown reason", "unknown reason".length],
    ];

    for (const [thrown, expectedCode, expectedMessageLength] of cases) {
      const readSpy = jest.spyOn(fs, "readFileSync").mockImplementation(() => {
        throw thrown;
      });
      const failures: Array<Record<string, unknown>> = [];
      try {
        secretScanUtils.scanTargets([target], emptyAllowlist, {
          onTargetError: (failure: Record<string, unknown>) => failures.push(failure),
        });
      } finally {
        readSpy.mockRestore();
      }

      expect(failures).toHaveLength(1);
      expect(failures[0]).toMatchObject({
        target: "src/ok.ts",
        reason: "unreadable-target",
        code: expectedCode,
      });
      // T5: the message is bounded and never carries file content.
      expect(String(failures[0].message).length).toBe(expectedMessageLength as number);
      expect(String(failures[0].message).length).toBeLessThanOrEqual(200);
      expect(String(failures[0].message)).not.toContain(oversized);
    }
  });

  it("keeps collectScanTargets output scannable and free of duplicates for nested roots", () => {
    const fixtureRoot = createFixtureDir();
    const key = awsKey();
    seedTarget(fixtureRoot, "src/lib/deep/nested.ts", `const a = "${key}";\n`);
    seedTarget(fixtureRoot, "src/top.ts", `const b = "${key}";\n`);

    // Overlapping roots previously produced one finding per covering root.
    const targets = secretScanUtils.collectScanTargets(fixtureRoot, {
      scanRoots: ["src", "src/lib", "src/lib/deep"],
      exampleFiles: [],
    });
    const { findings, failures } = scan(targets);

    expect(failures).toHaveLength(0);
    expect(findings).toHaveLength(2);
    expect(new Set(findings.map((finding: { file: string }) => finding.file))).toEqual(
      new Set(["src/lib/deep/nested.ts", "src/top.ts"])
    );
  });
});
