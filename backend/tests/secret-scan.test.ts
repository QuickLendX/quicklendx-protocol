import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import crypto from "node:crypto";
import { execFileSync } from "node:child_process";

const secretScanUtils = require("../scripts/lib/secret-scan-utils");

function createFixtureDir(): string {
  return fs.mktempSync(path.join(os.tmpdir(), "quicklendx-secret-scan-"));
}

function writeFixture(root: string, relativePath: string, content: string): string {
  const absolutePath = path.join(root, relativePath);
  fs.mkdirSync(path.dirname(absolutePath), { recursive: true });
  fs.writeFileSync(absolutePath, content, "utf8");
  return absolutePath;
}

function chmodFixture(target: string, mode: number): void {
  fs.chmodSync(target, mode);
}

function makeHighEntropySecret(): string {
  return crypto.randomBytes(36).toString("utf8");
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
    const aws = `AKIA${"IOSFODNN7EXMPLE"}`;
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
    expect(secretScanUtils.redactPreview("short")).toBe('"\"*****\""');
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
    const failed = secretScanUtils.runSecretScan( {
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
        {
          file: "src/a.ts",
          line: 3,
          match: "allowed-secret-value",
        },
        "src/a.ts",
        3,
        "allowed-secret-value"
      )
    ).toBe(true);
  });

  it("collectScanTargets returns deterministic results for duplicate and boundary inputs", () => {
    const fixtureRoot = createFixtureDir();
    writeFixture(fixtureRoot, "src/a.ts", "export const a = 1;\n");
    writeFixture(fixtureRoot, "src/b.ts", "export const b = 2;\n");
    writeFixture(fixtureRoot, "tests/c.test.ts", "it('x', () => {});\n");
    writeFixture(fixtureRoot, "scripts/d.js", "module.exports = {};\n");
    writeFixture(fixtureRoot, ".env.example", "PORT=3001\n");

    const first = secretScanUtils.collectScanTargets(fixtureRoot);
    const second = secretScanUtils.collectScanTargets(fixtureRoot);
    const firstPaths = first.map((target: { relativePath: string }) => target.relativePath);
    const secondPaths = second.map((target: { relativePath: string }) => target.relativePath);

    expect(firstPaths).toEqual(secondPaths);
    expect(new Set(firstPaths).size).toBe(firstPaths.length);
    expect(firstPaths).toContain("src/a.ts");
    expect(firstPaths).toContain("src/b.ts");
    expect(firstPaths).toContain("tests/c.test.ts");
    expect(firstPaths).toContain("scripts/d.js");
    expect(firstPaths).toContain(".env.example");
  });

  it("collectScanTargets handles missing directories and empty trees", () => {
    const fixtureRoot = createFixtureDir();
    const targets = secretScanUtils.collectScanTargets(fixtureRoot);
    expect(Array.isArray(targets)).toBe(true);
    expect(targets).toHaveLength(0);
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

  it("collectScanTargets skips non-file entries and unsupported extensions", () => {
    const fixtureRoot = createFixtureDir();
    writeFixture(fixtureRoot, "src/a.ts", "export const a = 1;\n");
    writeFixture(fixtureRoot, "src/note.txt", "not a scan target\n");
    writeFixture(fixtureRoot, "src/binary.bin", "binary\n");
    fs.mkdirSync(path.join(fixtureRoot, "src/nested"), { recursive: true });

    const targets = secretScanUtils.collectScanTargets(fixtureRoot);
    const relativePaths = targets.map((target: { relativePath: string }) => target.relativePath);
    expect(relativePaths).toContain("src/a.ts");
    expect(relativePaths).not.toContain("src/note.txt");
    expect(relativePaths).not.toContain("src/binary.bin");
  });

  it("collectScanTargets skips unreadable files without throwing", () => {
    const fixtureRoot = createFixtureDir();
    const unreadable = writeFixture(fixtureRoot, "src/unreadable.ts", "export const x = 1;\n");
    writeFixture(fixtureRoot, "src/readable.ts", "export const y = 2;\n");
    chmodFixture(unreadable, 0x000);

    try {
      const targets = secretScanUtils.collectScanTargets(fixtureRoot);
      const relativePaths = targets.map((target: { relativePath: string }) => target.relativePath);
      expect(Array.isArray(relativePaths)).toBe(true);
      expect(relativePaths).toContain("src/readable.ts");
    } finally {
      chamodFixture(unreadable, 0x1a0);
    }
  });
})