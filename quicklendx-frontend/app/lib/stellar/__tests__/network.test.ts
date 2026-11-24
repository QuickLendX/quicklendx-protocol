/**
 * Tests for getNetworkConfig (#3145).
 *
 * Run with: npx jest app/lib/stellar/__tests__/network.test.ts
 */

const TESTNET_PASSPHRASE = "Test SDF Network ; September 2015";
const PUBLIC_PASSPHRASE = "Public Global Stellar Network ; September 2015";

const ORIGINAL_NETWORK = process.env.NEXT_PUBLIC_NETWORK;

/**
 * Load a fresh copy of the module. The "already warned" memory lives at module
 * level, so every test gets its own copy and cannot be affected by another
 * test's warnings.
 */
function loadModule(): typeof import("../network") {
  let mod!: typeof import("../network");
  jest.isolateModules(() => {
    mod = require("../network");
  });
  return mod;
}

function setNetwork(value: string | undefined): void {
  if (value === undefined) {
    delete process.env.NEXT_PUBLIC_NETWORK;
  } else {
    process.env.NEXT_PUBLIC_NETWORK = value;
  }
}

describe("getNetworkConfig", () => {
  let warnSpy: jest.SpyInstance;

  beforeEach(() => {
    warnSpy = jest.spyOn(console, "warn").mockImplementation(() => {});
    delete process.env.NEXT_PUBLIC_NETWORK;
  });

  afterEach(() => {
    warnSpy.mockRestore();
    if (ORIGINAL_NETWORK === undefined) {
      delete process.env.NEXT_PUBLIC_NETWORK;
    } else {
      process.env.NEXT_PUBLIC_NETWORK = ORIGINAL_NETWORK;
    }
  });

  describe("default", () => {
    it("is testnet when NEXT_PUBLIC_NETWORK is unset, without warning", () => {
      const { getNetworkConfig } = loadModule();

      expect(getNetworkConfig()).toEqual({
        network: "testnet",
        passphrase: TESTNET_PASSPHRASE,
      });
      expect(warnSpy).not.toHaveBeenCalled();
    });

    it.each(["", "   ", "\t\n"])(
      "treats the blank value %j as unset, without warning",
      (blank) => {
        setNetwork(blank);
        const { getNetworkConfig } = loadModule();

        expect(getNetworkConfig().network).toBe("testnet");
        expect(warnSpy).not.toHaveBeenCalled();
      }
    );
  });

  describe("testnet", () => {
    it.each(["testnet", "TESTNET", "TestNet", "  testnet  "])(
      "resolves %j to testnet without warning",
      (value) => {
        setNetwork(value);
        const { getNetworkConfig } = loadModule();

        expect(getNetworkConfig()).toEqual({
          network: "testnet",
          passphrase: TESTNET_PASSPHRASE,
        });
        expect(warnSpy).not.toHaveBeenCalled();
      }
    );
  });

  describe("public", () => {
    it.each(["public", "PUBLIC", "Public", "  public  "])(
      "resolves %j to public without warning",
      (value) => {
        setNetwork(value);
        const { getNetworkConfig } = loadModule();

        expect(getNetworkConfig()).toEqual({
          network: "public",
          passphrase: PUBLIC_PASSPHRASE,
        });
        expect(warnSpy).not.toHaveBeenCalled();
      }
    );

    it.each(["mainnet", "MAINNET", "Mainnet", "  mainnet  "])(
      "accepts %j as an alias of public without warning",
      (value) => {
        setNetwork(value);
        const { getNetworkConfig } = loadModule();

        expect(getNetworkConfig()).toEqual({
          network: "public",
          passphrase: PUBLIC_PASSPHRASE,
        });
        expect(warnSpy).not.toHaveBeenCalled();
      }
    );
  });

  describe("unrecognised values", () => {
    it("falls back to testnet and warns once for a garbage value", () => {
      setNetwork("not-a-network");
      const { getNetworkConfig } = loadModule();

      const first = getNetworkConfig();
      const second = getNetworkConfig();
      const third = getNetworkConfig();

      expect(first).toEqual({
        network: "testnet",
        passphrase: TESTNET_PASSPHRASE,
      });
      expect(second).toEqual(first);
      expect(third).toEqual(first);
      expect(warnSpy).toHaveBeenCalledTimes(1);
    });

    it("names the offending value and the accepted ones in the warning", () => {
      setNetwork("mainet");
      const { getNetworkConfig } = loadModule();

      getNetworkConfig();

      const message = String(warnSpy.mock.calls[0][0]);
      expect(message).toContain("NEXT_PUBLIC_NETWORK");
      expect(message).toContain('"mainet"');
      expect(message).toContain('falling back to "testnet"');
      expect(message).toContain("public");
      expect(message).toContain("mainnet");
    });

    it("warns again for a different unrecognised value, but not for a repeat", () => {
      const { getNetworkConfig } = loadModule();

      setNetwork("typo-one");
      getNetworkConfig();
      getNetworkConfig();
      setNetwork("typo-two");
      getNetworkConfig();
      setNetwork("typo-one");
      getNetworkConfig();

      expect(warnSpy).toHaveBeenCalledTimes(2);
    });

    it("does not warn for a recognised value that follows an unrecognised one", () => {
      const { getNetworkConfig } = loadModule();

      setNetwork("oops");
      getNetworkConfig();
      setNetwork("public");
      const config = getNetworkConfig();

      expect(config.network).toBe("public");
      expect(warnSpy).toHaveBeenCalledTimes(1);
    });

    it.each(["production", "live", "main", "pubic", "test-net"])(
      "does not silently map %j to a real network",
      (value) => {
        setNetwork(value);
        const { getNetworkConfig } = loadModule();

        expect(getNetworkConfig().network).toBe("testnet");
        expect(warnSpy).toHaveBeenCalledTimes(1);
      }
    );

    it.each(["constructor", "__proto__", "toString", "hasOwnProperty"])(
      "treats the Object.prototype name %j as unrecognised",
      (value) => {
        setNetwork(value);
        const { getNetworkConfig } = loadModule();

        const config = getNetworkConfig();

        expect(config).toEqual({
          network: "testnet",
          passphrase: TESTNET_PASSPHRASE,
        });
        expect(warnSpy).toHaveBeenCalledTimes(1);
      }
    );

    it("truncates a very long value in the warning", () => {
      setNetwork("x".repeat(500));
      const { getNetworkConfig } = loadModule();

      getNetworkConfig();

      const message = String(warnSpy.mock.calls[0][0]);
      expect(message.length).toBeLessThan(250);
      expect(message).toContain("…");
    });

    it("never throws", () => {
      setNetwork("<script>alert(1)</script>");
      const { getNetworkConfig } = loadModule();

      expect(() => getNetworkConfig()).not.toThrow();
      expect(getNetworkConfig().network).toBe("testnet");
    });
  });

  describe("reading the environment", () => {
    it("reads NEXT_PUBLIC_NETWORK on every call, not once at import", () => {
      const { getNetworkConfig } = loadModule();

      setNetwork("testnet");
      expect(getNetworkConfig().network).toBe("testnet");
      setNetwork("public");
      expect(getNetworkConfig().network).toBe("public");
      setNetwork(undefined);
      expect(getNetworkConfig().network).toBe("testnet");
    });
  });

  describe("passphrases", () => {
    it("uses the exact Stellar passphrases", () => {
      setNetwork("testnet");
      const testnet = loadModule().getNetworkConfig();
      setNetwork("public");
      const publicNet = loadModule().getNetworkConfig();

      expect(testnet.passphrase).toBe("Test SDF Network ; September 2015");
      expect(publicNet.passphrase).toBe(
        "Public Global Stellar Network ; September 2015"
      );
    });
  });
});
