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

function describeType(value) {
  if (value === null) {
    return "null";
  }

  if (Array.isArray(value)) {
    return "array";
  }

  return typeof value;
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

/**
 * Strips one surrounding quote pair from a string literal, or returns the
 * input unchanged when it is not quoted.
 *
 * Deterministic contract (invariant 5 above):
 * - non-string input throws a `TypeError` naming the received type instead of
 *   being indexed and passed through (a `symbol` used to yield `undefined`, a
 *   number used to return a number, and `null` used to throw an opaque
 *   "Cannot read properties of null");
 * - a string that opens with `'`, `"` or `` ` `` must close with the same
 *   quote character and be at least two characters long, otherwise the literal
 *   is unbalanced and throws a `TypeError` instead of being truncated:
 *   `slice(1, -1)` silently rewrote `"abc` to `ab` and a lone `"` to `""`;
 * - everything else (including the empty string) is returned byte-for-byte, so
 *   an unquoted value such as `abc"` is never altered.
 *
 * As with `assertScannableLineArguments`, every rejection is a `TypeError`
 * carrying the same contract, distinguished only by its message. Neither
 * message carries literal text: an unbalanced literal reaching this function
 * may contain a real secret, and errors surface verbatim in CI logs through
 * `secret-scan.js`.
 */
function unquoteString(literal) {
  if (typeof literal !== "string") {
    throw new TypeError(
      `unquoteString requires a string literal, received ${describeType(literal)}.`
    );
  }

  const quote = literal[0];
  if (quote !== "'" && quote !== '"' && quote !== "`") {
    return literal;
  }

  if (literal.length < 2 || literal[literal.length - 1] !== quote) {
    throw new TypeError(
      "unquoteString requires a balanced quoted literal: the opening quote must be " +
        "closed by the same quote character. The literal text is omitted because it may contain a secret."
    );
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

  return "unknown error";
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
  MIN_PARTIAL_PREVIEW_LENGTH,
  MIN_PREVIEW_EDGE_LENGTH,
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
  assertAllowlistPatternsCompilable,
  collectHighEntropyMatches,
  collectQuotedStringMatches,
  collectRegexMatches,
  collectScanTargets,
  compilePattern,
  formatFinding,
  formatFindings,
  formatTargetFailure,
  hasMixedCharacterClasses,
  isAllowlisted,
  isAllowlistedIn,
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