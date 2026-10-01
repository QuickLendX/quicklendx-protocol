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

// Anchored hex shapes. Both are module-level literals deliberately created
// without the g or y flag: RegExp.prototype.test only reads lastIndex for those
// two flags, so the patterns are position-free and can be shared by every call
// without leaking state between a scan line, a retry, and an interleaved call.
const HEX_DIGITS_PATTERN = /^[0-9a-fA-F]+$/;
const HEX_PREFIXED_PATTERN = /^0x[0-9a-fA-F]+$/;

// Failure-boundary contract for isHexString. These invariants are pinned down
// by tests/secret-scan-hex.test.ts, and every one of them is load-bearing
// because isHexString sits on the *suppressor* side of secret classification:
// isHighEntropyToken drops a candidate whenever isHexString reports true, so a
// wrong true is a silently dropped finding, not a noisy one.
//
//   H1 Total          - never throws, for any JavaScript value, including
//                       symbols, revoked proxies, and objects whose toString
//                       or Symbol.toPrimitive throws. RegExp.prototype.test
//                       coerces its argument with ToString, which runs
//                       user-visible code; an uncaught throw escapes through
//                       isHighEntropyToken -> collectHighEntropyMatches ->
//                       scanLine -> scanTargets, aborts the scan, and discards
//                       the findings already collected for other files. The one
//                       current caller feeds it a regex-derived primitive
//                       string, so this is defence in depth for the exported
//                       helper and for any caller added later.
//   H2 Type-exact     - only a string primitive can be hex. Numbers, bigints,
//                       and String wrappers coerce to text that can be entirely
//                       hex ("255", "0", "deadbeef"), and accepting that
//                       coercion would let a non-string suppress a finding.
//                       Refusing non-strings is the fail-closed direction: it
//                       can only add findings, never remove one.
//   H3 Full value     - the shape is matched end to end, so a padded, embedded,
//                       or line-terminated value (" deadbeef ", "0xdeadbeefg",
//                       "deadbeef\n") is not hex.
//   H4 Minimal length - at least one hex digit is required, so neither the empty
//                       string nor a bare "0x" prefix is hex.
//   H5 Lowercase prefix only - exactly one optional "0x". "0X1f" is not hex:
//                       widening the accepted set would suppress more findings,
//                       i.e. weaken detection, so the conservative reading is
//                       kept rather than "fixed".
//   H6 Linear, pure   - the patterns are a single anchored character class with
//                       no ambiguous quantifier, so matching is O(length) with
//                       no catastrophic backtracking. No shared mutable state,
//                       no I/O, and the value is never logged, so repeated
//                       calls, retries, and interleaved execution are
//                       deterministic and never disclose a candidate.
//
// Compatibility: for every string input the result is exactly what the
// previous implementation returned, and the sole caller (isHighEntropyToken)
// cannot observe the non-string change, because a non-string can never reach a
// `true` from isHighEntropyToken anyway: isObviousPlaceholder fails closed to
// true and hasMixedCharacterClasses returns false for non-strings. The only
// observable change is for non-string inputs, which previously either threw a
// TypeError (aborting the entire scan) or were misclassified as hex.
function isHexString(value) {
  // H1/H2: refuse everything that is not a string primitive before a regex can
  // coerce it. typeof reads an internal slot, so a hostile value cannot throw
  // and cannot run a trap here.
  if (typeof value !== "string") {
    return false;
  }

  return HEX_DIGITS_PATTERN.test(value) || HEX_PREFIXED_PATTERN.test(value);
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

const STELLAR_STRKEY_REGEX = /^[GX][A-Z2-7]{55}$/;

/**
 * Validates whether a value is a Stellar StrKey-like public identifier (e.g. account G... or hash signer X...).
 *
 * Invariants:
 * - Type Safety: Safely handles non-string inputs (null, undefined, Symbol, BigInt, objects) returning false without throwing.
 * - Length Boundary: Exactly 56 ASCII characters; fast-fails for any other length.
 * - Alphabet Boundary: RFC 4648 Base32 alphabet ([A-Z2-7]) prefixed strictly by public key identifiers 'G' or 'X'.
 * - Security Rejection: Never returns true for Stellar secret seeds ('S...') or other sensitive key types.
 * - Stateless & Deterministic: Pure function with no shared mutable regex state; safe across concurrent, retry, and re-entrant calls.
 *
 * @param {unknown} value The candidate value to evaluate.
 * @returns {boolean} True if the value matches the Stellar StrKey public format, false otherwise.
 */
function isStellarStrKeyLike(value) {
  if (typeof value !== "string") {
    return false;
  }

  if (value.length !== 56) {
    return false;
  }

  return STELLAR_STRKEY_REGEX.test(value);
}

function isHighEntropyToken(value) {
  if (typeof value !== "string" || value.length < MIN_HIGH_ENTROPY_LENGTH) {
    return false;
  }

  if (!/^[A-Za-z0-9+/=_-]+$/.test(value)) {
    return false;
  }

  // A hex run is a commit SHA, a colour or a byte buffer, not a credential, so
  // it is suppressed here. This is the reason isHexString is held to H1/H2 in
  // its contract above: a spurious true from this branch silently drops the
  // finding instead of raising a false alarm.
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

// Deterministic failure-boundary handling for formatFindings.
//
// formatFindings is the last stage before findings are rendered into CI logs,
// so it must never throw and must never emit an unsafe preview. The helpers
// below normalize the finding collection and each individual finding so that
// malformed, duplicate, or boundary-case inputs degrade to a stable, reviewable
// shape instead of aborting the scan or leaking data.
const FINDING_SEVERITY_ORDER = new Map([
  ["critical", 0],
  ["high", 1],
  ["medium", 2],
  ["low", 3],
  ["info", 4],
]);

function normalizeFindingSeverity(severity) {
  if (typeof severity !== "string") {
    return "unknown";
  }

  const normalized = severity.trim().toLowerCase();
  return normalized.length > 0 ? normalized : "unknown";
}

function normalizeFindingLocation(location) {
  if (typeof location !== "string") {
    return "";
  }

  return location;
}

function normalizeFindingType(type) {
  if (typeof type !== "string" || type.length === 0) {
    return "unknown";
  }

  return type;
}

function normalizeFinding(finding) {
  if (finding === null || typeof finding !== "object") {
    return null;
  }

  const type = normalizeFindingType(finding.type);
  const severity = normalizeFindingSeverity(finding.severity);
  const location = normalizeFindingLocation(finding.location);
  const preview = redactPreview(finding.match);

  return { type, severity, location, preview };
}

function findingDedupeKey(finding) {
  return `${finding.type}\u0000${finding.severity}\u0000${finding.location}\u0000${finding.preview}`;
}

function compareFindings(left, right) {
  const leftSeverity = FINDING_SEVERITY_ORDER.has(left.severity)
    ? FINDING_SEVERITY_ORDER.get(left.severity)
    : Number.MAX_SAFE_INTEGER;
  const rightSeverity = FINDING_SEVERITY_ORDER.has(right.severity)
    ? FINDING_SEVERITY_ORDER.get(right.severity)
    : Number.MAX_SAFE_INTEGER;

  if (leftSeverity !== rightSeverity) {
    return leftSeverity - rightSeverity;
  }

  if (left.type !== right.type) {
    return left.type < right.type ? -1 : 1;
  }

  if (left.location !== right.location) {
    return left.location < right.location ? -1 : 1;
  }

  if (left.preview !== right.preview) {
    return left.preview < right.preview ? -1 : 1;
  }

  return 0;
}

function formatFindings(findings) {
  if (!Array.isArray(findings)) {
    return [];
  }

  const normalized = [];
  const seen = new Set();

  for (const finding of findings) {
    const entry = normalizeFinding(finding);
    if (entry === null) {
      continue;
    }

    const key = findingDedupeKey(entry);
    if (seen.has(key)) {
      continue;
    }

    seen.add(key);
    normalized.push(entry);
  }

  normalized.sort(compareFindings);

  return normalized.map((entry) => {
    const location = entry.location.length > 0 ? ` ${entry.location}` : "";
    return `[${entry.severity}] ${entry.type}${location}: ${entry.preview}`;
  });
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
  // Scan callers process file lines, but keeping this helper total makes the
  // failure boundary deterministic when a malformed caller supplies a
  // non-string value. Returning no matches is safer than coercing arbitrary
  // objects (which may execute user code or disclose sensitive data).
  if (typeof line !== "string") {
    return [];
  }

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
  if (typeof line !== "string") {
    return [];
  }

  const matches = [];

  try {
    for (const quoted of module.exports.collectQuotedStringMatches(line)) {
      try {
        if (!quoted || typeof quoted.value !== "string") {
          continue;
        }

        if (!module.exports.isHighEntropyToken(quoted.value)) {
          continue;
        }

        matches.push({
          type: "high-entropy",
          match: quoted.value,
          column: typeof quoted.column === "number" ? quoted.column : 0,
        });
      } catch (innerError) {
        // Deterministic partial failure recovery: skip this match but continue processing.
        continue;
      }
    }
  } catch (error) {
    // Deterministic failure boundary: return accumulated matches on unexpected iterator/regex error.
    return matches;
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
  try {
    if (!left || typeof left !== "object" || !right || typeof right !== "object") {
      return false;
    }

    const leftMatch = typeof left.match === "string" ? left.match : "";
    const rightMatch = typeof right.match === "string" ? right.match : "";

    if (!leftMatch || !rightMatch) {
      return false;
    }

    const leftStart = typeof left.column === "number" ? left.column : -1;
    const rightStart = typeof right.column === "number" ? right.column : -1;

    if (leftStart === -1 || rightStart === -1) {
      return leftMatch === rightMatch;
    }

    const leftEnd = leftStart + leftMatch.length;
    const rightEnd = rightStart + rightMatch.length;

    return leftStart < rightEnd && rightStart < leftEnd;
  } catch (error) {
    return false;
  }
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

// Invariants for scanTargets:
//
//   T1 Total        - never throws because of a per-target problem. Every
//                      target in the list is attempted, so one unreadable file
//                      can no longer abort the loop and discard the findings
//                      already collected for the files ahead of it.
//   T2 Fail closed  - a target that could not be scanned is never silently
//                      dropped. Every failure is handed to
//                      options.onTargetError, and when no reporter is supplied
//                      the aggregated failures are rethrown once the loop has
//                      finished. runSecretScan always supplies a reporter, so a
//                      partial scan exits non-zero instead of reporting
//                      "clean". The scan is never weakened by this isolation.
//   T3 Deduped      - an absolutePath is scanned at most once even when it
//                      appears repeatedly in targets. Overlapping scanRoots
//                      used to emit the same finding once per covering root.
//   T4 Typed        - an entry that is not an object, or that has no string
//                      absolutePath, is recorded as a structural failure
//                      instead of crashing the loop.
//   T5 Redacted     - a failure record carries only the target relativePath, a
//                      filesystem errno code, and a bounded message. File
//                      content and matched values never enter a record, so
//                      diagnostics cannot leak the string that was scanned.
//   T6 Pure         - no shared mutable state; the same list always yields the
//                      same findings and the same failures in the same order.
//
// Argument-shape violations (a non-array `targets`) still throw, as they did
// before, but with an explicit message naming the received type. That is a
// caller bug rather than a per-target condition, and silently scanning nothing
// would be the unsafe answer.

const TARGET_FAILURE_MAX_MESSAGE_LENGTH = 200;
const TARGET_FAILURE_UNKNOWN_PATH = "<unknown>";
const TARGET_FAILURE_UNKNOWN_REASON = "unknown reason";

function targetFailureKindTag(value) {
  if (value === null) {
    return "null";
  }
  if (Array.isArray(value)) {
    return "array";
  }
  return typeof value;
}

// Builds the redacted, bounded failure record described by T5. error.code is
// a filesystem errno such as ENOENT or EACCES and is the useful triage signal;
// the message is truncated because a path or an OS message can be arbitrarily
// long. A hostile error is reduced to a constant rather than stringified,
// because String(error) would run user code and can throw.
function describeTargetFailure(error) {
  let code = TARGET_FAILURE_UNKNOWN_REASON;
  let message = "";

  if (typeof error === "object" && error !== null) {
    try {
      if (typeof error.code === "string") {
        code = error.code;
      }
      if (typeof error.message === "string") {
        message = error.message;
      }
    } catch {
      // A throwing getter on a hostile error object degrades to the defaults.
      code = TARGET_FAILURE_UNKNOWN_REASON;
      message = "";
    }
  }

  if (message.length === 0) {
    message = code;
  }

  return {
    code,
    message: message.slice(0, TARGET_FAILURE_MAX_MESSAGE_LENGTH),
  };
}

// Reads one property without letting a hostile getter escape. A revoked proxy
// or a booby-trapped accessor yields undefined instead of propagating, which is
// what keeps T1 total for target descriptors built by callers.
function safeReadProperty(target, property) {
  try {
    return target[property];
  } catch {
    return undefined;
  }
}

// Returns the { absolutePath, relativePath } pair for a usable target, or null
// when the entry is structurally invalid (T4). relativePath is optional: when
// it is absent or not a string the absolutePath is reported instead, so a
// finding is still attributable to a file rather than rendered as "undefined".
function resolveScanTarget(target) {
  if (typeof target !== "object" || target === null) {
    return null;
  }

  const absolutePath = safeReadProperty(target, "absolutePath");
  if (typeof absolutePath !== "string" || absolutePath.length === 0) {
    return null;
  }

  const relativePath = safeReadProperty(target, "relativePath");

  return {
    absolutePath,
    relativePath:
      typeof relativePath === "string" && relativePath.length > 0
        ? relativePath
        : absolutePath,
  };
}

function recordTargetFailure(failures, report, target, reason, code, message) {
  const failure = {
    target: typeof target === "string" && target.length > 0 ? target : TARGET_FAILURE_UNKNOWN_PATH,
    reason,
    code,
    message: message.slice(0, TARGET_FAILURE_MAX_MESSAGE_LENGTH),
  };

  failures.push(failure);

  if (report !== null) {
    report(failure);
  }

  return failure;
}

function scanTargets(targets, allowlist, options = {}) {
  if (!Array.isArray(targets)) {
    throw new TypeError(
      `scanTargets requires an array of targets, received ${targetFailureKindTag(targets)}`
    );
  }

  const report = typeof options.onTargetError === "function" ? options.onTargetError : null;
  const findings = [];
  const failures = [];
  const scanned = new Set();

  for (const target of targets) {
    const resolved = resolveScanTarget(target);

    if (resolved === null) {
      // T4: a structurally invalid entry is a recorded failure, never a crash.
      // The relativePath read is guarded too: a hostile getter must not turn a
      // structural failure into an unhandled throw.
      recordTargetFailure(
        failures,
        report,
        typeof target === "object" && target !== null
          ? safeReadProperty(target, "relativePath")
          : undefined,
        "invalid-target",
        "EINVAL",
        `target is not an object with a string absolutePath (received ${targetFailureKindTag(target)})`
      );
      continue;
    }

    // T3: scan each file once no matter how many roots or duplicates cover it.
    if (scanned.has(resolved.absolutePath)) {
      continue;
    }
    scanned.add(resolved.absolutePath);

    let content;
    try {
      content = fs.readFileSync(resolved.absolutePath, "utf8");
    } catch (error) {
      // T1/T2: isolate the failure so the remaining targets are still scanned
      // and the findings collected so far survive.
      const described = describeTargetFailure(error);
      recordTargetFailure(
        failures,
        report,
        resolved.relativePath,
        "unreadable-target",
        described.code,
        described.message
      );
      continue;
    }

    findings.push(...scanFileContent(content, resolved.relativePath, allowlist));
  }

  if (failures.length > 0 && report === null) {
    const summary = failures
      .map((failure) => `${failure.target} (${failure.code})`)
      .join(", ");
    throw new Error(
      `Secret scan could not read ${failures.length} of ${targets.length} target(s): ${summary}`
    );
  }

  return findings;
}

function scanBackend(backendRoot, options = {}) {
  const allowlist = options.allowlist || loadAllowlist(options.allowlistPath, backendRoot);
  const targets = collectScanTargets(backendRoot, options);
  return scanTargets(targets, allowlist, options);
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

// Renders one failure record. Only the target path, the reason, the errno code
// and the bounded message are emitted, per T5: no file content and no matched
// value can reach the log line.
function formatTargetFailure(failure) {
  return `  ${failure.target} [${failure.reason}] ${failure.code}: ${failure.message}`;
}

function formatFindings(findings, failures = []) {
  const lines = [];

  if (findings.length > 0) {
    lines.push(
      `Secret scan failed: ${findings.length} potential secret(s) found.`,
      "",
      ...findings.map((finding) => formatFinding(finding)),
      "",
      "Remove the secret or add a documented allowlist entry in scripts/.secret-scan-allow.json."
    );
  }

  if (failures.length > 0) {
    if (lines.length > 0) {
      lines.push("");
    }

    lines.push(
      `Secret scan failed: ${failures.length} target(s) could not be scanned.`,
      ...failures.map((failure) => formatTargetFailure(failure)),
      "An unreadable target is never treated as clean. Fix the path or permission and re-run the scan."
    );
  }

  if (lines.length === 0) {
    return "Secret scan passed: No committed secrets were detected.";
  }

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

  // T2: the reporter is what lets scanTargets isolate a per-target failure and
  // keep going. The recorded failures are what make the run fail closed, so a
  // partial scan never reports "clean".
  const failures = [];
  const findings = scanBackend(backendRoot, {
    ...options,
    onTargetError: (failure) => {
      failures.push(failure);
    },
  });

  const message = formatFindings(findings, failures);

  if (findings.length > 0 || failures.length > 0) {
    return {
      ok: false,
      exitCode: 1,
      findings,
      failures,
      message,
    };
  }

  return {
    ok: true,
    exitCode: 0,
    findings,
    failures,
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
  formatTargetFailure,
  hasMixedCharacterClasses,
  isAllowlisted,
  isHighEntropyToken,
  isHexString,
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
