#!/usr/bin/env bash
# Generate docs/events.schema.json from quicklendx-contracts/src/events_schema.rs.
#
# Usage:
#   scripts/gen-events-schema.sh
#
# Reproducibility check:
#   scripts/gen-events-schema.sh && git diff --exit-code docs/events.schema.json
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

SCHEMA_RS="${REPO_ROOT}/quicklendx-contracts/src/events_schema.rs"
OUT_JSON="${REPO_ROOT}/docs/events.schema.json"

if command -v cygpath >/dev/null 2>&1; then
  SCHEMA_RS="$(cygpath -m "${SCHEMA_RS}")"
  OUT_JSON="$(cygpath -m "${OUT_JSON}")"
fi

if ! command -v node >/dev/null 2>&1; then
  echo "Error: node is required to run scripts/gen-events-schema.sh" >&2
  exit 1
fi

SCHEMA_RS="${SCHEMA_RS}" OUT_JSON="${OUT_JSON}" node <<'JS'
const fs = require("fs");
const path = require("path");

const schemaRsPath = process.env.SCHEMA_RS;
const outJsonPath = process.env.OUT_JSON;

const schemaRs = fs.readFileSync(schemaRsPath, "utf8");

const tableMatch = schemaRs.match(
  /pub const EVENT_SCHEMA:\s*&\[EventSchema\]\s*=\s*&\[([\s\S]*?)\r?\n\];/
);
if (!tableMatch) {
  console.error("Failed to locate EVENT_SCHEMA in events_schema.rs");
  process.exit(1);
}

const body = tableMatch[1];

const fieldRe = /field\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)/g;
const entryRe =
  /EventSchema\s*\{\s*emitter:\s*"([^"]+)"\s*,\s*topic:\s*"([^"]+)"\s*,\s*topic_fields:\s*&\[([\s\S]*?)\]\s*,\s*fields:\s*&\[([\s\S]*?)\]\s*,\s*format:\s*"([^"]+)"\s*,?\s*\}/g;

const parseFields = (raw) => {
  const result = [];
  let m;
  fieldRe.lastIndex = 0;
  while ((m = fieldRe.exec(raw)) !== null) {
    result.push({ name: m[1], type: m[2] });
  }
  return result;
};

const events = [];
let em;
while ((em = entryRe.exec(body)) !== null) {
  const [, emitter, topic, topicFieldsRaw, fieldsRaw, format] = em;
  events.push({
    emitter,
    topic,
    format,
    topic_fields: parseFields(topicFieldsRaw),
    fields: parseFields(fieldsRaw),
  });
}

if (events.length === 0) {
  console.error("Parsed 0 EventSchema entries from events_schema.rs");
  process.exit(1);
}

const doc = {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  title: "QuickLendX Contract Events Schema",
  description:
    "Authoritative machine-readable schema of all on-chain events emitted by quicklendx-contracts. Generated from quicklendx-contracts/src/events_schema.rs via scripts/gen-events-schema.sh.",
  source: "quicklendx-contracts/src/events_schema.rs",
  event_count: events.length,
  events,
};

fs.mkdirSync(path.dirname(outJsonPath), { recursive: true });
fs.writeFileSync(outJsonPath, JSON.stringify(doc, null, 2) + "\n", "utf8");
console.log(`Generated ${outJsonPath} (${events.length} events)`);
JS
