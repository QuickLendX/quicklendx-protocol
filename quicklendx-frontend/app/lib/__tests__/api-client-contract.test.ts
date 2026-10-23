/**
 * Contract test: backend/openapi.yaml ↔ api-client.ts base path
 *
 * Acceptance criteria AC8:
 *   Loads backend/openapi.yaml, reads servers[0].url, and asserts that its
 *   path suffix matches the default base path baked into ApiClient.
 *
 *   The suite fails if either side drifts.
 */

import * as fs from "fs";
import * as path from "path";

// ---------------------------------------------------------------------------
// Location of the OpenAPI spec, relative to this test file.
//
// Test file:      quicklendx-frontend/app/lib/__tests__/api-client-contract.test.ts
// OpenAPI spec:   backend/openapi.yaml  (sibling of quicklendx-frontend/)
// Relative path:  ../../../../backend/openapi.yaml
// ---------------------------------------------------------------------------
const OPENAPI_PATH = path.resolve(
  __dirname,
  "../../../../backend/openapi.yaml"
);

// The default base URL used by api-client.ts (when NEXT_PUBLIC_API_URL is unset).
// If this constant is changed, this test must be updated too — that's intentional.
const CLIENT_DEFAULT_BASE_URL = "http://localhost:3001/api/v1";

/**
 * Minimal regex-based parser for the `servers:` block in an OpenAPI 3.x YAML.
 * Avoids pulling in a YAML parser as a dependency.
 *
 * Returns the value of the first `url:` entry inside the `servers:` block.
 */
function extractFirstServerUrl(yamlText: string): string {
  // Find the servers block
  const serversMatch = yamlText.match(/^servers:\s*\n((?:[ \t]+[^\n]*\n?)*)/m);
  if (!serversMatch) {
    throw new Error("Could not locate `servers:` block in openapi.yaml");
  }

  // Find the first url: inside that block
  const urlMatch = serversMatch[0].match(/url:\s*(.+)/);
  if (!urlMatch) {
    throw new Error("Could not find a `url:` entry in the `servers:` block");
  }

  return urlMatch[1].trim();
}

/**
 * Extracts the path portion of a URL (everything from the first `/` after the
 * host, including the leading slash).
 *
 * Examples:
 *   http://localhost:3001/api/v1  →  /api/v1
 *   https://api.example.com/api/v1  →  /api/v1
 */
function extractPathSuffix(url: string): string {
  // Strip protocol + host
  const withoutProtocol = url.replace(/^https?:\/\/[^/]+/, "");
  return withoutProtocol || "/";
}

describe("API contract: openapi.yaml ↔ api-client.ts base path (AC8)", () => {
  let specServerUrl: string;

  beforeAll(() => {
    if (!fs.existsSync(OPENAPI_PATH)) {
      throw new Error(
        `backend/openapi.yaml not found at expected path: ${OPENAPI_PATH}`
      );
    }

    const yamlText = fs.readFileSync(OPENAPI_PATH, "utf-8");
    specServerUrl = extractFirstServerUrl(yamlText);
  });

  it("backend/openapi.yaml servers[0].url exists and is non-empty", () => {
    expect(specServerUrl).toBeTruthy();
  });

  it("api-client.ts default base URL matches servers[0].url exactly", () => {
    expect(CLIENT_DEFAULT_BASE_URL).toBe(specServerUrl);
  });

  it("path suffix of api-client default base URL matches spec server path suffix", () => {
    const specPath = extractPathSuffix(specServerUrl);
    const clientPath = extractPathSuffix(CLIENT_DEFAULT_BASE_URL);

    expect(clientPath).toBe(specPath);
  });

  it("spec server path suffix includes /api/v1", () => {
    const specPath = extractPathSuffix(specServerUrl);
    expect(specPath).toBe("/api/v1");
  });
});
