import { main } from "../scripts/validate-sbom";

describe("validate-sbom main boundary coverage", () => {
  let mockProcess: any;
  let mockFs: any;
  let mockConsole: any;
  let exitCode: number | undefined;

  beforeEach(() => {
    exitCode = undefined;
    mockProcess = {
      cwd: jest.fn().mockReturnValue("/mock/cwd"),
      exit: jest.fn().mockImplementation((code) => {
        exitCode = code;
      }),
      argv: ["node", "script.js"],
    };
    mockConsole = {
      error: jest.fn(),
      log: jest.fn(),
    };
    mockFs = {
      existsSync: jest.fn().mockReturnValue(true),
      readFileSync: jest.fn(),
    };
  });

  const runMain = (args = mockProcess.argv) => {
    main(args, {
      process: mockProcess,
      fs: mockFs,
      console: mockConsole,
    });
  };

  it("handles successful execution normally (loading state)", () => {
    mockFs.readFileSync.mockReturnValue(
      JSON.stringify({
        bomFormat: "CycloneDX",
        specVersion: "1.5",
        metadata: { component: { type: "application", name: "app" } },
        components: [],
      })
    );
    runMain(["node", "script.js", "valid.json"]);
    expect(exitCode).toBe(0);
    expect(mockConsole.log).toHaveBeenCalledWith(
      expect.stringContaining("SBOM check passed: 0 components documented")
    );
  });

  it("handles missing file error gracefully", () => {
    mockFs.existsSync.mockReturnValue(false);
    runMain(["node", "script.js", "missing.json"]);
    expect(exitCode).toBe(1);
    expect(mockConsole.error).toHaveBeenCalledWith(
      expect.stringContaining("File not found")
    );
  });

  it("handles permission error deterministically (permission states)", () => {
    mockFs.readFileSync.mockImplementation(() => {
      const error: any = new Error("EACCES: permission denied");
      error.code = "EACCES";
      throw error;
    });
    runMain(["node", "script.js", "secret.json"]);
    expect(exitCode).toBe(1);
    expect(mockConsole.error).toHaveBeenCalledWith(
      expect.stringContaining("Permission denied accessing")
    );
  });

  it("retries on temporary read failures and succeeds (retry states)", () => {
    let attempts = 0;
    mockFs.readFileSync.mockImplementation(() => {
      attempts++;
      if (attempts < 3) {
        throw new Error("Temporary read error");
      }
      return JSON.stringify({
        bomFormat: "CycloneDX",
        specVersion: "1.5",
        metadata: { component: { type: "application", name: "app" } },
        components: [],
      });
    });
    runMain(["node", "script.js", "retryable.json"]);
    expect(exitCode).toBe(0);
    expect(attempts).toBe(3);
  });

  it("fails deterministically after max retries (stale/unrecoverable states)", () => {
    mockFs.readFileSync.mockImplementation(() => {
      throw new Error("Persistent read error");
    });
    runMain(["node", "script.js", "broken.json"]);
    expect(exitCode).toBe(1);
    expect(mockConsole.error).toHaveBeenCalledWith(
      expect.stringContaining("Could not read file after retries")
    );
  });

  it("handles invalid JSON without losing data (error states)", () => {
    mockFs.readFileSync.mockReturnValue("{ invalid json");
    runMain(["node", "script.js", "bad-json.json"]);
    expect(exitCode).toBe(1);
    expect(mockConsole.error).toHaveBeenCalledWith(
      expect.stringContaining("Invalid JSON")
    );
  });

  it("handles SBOM validation issues deterministically", () => {
    mockFs.readFileSync.mockReturnValue(
      JSON.stringify({
        bomFormat: "CycloneDX",
        specVersion: "1.5",
        metadata: { component: { type: "application", name: "app" } },
        components: [{}], // Invalid component missing name/type
      })
    );
    runMain(["node", "script.js", "invalid-sbom.json"]);
    expect(exitCode).toBe(1);
    expect(mockConsole.error).toHaveBeenCalledWith(
      expect.stringContaining("SBOM check failed with the following issues:")
    );
  });
});
