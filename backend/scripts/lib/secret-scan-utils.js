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

function redactPreview(value) {
  if (!value) {
    return '""';
  }

  if (value.length <= 8) {
    return `"${"*".repeat(value.length)}"`;
  }

  return `"${value.slice(0, 4)}...${value.slice(-4)}"`;
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

  if (hasLine && Number(entry.line) !== lineNumber) {
    return false;
  }

  if (hasMatch && !matchValue.includes(entry.match)) {
    return false;
  }

  if (hasPattern) {
    const pattern = new RegExp(entry.pattern);
    if (!pattern.test(matchValue)) {
      return false;
    }
  }

  return true;
}

function isAllowlisted(relativePath, lineNumber, matchValue, allowlist) {
  const normalized = normalizeAllowlist(allowlist);

  for (const entry of normalized.entries) {
    if (matchesAllowlistEntry(entry, relativePath, lineNumber, matchValue)) {
      return true;
    }
  }

  for (const entry of normalized.globalPatterns) {
    if (!entry?.pattern) {
      continue;
    }

    const pattern = new RegExp(entry.pattern);
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
  assertNoSecretsPrinted,
  collectHighEntropyMatches,
  collectQuotedStringMatches,
  collectRegexMatches,
  collectScanTargets,
  formatFinding,
  formatFindings,
  isAllowlisted,
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
