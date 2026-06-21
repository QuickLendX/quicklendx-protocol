import path from "node:path";
import { execFileSync } from "node:child_process";

describe("SBOM validation script", () => {
  const repoRoot = path.resolve(__dirname, "..");
  const scriptPath = path.resolve(repoRoot, "scripts/validate-sbom.js");
  const validFixture = path.resolve(repoRoot, "tests/fixtures/sample-sbom.cdx.json");
  const invalidFixture = path.resolve(repoRoot, "tests/fixtures/invalid-sbom.cdx.json");

  it("passes for a valid CycloneDX SBOM", () => {
    expect(() => {
      execFileSync("node", [scriptPath, validFixture], {
        cwd: repoRoot,
        stdio: "pipe",
      });
    }).not.toThrow();
  });

  it("fails for malformed SBOM content", () => {
    expect(() => {
      execFileSync("node", [scriptPath, invalidFixture], {
        cwd: repoRoot,
        stdio: "pipe",
      });
    }).toThrow();
  });
});
