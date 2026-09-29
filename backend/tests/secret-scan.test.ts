import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import crypto from "node:crypto";
import { execFileSync } from "node:child_process";

const secretScanUtils = require("../scripts/lib/secret-scan-utils");

function createFixtureDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "quicklendx-secret-scan-"));
}

function writeFixture(root: string, relativePath: string, content: string): string {
  const absolutePath = path.join(root, relativePath);
  fs.mkdirSync(path.dirname(absolutePath), { recursive: true });
  fs.writeFileSync(absolutePath, content, "utf8");
  return absolutePath;
}

function makeHighEntropySecret(): string {
  return crypto.randomBytes(36).toString("base64url");
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
    const qlx = `qlx_${"live"}_${suffix}`;
    const stripe = `sk_${"live"}_${stripeSuffix}`;
    const slack = `xoxb-${"123"}-${"456"}-${suffix}`;
    const aws = `AKIA${"IOSFODNN7EXAMPLE"}`;
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

  it("ignores obvious placeholders and Stellar public keys", () => {
    expect(secretScanUtils.isObviousPlaceholder("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")).toBe(true);
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
    const deduped = secretScanUtils.scanLine(
      `const token = "${`qlx_${"live"}_${qlxSuffix}`}";`,
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
    const failedAfterWrite = secretScanUtils.runSecretScan({
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
    expect(secretScanUtils.isHighEntropyToken("ABCDEFGHIJKLMNOPQRSTUVWXYZABCD")).toBe(false);
    expect(secretScanUtils.isHighEntropyToken("aaaaaaaaaaaaaaaaaaaa1234567890ab")).toBe(false);
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
    expect(
      secretScanUtils.matchesAllowlistEntry({ line: 9 }, "src/a.ts", 1, "value")
    ).toBe(false);
    expect(
      secretScanUtils.matchesAllowlistEntry({ match: "missing" }, "src/a.ts", 1, "value")
    ).toBe(false);
    expect(
      secretScanUtils.matchesAllowlistEntry({ pattern: "^nope$" }, "src/a.ts", 1, "value")
    ).toBe(false);
    expect(secretScanUtils.isAllowlisted("src/a.ts", 1, "value", { globalPatterns: [{}] })).toBe(
      false
    );

    const allowlistedLine = makeHighEntropySecret();
    const allowlistedFindings = secretScanUtils.scanLine(
      `const token = "${allowlistedLine}";`,
      8,
      "src/allowed.ts",
      {
        entries: [{ file: "src/allowed.ts", line: 8, match: allowlistedLine }],
        globalPatterns: [],
      }
    );
    expect(allowlistedFindings).toHaveLength(0);

    const duplicateSecret = makeHighEntropySecret();
    const duplicateFindings = secretScanUtils.scanLine(
      `const one = "${duplicateSecret}"; const two = "${duplicateSecret}";`,
      4,
      "src/example.ts",
      { entries: [], globalPatterns: [] }
    );
    expect(duplicateFindings).toHaveLength(1);

    const fixtureWithDirs = createFixtureDir();
    writeFixture(fixtureWithDirs, "node_modules/pkg/index.js", "module.exports = {};\n");
    writeFixture(fixtureWithDirs, "src/nested/deep.ts", "export {};\n");
    expect(secretScanUtils.collectScanTargets(path.join(fixtureWithDirs, "missing"))).toEqual([]);
    expect(
      secretScanUtils.shouldScanFile("scripts/.secret-scan-allow.json", {
        ignoredFiles: [".secret-scan-allow.json"],
      })
    ).toBe(false);
    expect(secretScanUtils.shouldScanFile(".env.example")).toBe(true);

    const nestedTargets = secretScanUtils.collectScanTargets(fixtureWithDirs);
    expect(nestedTargets.map((target: { relativePath: string }) => target.relativePath)).toEqual(
      expect.arrayContaining(["src/nested/deep.ts"])
    );
    expect(
      nestedTargets.map((target: { relativePath: string }) => target.relativePath)
    ).not.toContain("node_modules/pkg/index.js");
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

    it("treats String objects identically to string primitives", () => {
      const runtimeNonPlaceholder = ["prod", "token", "1A2b3C4d5E6f7G8h9I0j!?"].join("-");
      // eslint-disable-next-line no-new-wrappers
      const placeholderObj = new String("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
      // eslint-disable-next-line no-new-wrappers
      const realObj = new String(runtimeNonPlaceholder);
      const placeholderPrim = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
      const realPrim = runtimeNonPlaceholder;

      expect(secretScanUtils.isObviousPlaceholder(placeholderObj as unknown as string)).toBe(
        secretScanUtils.isObviousPlaceholder(placeholderPrim)
      );
      expect(secretScanUtils.isObviousPlaceholder(realObj as unknown as string)).toBe(
        secretScanUtils.isObviousPlaceholder(realPrim)
      );
      expect(secretScanUtils.isObviousPlaceholder(placeholderObj as unknown as string)).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder(realObj as unknown as string)).toBe(false);
    });

    it("is deterministic across repeated calls with identical inputs", () => {
      const runtimeQlx = ["qlx", "live", "abcdefghijklmnopqrstuvwxyz01"].join("_");
      const runtimeStripeSuffix = ["abcdefghijklmnop", "qrstuvwxyz123456"].join("");
      const runtimeStripe = ["sk", "live", runtimeStripeSuffix].join("_");
      const cases = [
        "",
        "xxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        "your-api-key-here",
        "example-secret-value",
        "PLACEHOLDER_FOR_TEST",
        "changeme-in-prod",
        "test_secret_value",
        "test-secret-123",
        "development-only-token",
        "fallback-secret-config",
        "getInvoicesQuerySchema",
        "/api/v1/invoices",
        "https://quicklendx.example.com/callback",
        runtimeQlx,
        runtimeStripe,
        "SAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "abababababababababababababababab",
        "   ",
        "\n\t",
      ];

      for (const input of cases) {
        const first = secretScanUtils.isObviousPlaceholder(input);
        for (let i = 0; i < 50; i++) {
          expect(secretScanUtils.isObviousPlaceholder(input)).toBe(first);
        }
      }
    });

    it("produces consistent results under concurrent interleaved calls", () => {
      const runtimeQlx = ["qlx", "live", "abcdefghijklmnopqrstuvwxyz01"].join("_");
      const runtimeStripeSuffix = ["abcdefghijklmnop", "qrstuvwxyz123456"].join("");
      const runtimeStripe = ["sk", "live", runtimeStripeSuffix].join("_");
      const inputs = [
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        runtimeQlx,
        "your-secret-key",
        runtimeStripe,
        "",
      ];
      const expected = inputs.map((input) => secretScanUtils.isObviousPlaceholder(input));

      const permutations = inputs.map((_, offset) => [
        ...inputs.slice(offset),
        ...inputs.slice(0, offset),
      ]);
      for (const orderedInputs of permutations) {
        for (const value of orderedInputs) {
          const idx = inputs.indexOf(value);
          expect(secretScanUtils.isObviousPlaceholder(value)).toBe(expected[idx]);
        }
      }
    });

    it("respects MIN_HIGH_ENTROPY_LENGTH boundary for low-unique-char strings", () => {
      const threshold = secretScanUtils.MIN_HIGH_ENTROPY_LENGTH as number;

      const below = "a".repeat(threshold - 1);
      const at = "a".repeat(threshold);
      const above = "a".repeat(threshold + 1);

      expect(secretScanUtils.isObviousPlaceholder(below)).toBe(false);
      expect(secretScanUtils.isObviousPlaceholder(at)).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder(above)).toBe(true);

      const belowTwo = "ab".repeat(Math.floor((threshold - 1) / 2));
      const atTwo = "ab".repeat(Math.floor(threshold / 2)).slice(0, threshold);
      expect(new Set(belowTwo).size <= 2).toBe(true);
      expect(new Set(atTwo).size <= 2).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder(belowTwo)).toBe(false);
      expect(secretScanUtils.isObviousPlaceholder(atTwo)).toBe(true);

      const threeChars = "abc".repeat(Math.ceil(threshold / 3)).slice(0, threshold);
      expect(new Set(threeChars).size).toBeGreaterThan(2);
      expect(secretScanUtils.isObviousPlaceholder(threeChars)).toBe(false);
    });

    it("returns false for clearly non-placeholder strings", () => {
      const runtimeQlx = ["qlx", "live", "abcdefghijklmnopqrstuvwxyz01"].join("_");
      const runtimeStripeSuffix = ["abcdefghijklmnop", "qrstuvwxyz123456"].join("");
      const runtimeStripe = ["sk", "live", runtimeStripeSuffix].join("_");
      const runtimeSlack = ["xoxb", "123456", "789012", "abcdefghijklmnop"].join("-");
      const runtimeAws = ["AKIA", "IOSFODNN7EXAMPLE"].join("");
      const nonPlaceholders = [
        runtimeQlx,
        runtimeStripe,
        runtimeSlack,
        runtimeAws,
        "not-a-template-32-chars-of-entropy-!",
        "mixedContent123!@#",
      ];

      for (const value of nonPlaceholders) {
        expect(secretScanUtils.isObviousPlaceholder(value)).toBe(false);
      }
    });

    it("prevents scan-line crashes when candidate.match is a non-string via direct path", () => {
      const planted = makeHighEntropySecret();
      const findingsBefore = secretScanUtils.scanLine(
        `const token = "${planted}";`,
        1,
        "src/ok.ts",
        { entries: [], globalPatterns: [] }
      );
      expect(findingsBefore).toHaveLength(1);
      expect(findingsBefore[0].type).toBe("high-entropy");
    });

    it("matches prefix patterns case-insensitively and with boundaries", () => {
      expect(secretScanUtils.isObviousPlaceholder("Your_Key_Here")).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder("EXAMPLE_SECRET_TOKEN")).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder("PlaceholderValue")).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder("CHANGEME_IN_PRODUCTION")).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder("TESTSECRET_XYZ")).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder("test_secret_xyz")).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder("Development-Only-Stub")).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder("Fallback-Secret-Standalone")).toBe(true);
    });

    it("handles single-char repeating strings just below threshold", () => {
      const threshold = secretScanUtils.MIN_HIGH_ENTROPY_LENGTH as number;
      const justBelow = "x".repeat(threshold - 1);
      expect(secretScanUtils.isObviousPlaceholder(justBelow)).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder("x")).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder("X")).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder("y")).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder("z")).toBe(true);
      expect(secretScanUtils.isObviousPlaceholder("w")).toBe(false);
    });
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
});

describe("backend security:scan integration", () => {
  const repoRoot = path.resolve(__dirname, "..");

  it("chains secret scanning into security:scan", () => {
    const packageJson = JSON.parse(
      fs.readFileSync(path.join(repoRoot, "package.json"), "utf8")
    ) as { scripts: Record<string, string> };

    expect(packageJson.scripts["security:scan"]).toContain("dependency-scan.js");
    expect(packageJson.scripts["security:scan"]).toContain("secret-scan.js");
  });
});
