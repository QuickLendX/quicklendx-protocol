"use strict";

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
const PLAIN_STRING_REGEX = /'([^'\\]|\\.)*'|"([^"\\]|\\.)*"/g;

const PREVIEW_MASK_CHARACTER = "*";
const PREVIEW_EDGE_LENGTH = 4;
const PREVIEW_ELLIPSIS = "...";
const PREVIEW_MASK_LENGTH = 8;
const PREVIEW_QUOTE = '"';

// Characters that may be emitted literally into a rendered preview. Detected
// secrets are base64 / base64url / hex / base32 shaped, so this covers real
// matches while keeping quotes, backslashes, control bytes, ANSI escapes,
// Unicode line separators and non-ASCII code units out of the log line. The
// brackets are what render the non-string "[redacted:<type>]" marker.
const PREVIEW_SAFE_CHARACTER = /^[A-Za-z0-9_\-+/=.:@[\]]$/;

// Readable short escapes for the control characters that show up often enough
// to be worth keeping legible; every other unsafe code unit uses \uXXXX.
const PREVIEW_SHORT_ESCAPES = new Map([
  ["\\", "\\\\"],
  ['"', '\\"'],
  ["\b", "\\b"],
  ["\f", "\\f"],
  ["\n", "\\n"],
  ["\r", "\\r"],
  ["\t", "\\t"],
]);

// The exact escape sequences escapePreviewText is allowed to emit. Used by
// isLogSafePreview to re-validate an already rendered preview.
const PREVIEW_ESCAPE_SEQUENCE = /^(?:[\\bfnrt"\\]|u[0-9a-f]{4})/;

// Upper bound on a rendered preview: two quotes, four escaped head characters,
// the ellipsis, four escaped tail characters. Each escaped character costs at
// most six code units ("\uXXXX"), so the bound is independent of input size.
const PREVIEW_MAX_LENGTH =
  PREVIEW_QUOTE.length +
  PREVIEW_EDGE_LENGTH * 6 +
  PREVIEW_ELLIPSIS.length +
  PREVIEW_EDGE_LENGTH * 6 +
  PREVIEW_QUOTE.length;

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

// Invariants for shannonEntropy:
//
//   E1 Total      - never throws, for any JavaScript value. A throw here
//                   propagates out of isHighEntropyToken -> scanLine ->
//                   scanTargets and aborts the whole scan, discarding the
//                   findings already collected for every other file.
//   E2 Typed      - only strings are measured. A primitive string is used
//                   directly; a String object is unwrapped through the
//                   internal slot, which cannot run user code and cannot
//                   throw for a well-formed wrapper. Every other type,
//                   including revoked proxies, returns 0. Nothing is coerced
//                   with String(value): coercion runs user code, can throw,
//                   and would manufacture an alphabet out of
//                   "[object Object]" and return a meaningless score.
//   E3 Normalised - probabilities sum to exactly 1. The symbol census is
//                   taken by code point (for...of) and divided by that same
//                   code-point count. Dividing by value.length (UTF-16 code
//                   units) weights every astral character at 1/2, so the
//                   probabilities sum to less than 1, entropy is
//                   under-reported, and a genuine secret can fall below
//                   MIN_HIGH_ENTROPY_SCORE and be missed without a trace.
//   E4 Bounded    - the result is always a finite, non-negative number within
//                   [0, log2(codePointCount)]. An all-uniform input yields
//                   +0 rather than -0, so toBe(0) holds.
//   E5 Pure       - no shared mutable state and no I/O, so repeated and
//                   interleaved calls on equal input return equal doubles.
//
// Compatibility: for any input drawn entirely from the Basic Multilingual
// Plane, codePointCount equals value.length, so this returns bit-identical
// doubles to the previous implementation for every call that did not throw.
// The observable changes are limited to inputs that previously threw
// TypeError (now 0) and astral input (now correctly normalised).

function shannonEntropy(value) {
  let text;

  if (typeof value === "string") {
    text = value;
  } else {
    text = unwrapStringObject(value);
  }

  if (text === null || text.length === 0) {
    return 0;
  }

  const counts = new Map();
  let codePoints = 0;
  for (const char of text) {
    counts.set(char, (counts.get(char) || 0) + 1);
    codePoints += 1;
  }

  // The division below is safe by construction: text.length > 0 was already
  // rejected above, and for...of yields at least one code point for any
  // non-empty string, so codePoints >= 1 and no count is ever 0.
  let entropy = 0;
  for (const count of counts.values()) {
    const probability = count / codePoints;
    entropy -= probability * Math.log2(probability);
  }

  // E4: collapse -0 and any non-finite residue to 0 so the result is always a
  // plain non-negative number that compares consistently against
  // MIN_HIGH_ENTROPY_SCORE.
  if (!Number.isFinite(entropy) || entropy <= 0) {
    return 0;
  }

  return entropy;
}

// Accepts a String object and returns its primitive value; returns null for
// every other input. String.prototype.valueOf is called directly on the
// intrinsic so a hostile `valueOf`/`Symbol.toPrimitive` override on a wrapper
// or proxy is never consulted, and the revoked-proxy TypeError is absorbed.
function unwrapStringObject(value) {
  if (typeof value !== "object" || value === null) {
    return null;
  }

  let unwrapped;
  try {
    unwrapped = String.prototype.valueOf.call(value);
  } catch {
    return null;
  }

  return typeof unwrapped === "string" ? unwrapped : null;
}

function isHexString(value) {
  return /^[0-9a-fA-F]+$/.test(value) || /^0x[0-9a-fA-F]+$/.test(value);
}

function isIdentifierLikeString(value) {
  if (typeof value !== "string") {
    return false;
  }
  return (
    /^[A-Za-z][A-Za-z0-9_$/-]*$/.test(value) &&
    /[a-z]/.test(value) &&
    /[A-Z]/.test(value) &&
    !/[0-9]/.test(value)
  );
}

function isObviousPlaceholder(value) {
  // Fail closed for non-strings so scan callers never crash on unexpected input.
  if (typeof value !== "string" && !(value instanceof String)) {
    return true;
  }

  const str = String(value);

  if (str.length === 0) {
    return true;
  }

  if (/^x+$/i.test(str) || /^y+$/i.test(str) || /^z+$/i.test(str)) {
    return true;
  }

  const uniqueChars = new Set(str);
  if (uniqueChars.size <= 2 && str.length >= MIN_HIGH_ENTROPY_LENGTH) {
    return true;
  }

  if (/^(your_|example_|placeholder|changeme|test[-_]?secret|development-only|fallback-secret)/i.test(str)) {
    return true;
  }

  if (isIdentifierLikeString(str)) {
    return true;
  }

  if (/^\/api\//.test(str) || /^https?:\/\//.test(str)) {
    return true;
  }

  return false;
}

function hasMixedCharacterClasses(value) {
  // Input validation: handle null, undefined, and non-string inputs deterministically
  if (value === null || value === undefined) {
    return false;
  }

  if (typeof value !== "string") {
    return false;
  }

  // Empty strings cannot have mixed character classes
  if (value.length === 0) {
    return false;
  }

  // Single character strings cannot have mixed character classes
  if (value.length === 1) {
    return false;
  }

  // Check for presence of each character class
  const classes = [
    /[a-z]/.test(value),
    /[A-Z]/.test(value),
    /[0-9]/.test(value),
    /[^A-Za-z0-9]/.test(value),
  ];

  // At least two different character classes must be present
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

// Redaction contract enforced by redactPreview. These are the invariants the
// focused tests in tests/secret-scan-redaction.test.ts pin down, and every one
// of them must hold for CI output to stay diagnosable without leaking a secret:
//
//   R1 Total          - never throws, for any JavaScript value, including
//                       revoked proxies and values whose coercion throws.
//                       A throw here would abort scanLine/scanTargets and
//                       discard findings already collected for other files.
//   R2 Empty          - nullish and "" render as '""'.
//   R3 Short values   - 1..PREVIEW_MASK_LENGTH characters render as a uniform
//                       mask of the same length: no position carries
//                       information about which character sat there.
//   R4 Edge only      - longer values render the first and last
//                       PREVIEW_EDGE_LENGTH characters joined by an ellipsis.
//                       The value.length - 2 * PREVIEW_EDGE_LENGTH middle
//                       characters are never emitted, so the preview can never
//                       contain the whole value.
//   R5 Log safe       - only PREVIEW_SAFE_CHARACTER, the quote and the mask
//                       character appear literally. Quotes, backslashes,
//                       control bytes, ANSI escapes, Unicode line separators
//                       and non-ASCII/surrogate code units are escaped, so one
//                       finding can never forge or break a log line.
//   R6 Bounded        - output length never exceeds PREVIEW_MAX_LENGTH and
//                       does not grow with the input.
//   R7 Pure           - no shared mutable state, so repeated and interleaved
//                       calls are deterministic.
//   R8 Typed refusal  - a non-string, non-nullish value is reported by type
//                       only ("[redacted:<type>]"). It is never coerced:
//                       String(value) can run user code, can throw, and can
//                       disclose far more than an edge.
//
// Compatibility: every string input that contains only PREVIEW_SAFE_CHARACTER
// renders exactly as before ('""', '"*****"', '"abcd...wxyz"'). The only
// observable change is for non-string inputs, which previously threw a
// TypeError (aborting the entire scan) or were silently mis-masked by array
// length.

function escapePreviewText(text) {
  let escaped = "";

  // Iterated by UTF-16 code unit on purpose: a slice taken at a fixed offset
  // can split a surrogate pair, and escaping each half keeps the output
  // losslessly decodable instead of emitting a lone surrogate.
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];

    const shortEscape = PREVIEW_SHORT_ESCAPES.get(character);
    if (shortEscape !== undefined) {
      escaped += shortEscape;
      continue;
    }

    if (PREVIEW_SAFE_CHARACTER.test(character)) {
      escaped += character;
      continue;
    }

    const codeUnit = text.charCodeAt(index);
    escaped += `\\u${codeUnit.toString(16).padStart(4, "0")}`;
  }

  return escaped;
}

function previewValueTypeTag(value) {
  if (value === null) {
    return "null";
  }

  // Array.isArray reads the internal slot without invoking user code, but it
  // still throws for a revoked proxy. The type tag is diagnostic only, so a
  // failed check degrades to the plain typeof instead of propagating.
  try {
    if (Array.isArray(value)) {
      return "array";
    }
  } catch (error) {
    return "object";
  }

  return typeof value;
}

function isLogSafePreview(text) {
  if (typeof text !== "string") {
    return false;
  }

  let index = 0;
  while (index < text.length) {
    const character = text[index];

    if (character === "\\") {
      const escape = text.slice(index + 1, index + 7).match(PREVIEW_ESCAPE_SEQUENCE);
      if (!escape) {
        return false;
      }
      index += 1 + escape[0].length;
      continue;
    }

    if (
      !PREVIEW_SAFE_CHARACTER.test(character) &&
      character !== PREVIEW_QUOTE &&
      character !== PREVIEW_MASK_CHARACTER
    ) {
      return false;
    }

    index += 1;
  }

  return true;
}

function redactPreview(value) {
  if (value === null || value === undefined) {
    return `${PREVIEW_QUOTE}${PREVIEW_QUOTE}`;
  }

  if (typeof value !== "string") {
    return `${PREVIEW_QUOTE}[redacted:${previewValueTypeTag(value)}]${PREVIEW_QUOTE}`;
  }

  if (value.length === 0) {
    return `${PREVIEW_QUOTE}${PREVIEW_QUOTE}`;
  }

  if (value.length <= PREVIEW_MASK_LENGTH) {
    const mask = PREVIEW_MASK_CHARACTER.repeat(value.length);
    return `${PREVIEW_QUOTE}${mask}${PREVIEW_QUOTE}`;
  }

  const head = escapePreviewText(value.slice(0, PREVIEW_EDGE_LENGTH));
  const tail = escapePreviewText(value.slice(-PREVIEW_EDGE_LENGTH));

  return `${PREVIEW_QUOTE}${head}${PREVIEW_ELLIPSIS}${tail}${PREVIEW_QUOTE}`;
}

function resetRegex(regex) {
  regex.lastIndex = 0;
}

function collectRegexMatches(line, patternDef) {
  const matches = [];
  resetRegex(patternDef.regex);

  let match = patternDef.regex.exec(line);
  while (match) {
    matches.push({
      type: patternDef.name,
      match: match[0],
      column: match.index + 1,
    });
    match = patternDef.regex.exec(line);
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
  resetRegex(PLAIN_STRING_REGEX);

  let match = PLAIN_STRING_REGEX.exec(line);
  while (match) {
    const literal = match[0];
    const value = unquoteString(literal);
    matches.push({
      literal,
      value,
      column: match.index + 1,
    });
    match = PLAIN_STRING_REGEX.exec(line);
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

function safeCompilePattern(pattern) {
  if (typeof pattern !== "string" || pattern.length === 0) {
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

  const hasFile = entry.file !== undefined;
  const hasLine = entry.line !== undefined;
  const hasMatch = entry.match !== undefined;
  const hasPattern = entry.pattern !== undefined;

  if (!hasFile && !hasLine && !hasMatch && !hasPattern) {
    return false;
  }

  if (hasFile && entry.file !== relativePath) {
    return false;
  }

  if (hasLine) {
    const expectedLine = Number(entry.line);
    const actualLine = Number(lineNumber);
    if (Number.isNaN(expectedLine) || Number.isNaN(actualLine)) {
      return false;
    }
    if (expectedLine !== actualLine) {
      return false;
    }
  }

  // Invariants (fail-closed, no throws, no logging of matchValue):
  // - matchValue must be a string for match/pattern selectors; anything
  //   else (null, undefined, number, object) cannot be allowlisted.
  // - entry.match must be a string; non-string selectors never match.
  // - entry.pattern must compile; invalid regex never matches and never throws.
  if ((hasMatch || hasPattern) && typeof matchValue !== "string") {
    return false;
  }

  if (hasMatch) {
    if (typeof entry.match !== "string") {
      return false;
    }
    if (!matchValue.includes(entry.match)) {
      return false;
    }
  }

  if (hasPattern) {
    const pattern = safeCompilePattern(entry.pattern);
    if (!pattern) {
      return false;
    }
    if (!pattern.test(matchValue)) {
      return false;
    }
  }

  return true;
}

// Invariants for isAllowlisted:
// - Pure and deterministic: same inputs always yield the same boolean, with
//   no shared mutable state, no I/O, and no logging of matchValue (caller is
//   responsible for redaction via redactPreview/formatFindings).
// - Fail-closed: null/undefined/malformed allowlists, non-string matchValue,
//   and invalid regex patterns all yield false instead of throwing, so retries,
//   partial failure, or concurrent execution cannot produce an unsafe allow.
// - Line comparison is numeric (Number() on both sides); NaN on either side
//   never matches. File comparison remains strict equality.
function isAllowlisted(relativePath, lineNumber, matchValue, allowlist) {
  if (typeof matchValue !== "string") {
    return false;
  }

  const normalized = normalizeAllowlist(allowlist);

  for (const entry of normalized.entries) {
    if (matchesAllowlistEntry(entry, relativePath, lineNumber, matchValue)) {
      return true;
    }
  }

  for (const entry of normalized.globalPatterns) {
    if (!entry || typeof entry.pattern !== "string" || entry.pattern.length === 0) {
      continue;
    }

    const pattern = safeCompilePattern(entry.pattern);
    if (!pattern) {
      continue;
    }
    if (pattern.test(matchValue)) {
      return true;
    }
  }

  return false;
}

function overlapsMatch(left, right) {
  return left.match === right.match;
}

function scanLine(line, lineNumber, relativePath, allowlist) {
  const findings = [];
  const seen = new Set();

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

    if (isAllowlisted(relativePath, lineNumber, candidate.match, allowlist)) {
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

  for (const entry of fs.readdirSync(absoluteDir, { withFileTypes: true })) {
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
    const content = fs.readFileSync(target.absolutePath, "utf8");
    findings.push(...scanFileContent(content, target.relativePath, allowlist));
  }

  return findings;
}

function scanBackend(backendRoot, options = {}) {
  const allowlist = options.allowlist || loadAllowlist(options.allowlistPath, backendRoot);
  const targets = collectScanTargets(backendRoot, options);
  return scanTargets(targets, allowlist);
}

function loadAllowlist(allowlistPath, backendRoot = process.cwd()) {
  const resolvedPath =
    allowlistPath || path.join(backendRoot, "scripts", ".secret-scan-allow.json");

  if (!fs.existsSync(resolvedPath)) {
    return normalizeAllowlist(null);
  }

  const raw = fs.readFileSync(resolvedPath, "utf8");
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    throw new Error(`Failed to parse secret scan allowlist: ${error.message}`);
  }

  return normalizeAllowlist(parsed);
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
  MIN_UNIQUE_CHARACTERS,
  PLAIN_STRING_REGEX,
  MIN_HIGH_ENTROPY_LENGTH,
  MIN_HIGH_ENTROPY_SCORE,
  PREVIEW_EDGE_LENGTH,
  PREVIEW_ELLIPSIS,
  PREVIEW_MASK_CHARACTER,
  PREVIEW_MASK_LENGTH,
  PREVIEW_MAX_LENGTH,
  PREVIEW_QUOTE,
  assertNoSecretsPrinted,
  collectHighEntropyMatches,
  collectQuotedStringMatches,
  collectRegexMatches,
  collectScanTargets,
  escapePreviewText,
  formatFinding,
  formatFindings,
  hasMixedCharacterClasses,
  isAllowlisted,
  isHighEntropyToken,
  isIdentifierLikeString,
  isLogSafePreview,
  isObviousPlaceholder,
  isStellarStrKeyLike,
  loadAllowlist,
  matchesAllowlistEntry,
  normalizeAllowlist,
  previewValueTypeTag,
  redactPreview,
  runSecretScan,
  safeCompilePattern,
  scanBackend,
  scanFileContent,
  scanLine,
  scanTargets,
  shannonEntropy,
  shouldScanFile,
  unquoteString,
};
