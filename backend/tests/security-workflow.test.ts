import fs from "node:fs";
import path from "node:path";

describe("Backend CI workflow security controls", () => {
  const workflowPath = path.resolve(__dirname, "../../.github/workflows/backend-ci.yml");
  const workflow = fs.readFileSync(workflowPath, "utf8");

  it("includes a dedicated dependency scan job", () => {
    expect(workflow).toContain("security_scan:");
    expect(workflow).toContain("name: Dependency Vulnerability Scan");
    expect(workflow).toContain("npm run security:scan");
  });

  it("captures npm audit JSON for clear CI troubleshooting", () => {
    expect(workflow).toContain("npm audit --json > audit-report.json || true");
    expect(workflow).toContain("actions/upload-artifact@v4");
    expect(workflow).toContain("name: backend-audit-report");
  });

  it("generates and validates CycloneDX SBOM in CI", () => {
    expect(workflow).toContain("sbom:");
    expect(workflow).toContain("npm run sbom:generate");
    expect(workflow).toContain("npm run sbom:check");
    expect(workflow).toContain("name: backend-sbom-");
  });
});
