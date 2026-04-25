import { describe, it, expect, beforeEach, afterEach } from "@jest/globals";

// Re-load config with a clean env each test by resetting the module registry.
function loadConfig(overrides: Record<string, string | undefined> = {}) {
  jest.resetModules();
  const env = { ...process.env, ...overrides };
  jest.replaceProperty(process, "env", env as NodeJS.ProcessEnv);
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  return require("../config") as typeof import("../config");
}

describe("config", () => {
  const original = { ...process.env };

  afterEach(() => {
    jest.resetModules();
    jest.replaceProperty(process, "env", original as NodeJS.ProcessEnv);
  });

  it("loads defaults when only NODE_ENV=test is set", () => {
    const { config } = loadConfig({ NODE_ENV: "test" });
    expect(config.PORT).toBe(3001);
    expect(config.RATE_LIMIT_POINTS).toBe(100);
    expect(config.STELLAR_RPC_URL).toBe("https://soroban-testnet.stellar.org");
  });

  it("coerces PORT from string to number", () => {
    const { config } = loadConfig({ NODE_ENV: "test", PORT: "4000" });
    expect(config.PORT).toBe(4000);
  });

  it("coerces RATE_LIMIT_POINTS from string", () => {
    const { config } = loadConfig({ NODE_ENV: "test", RATE_LIMIT_POINTS: "50" });
    expect(config.RATE_LIMIT_POINTS).toBe(50);
  });

  it("accepts a valid STELLAR_RPC_URL override", () => {
    const { config } = loadConfig({
      NODE_ENV: "test",
      STELLAR_RPC_URL: "https://soroban-mainnet.stellar.org",
    });
    expect(config.STELLAR_RPC_URL).toBe("https://soroban-mainnet.stellar.org");
  });

  it("rejects an invalid STELLAR_RPC_URL", () => {
    expect(() => loadConfig({ NODE_ENV: "test", STELLAR_RPC_URL: "not-a-url" })).toThrow(
      "Invalid configuration"
    );
  });

  it("rejects PORT out of range", () => {
    expect(() => loadConfig({ NODE_ENV: "test", PORT: "99999" })).toThrow(
      "Invalid configuration"
    );
  });

  it("rejects invalid NODE_ENV", () => {
    expect(() => loadConfig({ NODE_ENV: "staging" as any })).toThrow(
      "Invalid configuration"
    );
  });

  it("accepts optional secrets in non-production", () => {
    const { config } = loadConfig({
      NODE_ENV: "development",
      ADMIN_API_KEY: "a".repeat(32),
    });
    expect(config.ADMIN_API_KEY).toBe("a".repeat(32));
  });

  it("allows missing secrets in development", () => {
    const { config } = loadConfig({ NODE_ENV: "development" });
    expect(config.ADMIN_API_KEY).toBeUndefined();
    expect(config.WEBHOOK_SECRET).toBeUndefined();
  });

  it("requires ADMIN_API_KEY in production", () => {
    expect(() =>
      loadConfig({ NODE_ENV: "production", WEBHOOK_SECRET: "b".repeat(16) })
    ).toThrow("Invalid configuration");
  });

  it("requires WEBHOOK_SECRET in production", () => {
    expect(() =>
      loadConfig({ NODE_ENV: "production", ADMIN_API_KEY: "a".repeat(32) })
    ).toThrow("Invalid configuration");
  });

  it("requires secrets to meet minimum length in production", () => {
    expect(() =>
      loadConfig({
        NODE_ENV: "production",
        ADMIN_API_KEY: "short",
        WEBHOOK_SECRET: "b".repeat(16),
      })
    ).toThrow("Invalid configuration");
  });

  it("error message lists field names but not values", () => {
    let message = "";
    try {
      loadConfig({ NODE_ENV: "test", PORT: "bad" });
    } catch (e: any) {
      message = e.message;
    }
    expect(message).toContain("PORT");
    expect(message).not.toContain("bad"); // value must not leak
  });

  it("loads successfully with all production secrets provided", () => {
    const { config } = loadConfig({
      NODE_ENV: "production",
      ADMIN_API_KEY: "a".repeat(32),
      WEBHOOK_SECRET: "b".repeat(16),
    });
    expect(config.NODE_ENV).toBe("production");
  });
});
