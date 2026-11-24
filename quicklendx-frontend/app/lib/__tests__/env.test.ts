import { DEFAULT_API_URL, DEFAULT_NETWORK, parseEnv } from "../env";

describe("env validation", () => {
  describe("defaults", () => {
    it("applies default values when all variables are unset", () => {
      const result = parseEnv({});

      expect(result).toEqual({
        NEXT_PUBLIC_API_URL: DEFAULT_API_URL,
        NEXT_PUBLIC_NETWORK: DEFAULT_NETWORK,
        NEXT_PUBLIC_E2E: false,
        NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE: false,
        NEXT_PUBLIC_ANALYTICS_ENABLED: false,
      });
    });
  });

  describe("valid values", () => {
    it("accepts valid http and https URLs ending with /api/v1", () => {
      expect(
        parseEnv({
          NEXT_PUBLIC_API_URL: "http://localhost:4000/api/v1",
        }).NEXT_PUBLIC_API_URL
      ).toBe("http://localhost:4000/api/v1");

      expect(
        parseEnv({
          NEXT_PUBLIC_API_URL: "https://api.quicklendx.io/api/v1",
        }).NEXT_PUBLIC_API_URL
      ).toBe("https://api.quicklendx.io/api/v1");
    });

    it("accepts testnet and public for NEXT_PUBLIC_NETWORK", () => {
      expect(
        parseEnv({ NEXT_PUBLIC_NETWORK: "testnet" }).NEXT_PUBLIC_NETWORK
      ).toBe("testnet");
      expect(
        parseEnv({ NEXT_PUBLIC_NETWORK: "public" }).NEXT_PUBLIC_NETWORK
      ).toBe("public");
    });

    it("parses boolean flags from true/false/1/0 strings", () => {
      expect(
        parseEnv({
          NEXT_PUBLIC_E2E: "1",
          NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE: "true",
          NEXT_PUBLIC_ANALYTICS_ENABLED: "TRUE",
        })
      ).toMatchObject({
        NEXT_PUBLIC_E2E: true,
        NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE: true,
        NEXT_PUBLIC_ANALYTICS_ENABLED: true,
      });

      expect(
        parseEnv({
          NEXT_PUBLIC_E2E: "0",
          NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE: "false",
          NEXT_PUBLIC_ANALYTICS_ENABLED: "FALSE",
        })
      ).toMatchObject({
        NEXT_PUBLIC_E2E: false,
        NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE: false,
        NEXT_PUBLIC_ANALYTICS_ENABLED: false,
      });
    });
  });

  describe("NEXT_PUBLIC_API_URL failures", () => {
    it("rejects a relative URL with a readable message", () => {
      expect(() => parseEnv({ NEXT_PUBLIC_API_URL: "/api/v1" })).toThrow(
        /Invalid environment configuration:/
      );
      expect(() => parseEnv({ NEXT_PUBLIC_API_URL: "/api/v1" })).toThrow(
        /NEXT_PUBLIC_API_URL must be an absolute http\(s\) URL/
      );
    });

    it("rejects a non-http(s) protocol with a readable message", () => {
      expect(() =>
        parseEnv({ NEXT_PUBLIC_API_URL: "ftp://localhost:3001/api/v1" })
      ).toThrow(
        /NEXT_PUBLIC_API_URL must use http:\/\/ or https:\/\/ protocol/
      );
    });

    it("rejects a URL not ending with /api/v1", () => {
      expect(() =>
        parseEnv({ NEXT_PUBLIC_API_URL: "http://localhost:3001" })
      ).toThrow(/NEXT_PUBLIC_API_URL must end with \/api\/v1/);

      expect(() =>
        parseEnv({ NEXT_PUBLIC_API_URL: "https://api.example.com/api/v2" })
      ).toThrow(/NEXT_PUBLIC_API_URL must end with \/api\/v1/);

      expect(() =>
        parseEnv({ NEXT_PUBLIC_API_URL: "http://localhost:3001/api/v1/" })
      ).toThrow(/NEXT_PUBLIC_API_URL must end with \/api\/v1/);
    });

    it("rejects an empty string", () => {
      expect(() => parseEnv({ NEXT_PUBLIC_API_URL: "" })).toThrow(
        /NEXT_PUBLIC_API_URL must be an absolute http\(s\) URL/
      );
    });
  });

  describe("NEXT_PUBLIC_NETWORK failures", () => {
    it("rejects unknown network values with a readable message", () => {
      expect(() => parseEnv({ NEXT_PUBLIC_NETWORK: "mainnet" })).toThrow(
        /NEXT_PUBLIC_NETWORK must be either "testnet" or "public"/
      );

      expect(() => parseEnv({ NEXT_PUBLIC_NETWORK: "" })).toThrow(
        /NEXT_PUBLIC_NETWORK must be either "testnet" or "public"/
      );
    });
  });

  describe("boolean flag failures", () => {
    it("rejects invalid NEXT_PUBLIC_E2E values with a readable message", () => {
      expect(() => parseEnv({ NEXT_PUBLIC_E2E: "yes" })).toThrow(
        /NEXT_PUBLIC_E2E must be a boolean flag/
      );
    });

    it("rejects invalid NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE values", () => {
      expect(() =>
        parseEnv({ NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE: "enabled" })
      ).toThrow(/NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE must be a boolean flag/);
    });

    it("rejects invalid NEXT_PUBLIC_ANALYTICS_ENABLED values", () => {
      expect(() => parseEnv({ NEXT_PUBLIC_ANALYTICS_ENABLED: "2" })).toThrow(
        /NEXT_PUBLIC_ANALYTICS_ENABLED must be a boolean flag/
      );
    });
  });

  describe("multiple failures and process.env integration", () => {
    const originalEnv = process.env;

    beforeEach(() => {
      jest.resetModules();
      process.env = { ...originalEnv };
      delete process.env.NEXT_PUBLIC_API_URL;
      delete process.env.NEXT_PUBLIC_NETWORK;
      delete process.env.NEXT_PUBLIC_E2E;
      delete process.env.NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE;
      delete process.env.NEXT_PUBLIC_ANALYTICS_ENABLED;
    });

    afterAll(() => {
      process.env = originalEnv;
    });

    it("reports all invalid variables together", () => {
      expect(() =>
        parseEnv({
          NEXT_PUBLIC_API_URL: "not-a-url",
          NEXT_PUBLIC_NETWORK: "invalid",
          NEXT_PUBLIC_E2E: "nope",
        })
      ).toThrow(
        /Invalid environment configuration:[\s\S]*NEXT_PUBLIC_API_URL[\s\S]*NEXT_PUBLIC_NETWORK[\s\S]*NEXT_PUBLIC_E2E/
      );
    });

    it("validates process.env on module load", async () => {
      process.env.NEXT_PUBLIC_API_URL = "https://staging.example.com/api/v1";
      process.env.NEXT_PUBLIC_NETWORK = "public";
      process.env.NEXT_PUBLIC_E2E = "1";

      const { env } = await import("../env");

      expect(env.NEXT_PUBLIC_API_URL).toBe(
        "https://staging.example.com/api/v1"
      );
      expect(env.NEXT_PUBLIC_NETWORK).toBe("public");
      expect(env.NEXT_PUBLIC_E2E).toBe(true);
    });
  });
});
