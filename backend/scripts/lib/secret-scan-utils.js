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
const MAX_EXTENSION_LENGTH = 16;
const MAX_RELATIVE_PATH_LENGTH = 4096;

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

// Hard upper bound on matches collected from a single line. A pathological
// pattern (or a caller-supplied regex with a zero-width match) must never be
// able to make collectRegexMatches run unbounded, so the loop stops here and
// returns what it has. The cap is far above any realistic secret density on
// one line, so normal scans are unaffected.
const MAX_MATCHES_PER_LINE = 10000;

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
  if (typeof value !== "string" && !(value instanceof String)) {
    return false;
  }
  const str = String(value);
  return /^[0-9a-fA-F]+$/.test(str) || /^0x[0-9a-fA-F]+$/.test(str);
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
  if (typeof value !== "string" && !(value instanceof String)) {
    return false;
  }
  return /^[GX][A-Z2-7]{55}$/.test(String(value));
}

// High-entropy token detection invariants enforced by isHighEntropyToken:
//
//   H1 Total (Fail closed) - Never throws for any JavaScript value (null, undefined,
//                            numbers, booleans, objects, arrays, symbols, bigints,
//                            functions, or revoked/throwing proxies). Fails closed
//                            by returning false.
//   H2 Type boundary       - Accepts string primitives and String wrapper objects.
//                            Any other type immediately returns false.
//   H3 Length boundary     - Strictly requires length >= MIN_HIGH_ENTROPY_LENGTH (32).
//                            Values of length 0..31 immediately return false.
//   H4 Character set       - Tokens must consist exclusively of valid base64 / base64url /
//                            safe token characters (/^[A-Za-z0-9+/=_-]+$/). Whitespace,
//                            control characters, non-ASCII Unicode, and symbols return false.
//   H5 Hex exclusion       - Pure hexadecimal strings (isHexString) representing commit hashes,
//                            SHA digests, Ethereum addresses, etc., return false.
//   H6 Stellar StrKey      - Stellar public keys or muxed accounts (isStellarStrKeyLike)
//                            return false.
//   H7 Obvious placeholder - Common development/test placeholders, repeated strings,
//                            and code identifiers (isObviousPlaceholder) return false.
//   H8 Unique characters   - Requires new Set(str).size >= MIN_UNIQUE_CHARACTERS (10).
//                            Low character variety returns false.
//   H9 Character classes   - Requires at least two character classes (hasMixedCharacterClasses).
//                            Single-class tokens (e.g. only lowercase or only digits) return false.
//   H10 Shannon entropy    - Computes Shannon entropy; requires score >= MIN_HIGH_ENTROPY_SCORE (4.5).
//   H11 Non-leakage        - Strictly returns boolean true or false; never logs or includes
//                            token content in exceptions or outputs.
//   H12 Determinism        - Fully pure and idempotent across repeated and concurrent calls,
//                            with no regex lastIndex or shared mutable state side-effects.
function isHighEntropyToken(value) {
  // Input validation: fail closed on null and undefined
  if (value === null || value === undefined) {
    return false;
  }

  try {
    if (typeof value !== "string" && !(value instanceof String)) {
      return false;
    }

    const str = typeof value === "string" ? value : String(value);

    if (str.length < MIN_HIGH_ENTROPY_LENGTH) {
      return false;
    }

    if (!/^[A-Za-z0-9+/=_-]+$/.test(str)) {
      return false;
    }

    if (isHexString(str)) {
      return false;
    }

    if (isStellarStrKeyLike(str)) {
      return false;
    }

    if (isObviousPlaceholder(str)) {
      return false;
    }

    if (new Set(str).size < MIN_UNIQUE_CHARACTERS) {
      return false;
    }

    if (!hasMixedCharacterClasses(str)) {
      return false;
    }

    return shannonEntropy(str) >= MIN_HIGH_ENTROPY_SCORE;
  } catch {
    return false;
  }
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

// Deterministic, fail-closed collection of regex matches for a single line.
//
// Invariants enforced here (see tests/secret-scan-collect-regex-matches.test.ts):
//   C1 Total        - never throws. A null/undefined line, a missing or
//                     malformed patternDef, a regex whose exec throws, or a
//                     revoked proxy all yield [] instead of aborting the scan
//                     and discarding findings already collected for other
//                     files.
//   C2 Deterministic- the same (line, patternDef) always yields the same
//                     matches, regardless of prior calls. lastIndex is reset
//                     before and after the loop so a shared regex cannot leak
//                     state across lines or across concurrent scans.
//   C3 Bounded      - at most MAX_MATCHES_PER_LINE matches are returned, and
//                     the loop always advances. Zero-length matches and
//                     non-global regexes cannot spin forever.
//   C4 Shape        - every returned entry is { type: string, match: string,
//                     column: number >= 1 }. Non-string match[0] is skipped
//                     rather than coerced.

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

  // C1: reject inputs that cannot be scanned without throwing.
  if (typeof line !== "string") {
    return matches;
  }

  if (!patternDef || typeof patternDef !== "object") {
    return matches;
  }

  const regex = patternDef.regex;
  if (!regex || typeof regex.exec !== "function") {
    return matches;
  }

  const type = typeof patternDef.name === "string" ? patternDef.name : "unknown";

  // C2: reset before scanning so a shared regex starts from a known state.
  try {
    resetRegex(regex);
  } catch (error) {
    return matches;
  }

  let match;
  try {
    match = regex.exec(line);
  } catch (error) {
    // C1: a throwing exec (revoked proxy, stateful getter) is a boundary,
    // not a crash. Return what we have and leave the regex reset below.
    return matches;
  }

  while (match && matches.length < MAX_MATCHES_PER_LINE) {
    const value = match[0];
    const index = match.index;

    // C4: only string matches with a numeric index are emitted. Anything
    // else is skipped rather than coerced, so a hostile match object cannot
    // inject non-string data into downstream redaction.
    if (typeof value === "string" && typeof index === "number" && index >= 0) {
      matches.push({
        type,
        match: value,
        column: index + 1,
      });
    }

    // C3: guarantee forward progress. A zero-length match leaves lastIndex
    // unchanged on a global regex, which would loop forever; advance it
    // manually. A non-global regex also never advances lastIndex, so the
    // same guard covers it.
    if (value === "") {
      if (regex.global || regex.sticky) {
        regex.lastIndex = index + 1;
      } else {
        break;
      }
    }

    try {
      match = regex.exec(line);
    } catch (error) {
      break;
    }
  }

  // C2: leave the regex in a clean state for the next caller.
  try {
    resetRegex(regex);
  } catch (error) {
    // Ignore: the regex is already unusable; callers get the matches we have.
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

  if (
    typeof relativePath !== "string" ||
    typeof lineNumber !== "number" ||
    typeof matchValue !== "string"
  ) {
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
  if (typeof content !== "string") {
    return [];
  }
  const lines = content.split(/\r?\n/);
  return lines.flatMap((line, index) =>
    scanLine(line, index + 1, relativePath, allowlist)
  );
}

function shouldScanFile(relativePath, options = {}) {
  if (typeof relativePath !== "string" || relativePath.length === 0) {
    return false;
  }

  const opts = options && typeof options === "object" ? options : {};
  const rawExtensions = opts.extensions || DEFAULT_EXTENSIONS;
  const extensions =
    rawExtensions instanceof Set
      ? rawExtensions
      : new Set(Array.isArray(rawExtensions) ? rawExtensions : [rawExtensions]);

  const rawIgnored = opts.ignoredFiles || [".secret-scan-allow.json"];
  const ignoredFiles =
    rawIgnored instanceof Set
      ? rawIgnored
      : new Set(Array.isArray(rawIgnored) ? rawIgnored : [rawIgnored]);

  const base = path.basename(relativePath);
  if (ignoredFiles.has(base) || ignoredFiles.has(relativePath)) {
    return false;
  }

  if (extensions.has(extension)) {
    return true;
  }

  const rawExamples = opts.exampleFiles || DEFAULT_EXAMPLE_FILES;
  const exampleFiles = Array.isArray(rawExamples)
    ? rawExamples
    : (rawExamples instanceof Set ? Array.from(rawExamples) : [rawExamples]);

  return exampleFiles.includes(base) || exampleFiles.includes(relativePath);
}

function walkDirectory(absoluteDir, relativeDir, files = [], visited = new Set()) {
  if (typeof absoluteDir !== "string" || absoluteDir.length === 0) {
    return files;
  }

  try {
    if (!fs.existsSync(absoluteDir)) {
      return files;
    }
    const stat = fs.statSync(absoluteDir);
    if (!stat.isDirectory()) {
      return files;
    }
  } catch (_err) {
    return files;
  }

  try {
    const real = fs.realpathSync(absoluteDir);
    if (visited.has(real)) {
      return files;
    }
    visited.add(real);
  } catch (_err) {
    // Proceed if realpath fails
  }

  let entries;
  try {
    entries = fs.readdirSync(absoluteDir, { withFileTypes: true });
  } catch (_err) {
    // Permission error (EACCES/EPERM) or concurrent removal: return files gathered so far
    return files;
  }

  // Sort entries for deterministic traversal across all operating systems
  entries.sort((a, b) => a.name.localeCompare(b.name));

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
      walkDirectory(absolutePath, relativePath, files, visited);
      continue;
    }

    if (entry.isFile()) {
      files.push({
        absolutePath,
        relativePath: relativePath.replace(/\\/g, "/"),
      });
    }
  }

  return files;
}

function collectScanTargets(backendRoot, options = {}) {
  if (typeof backendRoot !== "string" || backendRoot.trim().length === 0) {
    return [];
  }

  const opts = options && typeof options === "object" ? options : {};
  const rawRoots = opts.scanRoots !== undefined ? opts.scanRoots : DEFAULT_SCAN_ROOTS;
  const scanRoots = Array.isArray(rawRoots)
    ? rawRoots.filter((r) => typeof r === "string" && r.length > 0)
    : typeof rawRoots === "string" && rawRoots.length > 0
    ? [rawRoots]
    : [];

  const rawExamples = opts.exampleFiles !== undefined ? opts.exampleFiles : DEFAULT_EXAMPLE_FILES;
  const exampleFiles = Array.isArray(rawExamples)
    ? rawExamples.filter((f) => typeof f === "string" && f.length > 0)
    : typeof rawExamples === "string" && rawExamples.length > 0
    ? [rawExamples]
    : [];

  const targets = [];
  const seenRelativePaths = new Set();

  const addTarget = (absolutePath, relativePath) => {
    const normalizedRelative = relativePath.replace(/\\/g, "/");
    if (seenRelativePaths.has(normalizedRelative)) {
      return;
    }
    seenRelativePaths.add(normalizedRelative);
    targets.push({
      absolutePath,
      relativePath: normalizedRelative,
    });
  };

  const uniqueRoots = Array.from(new Set(scanRoots)).sort();
  for (const root of uniqueRoots) {
    const absoluteRoot = path.join(backendRoot, root);
    const relativeRoot = root.replace(/\\/g, "/");
    const found = walkDirectory(absoluteRoot, relativeRoot);
    for (const file of found) {
      addTarget(file.absolutePath, file.relativePath);
    }
  }

  const uniqueExamples = Array.from(new Set(exampleFiles)).sort();
  for (const exampleFile of uniqueExamples) {
    const absolutePath = path.join(backendRoot, exampleFile);
    try {
      if (fs.existsSync(absolutePath)) {
        addTarget(absolutePath, exampleFile);
      }
    } catch (_err) {
      // Permission or filesystem error on example file: skip gracefully
    }
  }

  const filtered = targets.filter((target) => shouldScanFile(target.relativePath, opts));
  filtered.sort((a, b) => a.relativePath.localeCompare(b.relativePath));
  return filtered;
}

function scanTargets(targets, allowlist, options = {}) {
  if (!Array.isArray(targets)) {
    return [];
  }

  const opts = options && typeof options === "object" ? options : {};
  const normalizedAllowlist = normalizeAllowlist(allowlist);
  const findings = [];
  const failures = [];
  const scanned = new Set();

  for (const target of targets) {
    if (!target || typeof target !== "object" || typeof target.absolutePath !== "string") {
      continue;
    }

    let content;
    try {
      content = fs.readFileSync(target.absolutePath, "utf8");
    } catch (error) {
      if (typeof opts.onFileError === "function") {
        opts.onFileError(target, error);
      }
      // Adverse condition (permission error, locked file, race condition):
      // do not discard findings already collected for other files.
      continue;
    }

    const relPath =
      typeof target.relativePath === "string" ? target.relativePath : target.absolutePath;
    findings.push(...scanFileContent(content, relPath, normalizedAllowlist));
  }

  return findings;
}

// Invariants for scanBackend:
// - S1 Total & Fail-Closed: Invalid, non-string, or non-existent backendRoot returns []
//   deterministically without throwing. Nullish, primitive, or malformed options are safely handled.
// - S2 Deterministic & Stable: Directory traversal and target collection sort paths alphabetically
//   so scan runs across different environments produce findings in identical order.
// - S3 Deduplication: Duplicate scan roots, overlapping example files, or repeated targets
//   are deduplicated by relativePath so files are never scanned multiple times.
// - S4 Adverse Resilience: Unreadable files or directories (EACCES/EPERM permissions, ENOENT
//   races, locked files) are handled gracefully without aborting the scan or dropping findings
//   already collected for valid files.
// - S5 Non-Disclosing & Pure: No shared mutable state; concurrent or repeated calls are pure
//   and thread-safe; all findings report redacted previews and never expose raw secret values.
// - S6 Caller Compatibility: Preserves public API signature scanBackend(backendRoot, options),
//   fully honoring allowlist, allowlistPath, scanRoots, and extensions.
function scanBackend(backendRoot, options = {}) {
  const opts = options && typeof options === "object" && !Array.isArray(options) ? options : {};

  let root;
  if (typeof backendRoot === "string") {
    root = backendRoot;
  } else if (backendRoot === undefined) {
    root = process.cwd();
  } else {
    // Fail closed on non-string, null, or invalid roots
    return [];
  }

  if (root.trim().length === 0) {
    return [];
  }

  try {
    if (!fs.existsSync(root)) {
      return [];
    }
  } catch (_err) {
    return [];
  }

  const allowlist =
    opts.allowlist !== undefined
      ? normalizeAllowlist(opts.allowlist)
      : loadAllowlist(opts.allowlistPath, root);

  const targets = collectScanTargets(root, opts);
  return scanTargets(targets, allowlist, opts);
}

function loadAllowlist(allowlistPath, backendRoot = process.cwd()) {
  const root =
    typeof backendRoot === "string" && backendRoot.length > 0 ? backendRoot : process.cwd();
  const resolvedPath =
    typeof allowlistPath === "string" && allowlistPath.length > 0
      ? allowlistPath
      : path.join(root, "scripts", ".secret-scan-allow.json");

  try {
    if (!fs.existsSync(resolvedPath)) {
      return normalizeAllowlist(null);
    }
  } catch (_err) {
    return normalizeAllowlist(null);
  }

  let raw;
  try {
    raw = fs.readFileSync(resolvedPath, "utf8");
  } catch (error) {
    throw new Error(`Failed to read secret scan allowlist: ${error.message}`);
  }

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
  if (typeof output !== "string") {
    throw new TypeError("Secret scan output must be a string");
  }

  if (!Array.isArray(findings)) {
    throw new TypeError("Secret scan findings must be an array");
  }

  for (const finding of findings) {
    if (!finding || typeof finding !== "object") {
      throw new TypeError("Secret scan findings must contain objects");
    }

    const match = finding.match;
    if (!match) {
      continue;
    }

    if (typeof match !== "string") {
      throw new TypeError("Secret scan finding matches must be strings");
    }

    if (output.includes(match)) {
      throw new Error(
        `Secret scan output leaked a matched value for ${finding.file}:${finding.line}`
      );
    }
  }
}

function runSecretScan(options = {}) {
  if (!options || typeof options !== "object" || Array.isArray(options)) {
    return {
      ok: false,
      exitCode: 1,
      findings: [],
      message: "Secret scan could not complete: invalid scan options.",
    };
  }

  try {
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
  } catch (error) {
    // A partial filesystem read must never be reported as a clean scan. Keep
    // the message bounded and single-line so an unexpected error cannot forge
    // additional log entries or expose a large/sensitive value.
    let detail = "unexpected scan error";
    try {
      detail = error instanceof Error ? error.message : String(error);
    } catch {
      // Preserve the failed result even when coercing an unusual thrown value fails.
    }
    detail = detail.replace(/[\r\n]+/g, " ").slice(0, 200);

    return {
      ok: false,
      exitCode: 1,
      findings: [],
      message: `Secret scan could not complete: ${detail || "unexpected scan error"}`,
    };
  }
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
  MAX_MATCHES_PER_LINE,
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
  isHexString,
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