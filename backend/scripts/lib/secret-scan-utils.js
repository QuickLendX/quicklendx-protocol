"use strict";

/**
 * Committed-secret scanner primitives.
 *
 * Invariants relied upon by the rest of this module and by
 * `backend/scripts/secret-scan.js`:
 *
 * 1. FAIL CLOSED. Anything the scanner cannot positively evaluate as safe must
 *    produce a finding or a thrown error. It must never resolve to "no
 *    findings" as a result of malformed configuration, unparseable input, or
 *    unreadable files. This is the invariant that an empty allowlist pattern
 *    (`{ "pattern": "" }`) previously violated: it matches every candidate and
 *    silently suppressed the entire gate.
 * 2. NO SHARED MUTABLE STATE. The module-level regexes are exported (and are
 *    therefore reachable and mutable by any importer), so scanning always runs
 *    against a private clone. `lastIndex` on a shared global regex makes
 *    results depend on call ordering, and a non-global regex in an exec loop
 *    never advances `lastIndex`, which spins forever.
 * 3. REDACTION IS BOUNDED. `redactPreview` never reveals more than
 *    `MIN_PREVIEW_EDGE_LENGTH` leading and trailing characters, and never
 *    renders a partial preview that reconstructs the value.
 * 4. OUTPUT NEVER CONTAINS A SECRET. Error messages identify findings by
 *    repo-relative path, line, and allowlist index. They never embed matched
 *    secret text, nor absolute filesystem paths from the scanning machine.
 */

const fs = require("node:fs");
const path = require("node:path");

const DEFAULT_SCAN_ROOTS = ["src", "tests", "scripts"];
const DEFAULT_EXAMPLE_FILES = [".env.example"];
const DEFAULT_EXTENSIONS = new Set([
  ".ts",
  ".js",
  ".json",
  ".md",
  ".yaml",
  ".yml",
  ".sql",
  ".example",
]);
const DEFAULT_IGNORED_DIRS = new Set([
  "node_modules",
  "coverage",
  ".git",
  "dist",
  "build",
]);
const MIN_HIGH_ENTROPY_LENGTH = 32;
const MIN_HIGH_ENTROPY_SCORE = 4.5;
const MIN_UNIQUE_CHARACTERS = 10;
const MIN_PREVIEW_EDGE_LENGTH = 4;
// Below this length a head+tail preview reconstructs most of the value (a
// 9-character secret would reveal 8 of 9 characters), so short values are
// masked in full instead.
const MIN_PARTIAL_PREVIEW_LENGTH = MIN_PREVIEW_EDGE_LENGTH * 2 + 8;
const PLAIN_STRING_REGEX = /'([^'\\]|\\.)*'|"([^"\\]|\\.)*"/g;

const KNOWN_SECRET_PATTERNS = [
  {
    name: "quicklendx-api-key",
    regex: /qlx_(?:test|live|dev)_[A-Za-z0-9_=-]{16,}/g,
  },
  {
    name: "stripe-secret-key",
    regex: /sk_(?:live|test)_[A-Za-z0-9]{20,}/g,
  },
  {
    name: "slack-bot-token",
    regex: /xoxb-[0-9]+-[0-9]+-[A-Za-z0-9-]+/g,
  },
  {
    name: "aws-access-key",
    regex: /AKIA[0-9A-Z]{16}/g,
  },
  {
    name: "stellar-secret-seed",
    regex: /\bS[A-Z2-7]{55}\b/g,
  },
];

function shannonEntropy(value) {
  if (!value) {
    return 0;
  }

  const counts = new Map();
  for (const char of value) {
    counts.set(char, (counts.get(char) || 0) + 1);
  }

  let entropy = 0;
  for (const count of counts.values()) {
    const probability = count / value.length;
    entropy -= probability * Math.log2(probability);
  }

  return entropy;
}

function isHexString(value) {
  return /^[0-9a-fA-F]+$/.test(value) || /^0x[0-9a-fA-F]+$/.test(value);
}

function isIdentifierLikeString(value) {
  return (
    /^[A-Za-z][A-Za-z0-9_$/-]*$/.test(value) &&
    /[a-z]/.test(value) &&
    /[A-Z]/.test(value) &&
    !/[0-9]/.test(value)
  );
}

function isObviousPlaceholder(value) {
  if (!value) {
    return true;
  }

  if (/^x+$/i.test(value) || /^y+$/i.test(value) || /^z+$/i.test(value)) {
    return true;
  }

  const uniqueChars = new Set(value);
  if (uniqueChars.size <= 2 && value.length >= MIN_HIGH_ENTROPY_LENGTH) {
    return true;
  }

  if (/^(your_|example_|placeholder|changeme|test[-_]?secret|development-only|fallback-secret)/i.test(value)) {
    return true;
  }

  if (isIdentifierLikeString(value)) {
    return true;
  }

  if (/^\/api\//.test(value) || /^https?:\/\//.test(value)) {
    return true;
  }

  return false;
}

function hasMixedCharacterClasses(value) {
  const classes = [
    /[a-z]/.test(value),
    /[A-Z]/.test(value),
    /[0-9]/.test(value),
    /[^A-Za-z0-9]/.test(value),
  ];

  return classes.filter(Boolean).length >= 2;
}

function isStellarStrKeyLike(value) {
  return /^[GX][A-Z2-7]{55}$/.test(value);
}

function isHighEntropyToken(value) {
  if (value.length < MIN_HIGH_ENTROPY_LENGTH) {
    return false;
  }

  if (!/^[A-Za-z0-9+/=_-]+$/.test(value)) {
    return false;
  }

  if (isHexString(value)) {
    return false;
  }

  if (isStellarStrKeyLike(value)) {
    return false;
  }

  if (isObviousPlaceholder(value)) {
    return false;
  }

  if (new Set(value).size < MIN_UNIQUE_CHARACTERS) {
    return false;
  }

  if (!hasMixedCharacterClasses(value)) {
    return false;
  }

  return shannonEntropy(value) >= MIN_HIGH_ENTROPY_SCORE;
}

/**
 * Renders a bounded, non-reconstructable preview of a matched value.
 *
 * The preview is diagnostic only: it must let an operator correlate a finding
 * with a real secret without disclosing it. A head+tail render is only safe
 * once the value is long enough that the elided middle is meaningful, so
 * shorter values are masked completely.
 */
function redactPreview(value) {
  if (!value) {
    return '""';
  }

  if (value.length < MIN_PARTIAL_PREVIEW_LENGTH) {
    return `"${"*".repeat(value.length)}"`;
  }

  return `"${value.slice(0, MIN_PREVIEW_EDGE_LENGTH)}...${value.slice(-MIN_PREVIEW_EDGE_LENGTH)}"`;
}

/**
 * Returns a private, zero-`lastIndex` copy of a scanner regex.
 *
 * The exported `KNOWN_SECRET_PATTERNS` and `PLAIN_STRING_REGEX` are shared
 * module state that any importer can mutate. Scanning a clone keeps results
 * independent of call ordering and of concurrent/interleaved callers, and
 * keeps a caller's in-flight `exec` loop from having its cursor moved.
 *
 * The `g` flag is required rather than assumed: iterating a non-global regex
 * with `exec` re-yields the same match forever, which hung the scan instead of
 * failing it.
 */
function cloneScanRegex(regex, label) {
  if (!(regex instanceof RegExp)) {
    throw new TypeError(
      `Secret scan pattern "${label}" must be a RegExp, received ${describeType(regex)}.`
    );
  }

  if (!regex.global) {
    throw new TypeError(
      `Secret scan pattern "${label}" must use the global (g) flag so every match on a line is scanned.`
    );
  }

  return new RegExp(regex.source, regex.flags);
}

function describeType(value) {
  if (value === null) {
    return "null";
  }

  if (Array.isArray(value)) {
    return "array";
  }

  return typeof value;
}

function collectRegexMatches(line, patternDef) {
  const matches = [];
  const regex = cloneScanRegex(patternDef.regex, patternDef.name);

  for (const match of line.matchAll(regex)) {
    matches.push({
      type: patternDef.name,
      match: match[0],
      column: match.index + 1,
    });
  }

  return matches;
}

function unquoteString(literal) {
  const quote = literal[0];
  if (quote !== "'" && quote !== '"' && quote !== "`") {
    return literal;
  }

  return literal.slice(1, -1);
}

function collectQuotedStringMatches(line) {
  const matches = [];

  for (const match of line.matchAll(cloneScanRegex(PLAIN_STRING_REGEX, "plain-string"))) {
    const literal = match[0];
    matches.push({
      literal,
      value: unquoteString(literal),
      column: match.index + 1,
    });
  }

  return matches;
}

function collectHighEntropyMatches(line) {
  const matches = [];

  for (const quoted of collectQuotedStringMatches(line)) {
    if (!isHighEntropyToken(quoted.value)) {
      continue;
    }

    matches.push({
      type: "high-entropy",
      match: quoted.value,
      column: quoted.column,
    });
  }

  return matches;
}

function normalizeAllowlist(allowlist) {
  if (!allowlist || typeof allowlist !== "object") {
    return { entries: [], globalPatterns: [] };
  }

  return {
    entries: Array.isArray(allowlist.entries) ? allowlist.entries : [],
    globalPatterns: Array.isArray(allowlist.globalPatterns)
      ? allowlist.globalPatterns
      : [],
  };
}

/**
 * Compiles an allowlist `pattern` into a RegExp, or returns `null` when the
 * pattern cannot be trusted.
 *
 * Returning `null` means "this entry does not allowlist anything", which is
 * the fail-closed outcome: the candidate stays in the report instead of being
 * silently suppressed. This deliberately covers
 *   - `""` / whitespace-only patterns, which match every candidate and would
 *     otherwise disable the whole gate with a one-character typo;
 *   - patterns that do not compile, which previously threw a `SyntaxError` out
 *     of `scanLine` and aborted the entire scan with no indication of which
 *     allowlist entry was at fault;
 *   - non-string, non-RegExp patterns.
 */
function compilePattern(pattern) {
  if (pattern instanceof RegExp) {
    return new RegExp(pattern.source, pattern.flags);
  }

  if (typeof pattern !== "string" || pattern.trim().length === 0) {
    return null;
  }

  try {
    return new RegExp(pattern);
  } catch {
    return null;
  }
}

function matchesAllowlistEntry(entry, relativePath, lineNumber, matchValue) {
  if (!entry || typeof entry !== "object") {
    return false;
  }

  // Each property is read exactly once so a reentrant or instrumented entry
  // observes a deterministic access pattern no matter which fields are
  // present: a getter on `pattern` fires once per candidate, not once per
  // internal check.
  const file = entry.file;
  const line = entry.line;
  const match = entry.match;
  const pattern = entry.pattern;

  if (file === undefined && line === undefined && match === undefined && pattern === undefined) {
    return false;
  }

  if (file !== undefined && file !== relativePath) {
    return false;
  }

  if (line !== undefined && Number(line) !== lineNumber) {
    return false;
  }

  if (match !== undefined && !matchValue.includes(match)) {
    return false;
  }

  if (pattern !== undefined) {
    const compiled = compilePattern(pattern);
    if (!compiled || !compiled.test(matchValue)) {
      return false;
    }
  }

  return true;
}

function isAllowlistedIn(normalized, relativePath, lineNumber, matchValue) {
  for (const entry of normalized.entries) {
    if (matchesAllowlistEntry(entry, relativePath, lineNumber, matchValue)) {
      return true;
    }
  }

  for (const entry of normalized.globalPatterns) {
    if (!entry || typeof entry !== "object") {
      continue;
    }

    const patternSource = entry.pattern;
    if (patternSource === undefined) {
      continue;
    }

    const pattern = compilePattern(patternSource);
    if (pattern && pattern.test(matchValue)) {
      return true;
    }
  }

  return false;
}

function isAllowlisted(relativePath, lineNumber, matchValue, allowlist) {
  return isAllowlistedIn(normalizeAllowlist(allowlist), relativePath, lineNumber, matchValue);
}

/**
 * A candidate from two different sources is considered the same leak only when
 * the matched text is identical; a known-format match suppresses the
 * high-entropy match covering the same text so a secret is reported once under
 * its most specific rule.
 */
function overlapsMatch(left, right) {
  return left.match === right.match;
}

/**
 * Rejects inputs `scanLine` cannot reason about.
 *
 * Previously a non-string `line` was coerced by `RegExp.prototype.exec` (null
 * became the literal "null"), so a caller that lost its file contents scanned
 * the string "null", found nothing, and reported a clean pass. Invalid
 * line numbers likewise produced findings whose `line` field could never
 * reconcile with an allowlist entry. Failing loudly keeps the gate's
 * fail-closed guarantee.
 */
function assertScannableLineArguments(line, lineNumber, relativePath) {
  if (typeof line !== "string") {
    throw new TypeError(
      `scanLine requires a string line, received ${describeType(line)}.`
    );
  }

  if (typeof relativePath !== "string" || relativePath.length === 0) {
    throw new TypeError("scanLine requires a non-empty relativePath string.");
  }

  if (!Number.isInteger(lineNumber) || lineNumber < 1) {
    throw new TypeError(
      `scanLine requires a positive integer lineNumber, received ${describeType(lineNumber)}.`
    );
  }
}

/**
 * Scans one line and returns its findings.
 *
 * Findings are ordered deterministically: known-format matches first, in
 * `KNOWN_SECRET_PATTERNS` declaration order and then by column, followed by
 * high-entropy matches by column. Identical (type, match) pairs on the same
 * line are reported once, anchored at the first column, so repeating a secret
 * in one line cannot inflate the finding count or shift its reported position.
 */
function scanLine(line, lineNumber, relativePath, allowlist) {
  assertScannableLineArguments(line, lineNumber, relativePath);

  const findings = [];
  const seen = new Set();
  const normalizedAllowlist = normalizeAllowlist(allowlist);

  const patternMatches = KNOWN_SECRET_PATTERNS.flatMap((patternDef) =>
    collectRegexMatches(line, patternDef)
  );
  const highEntropyMatches = collectHighEntropyMatches(line).filter((candidate) =>
    !patternMatches.some((patternMatch) => overlapsMatch(patternMatch, candidate))
  );
  const allMatches = [...patternMatches, ...highEntropyMatches];

  for (const candidate of allMatches) {
    if (isObviousPlaceholder(candidate.match)) {
      continue;
    }

    const dedupeKey = `${lineNumber}:${candidate.type}:${candidate.match}`;
    if (seen.has(dedupeKey)) {
      continue;
    }
    seen.add(dedupeKey);

    if (isAllowlistedIn(normalizedAllowlist, relativePath, lineNumber, candidate.match)) {
      continue;
    }

    findings.push({
      file: relativePath,
      line: lineNumber,
      column: candidate.column,
      type: candidate.type,
      match: candidate.match,
      preview: redactPreview(candidate.match),
      length: candidate.match.length,
    });
  }

  return findings;
}

function scanFileContent(content, relativePath, allowlist) {
  if (typeof content !== "string") {
    throw new TypeError(
      `scanFileContent requires string content, received ${describeType(content)}.`
    );
  }

  const lines = content.split(/\r?\n/);
  return lines.flatMap((line, index) =>
    scanLine(line, index + 1, relativePath, allowlist)
  );
}

function shouldScanFile(relativePath, options = {}) {
  const extensions = options.extensions || DEFAULT_EXTENSIONS;
  const ignoredFiles = new Set(options.ignoredFiles || [".secret-scan-allow.json"]);

  if (ignoredFiles.has(path.basename(relativePath))) {
    return false;
  }

  const extension = path.extname(relativePath);
  if (extensions.has(extension)) {
    return true;
  }

  return DEFAULT_EXAMPLE_FILES.includes(path.basename(relativePath));
}

function walkDirectory(absoluteDir, relativeDir, files = []) {
  if (!fs.existsSync(absoluteDir)) {
    return files;
  }

  let entries;
  try {
    entries = fs.readdirSync(absoluteDir, { withFileTypes: true });
  } catch (error) {
    // Fail closed: an unreadable directory could hide secrets, so abort the
    // scan instead of skipping it. The message carries the repo-relative path
    // and the errno code only, never the absolute path of the build machine.
    throw new Error(
      `Failed to list ${relativeDir || "."} during secret scan (${describeFileSystemError(error)}). ` +
        "Unreadable paths are a scan failure, not a clean result."
    );
  }

  for (const entry of entries) {
    if (entry.name.startsWith(".")) {
      continue;
    }

    const absolutePath = path.join(absoluteDir, entry.name);
    const relativePath = relativeDir
      ? path.posix.join(relativeDir.replace(/\\/g, "/"), entry.name)
      : entry.name;

    if (entry.isDirectory()) {
      if (DEFAULT_IGNORED_DIRS.has(entry.name)) {
        continue;
      }
      walkDirectory(absolutePath, relativePath, files);
      continue;
    }

    files.push({
      absolutePath,
      relativePath: relativePath.replace(/\\/g, "/"),
    });
  }

  return files;
}

function describeFileSystemError(error) {
  if (error && typeof error.code === "string" && error.code.length > 0) {
    return error.code;
  }

  return "unknown error";
}

function collectScanTargets(backendRoot, options = {}) {
  const scanRoots = options.scanRoots || DEFAULT_SCAN_ROOTS;
  const exampleFiles = options.exampleFiles || DEFAULT_EXAMPLE_FILES;
  const targets = [];

  for (const root of scanRoots) {
    const absoluteRoot = path.join(backendRoot, root);
    const relativeRoot = root.replace(/\\/g, "/");
    targets.push(...walkDirectory(absoluteRoot, relativeRoot));
  }

  for (const exampleFile of exampleFiles) {
    const absolutePath = path.join(backendRoot, exampleFile);
    if (fs.existsSync(absolutePath)) {
      targets.push({
        absolutePath,
        relativePath: exampleFile.replace(/\\/g, "/"),
      });
    }
  }

  return targets.filter((target) => shouldScanFile(target.relativePath, options));
}

function scanTargets(targets, allowlist) {
  const findings = [];

  for (const target of targets) {
    let content;
    try {
      content = fs.readFileSync(target.absolutePath, "utf8");
    } catch (error) {
      // Fail closed: a file the scan cannot read is a file whose contents were
      // never checked, so it must abort the gate rather than pass silently.
      // Identified by repo-relative path and errno so an operator can act on
      // it from CI logs without the absolute path of the build machine.
      throw new Error(
        `Failed to read ${target.relativePath} during secret scan (${describeFileSystemError(error)}). ` +
          "Unreadable files are a scan failure, not a clean result."
      );
    }

    findings.push(...scanFileContent(content, target.relativePath, allowlist));
  }

  return findings;
}

function scanBackend(backendRoot, options = {}) {
  const allowlist = options.allowlist || loadAllowlist(options.allowlistPath, backendRoot);
  const targets = collectScanTargets(backendRoot, options);
  return scanTargets(targets, allowlist);
}

/**
 * Validates allowlist patterns at the configuration boundary.
 *
 * Runtime matching already fails closed on an uncompilable pattern, but that
 * degrades silently into "not allowlisted" and would show up only as a flood of
 * findings. Rejecting the file at load time turns a typo into one actionable
 * error. The message names only the entry's index and field, never the pattern
 * text, so a pattern copied from a leaked secret is not echoed to CI logs.
 */
function assertAllowlistPatternsCompilable(normalized, source) {
  const groups = [
    ["entries", normalized.entries],
    ["globalPatterns", normalized.globalPatterns],
  ];

  for (const [groupName, items] of groups) {
    items.forEach((item, index) => {
      if (!item || typeof item !== "object" || item.pattern === undefined) {
        return;
      }

      if (compilePattern(item.pattern) === null) {
        throw new Error(
          `Failed to parse secret scan allowlist: ${source}.${groupName}[${index}].pattern ` +
            "must be a non-empty regular expression that compiles."
        );
      }
    });
  }
}

function loadAllowlist(allowlistPath, backendRoot = process.cwd()) {
  const resolvedPath =
    allowlistPath || path.join(backendRoot, "scripts", ".secret-scan-allow.json");

  if (!fs.existsSync(resolvedPath)) {
    return normalizeAllowlist(null);
  }

  let raw;
  try {
    raw = fs.readFileSync(resolvedPath, "utf8");
  } catch (error) {
    throw new Error(
      `Failed to read secret scan allowlist (${describeFileSystemError(error)}).`
    );
  }

  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    throw new Error(`Failed to parse secret scan allowlist: ${error.message}`);
  }

  const normalized = normalizeAllowlist(parsed);
  assertAllowlistPatternsCompilable(normalized, "allowlist");
  return normalized;
}

function formatFinding(finding) {
  return (
    `  ${finding.file}:${finding.line}:${finding.column} ` +
    `[${finding.type}] preview: ${finding.preview} (${finding.length} chars)`
  );
}

function formatFindings(findings) {
  if (findings.length === 0) {
    return "Secret scan passed: No committed secrets were detected.";
  }

  const lines = [
    `Secret scan failed: ${findings.length} potential secret(s) found.`,
    "",
    ...findings.map((finding) => formatFinding(finding)),
    "",
    "Remove the secret or add a documented allowlist entry in scripts/.secret-scan-allow.json.",
  ];

  return lines.join("\n");
}

function assertNoSecretsPrinted(output, findings) {
  for (const finding of findings) {
    if (finding.match && output.includes(finding.match)) {
      throw new Error(
        `Secret scan output leaked a matched value for ${finding.file}:${finding.line}`
      );
    }
  }
}

function runSecretScan(options = {}) {
  const backendRoot = options.backendRoot || process.cwd();
  const findings = scanBackend(backendRoot, options);
  const message = formatFindings(findings);

  if (findings.length > 0) {
    return {
      ok: false,
      exitCode: 1,
      findings,
      message,
    };
  }

  return {
    ok: true,
    exitCode: 0,
    findings,
    message,
  };
}

module.exports = {
  DEFAULT_EXAMPLE_FILES,
  DEFAULT_EXTENSIONS,
  DEFAULT_SCAN_ROOTS,
  KNOWN_SECRET_PATTERNS,
  MIN_PARTIAL_PREVIEW_LENGTH,
  MIN_PREVIEW_EDGE_LENGTH,
  MIN_UNIQUE_CHARACTERS,
  PLAIN_STRING_REGEX,
  MIN_HIGH_ENTROPY_LENGTH,
  MIN_HIGH_ENTROPY_SCORE,
  assertNoSecretsPrinted,
  assertAllowlistPatternsCompilable,
  collectHighEntropyMatches,
  collectQuotedStringMatches,
  collectRegexMatches,
  collectScanTargets,
  compilePattern,
  formatFinding,
  formatFindings,
  isAllowlisted,
  isAllowlistedIn,
  isHighEntropyToken,
  isIdentifierLikeString,
  isObviousPlaceholder,
  isStellarStrKeyLike,
  loadAllowlist,
  matchesAllowlistEntry,
  normalizeAllowlist,
  redactPreview,
  runSecretScan,
  scanBackend,
  scanFileContent,
  scanLine,
  scanTargets,
  shannonEntropy,
  shouldScanFile,
  unquoteString,
};
