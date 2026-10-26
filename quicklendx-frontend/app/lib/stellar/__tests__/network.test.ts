import type { NetworkConfig } from "../network";

describe("Stellar network configuration", () => {
  const originalEnv = { ...process.env };

  afterEach(() => {
    process.env = { ...originalEnv };
    jest.resetModules();
  });

  it("defaults to testnet and its passphrase", () => {
    delete process.env.NEXT_PUBLIC_NETWORK;
    delete process.env.NEXT_PUBLIC_RPC_URL;
    delete process.env.NEXT_PUBLIC_HORIZON_URL;
    delete process.env.NEXT_PUBLIC_CONTRACT_ID;
    let config: NetworkConfig | undefined;
    jest.isolateModules(() => {
      config = require("../network").getNetworkConfig();
    });

    expect(config).toMatchObject({
      network: "testnet",
      passphrase: "Test SDF Network ; September 2015",
      rpcUrl: "https://soroban-testnet.stellar.org",
    });
  });

  it("accepts mainnet as an alias for public", () => {
    process.env.NEXT_PUBLIC_NETWORK = "mainnet";
    let config: NetworkConfig | undefined;
    jest.isolateModules(() => {
      config = require("../network").getNetworkConfig();
    });
    expect(config?.network).toBe("public");
    expect(config?.passphrase).toBe(
      "Public Global Stellar Network ; September 2015"
    );
  });

  it("rejects an insecure public RPC URL in development", () => {
    process.env.NODE_ENV = "development";
    process.env.NEXT_PUBLIC_NETWORK = "public";
    process.env.NEXT_PUBLIC_RPC_URL = "http://rpc.example";

    expect(() => jest.isolateModules(() => require("../network"))).toThrow(
      /must use https/
    );
  });

  it("rejects malformed contract IDs in development", () => {
    process.env.NODE_ENV = "development";
    process.env.NEXT_PUBLIC_CONTRACT_ID = "XYZ";

    expect(() => jest.isolateModules(() => require("../network"))).toThrow(
      /Invalid Stellar network configuration/
    );
  });

  it("logs once and falls back to testnet for invalid production settings", () => {
    process.env.NODE_ENV = "production";
    process.env.NEXT_PUBLIC_CONTRACT_ID = "XYZ";
    const errorSpy = jest.spyOn(console, "error").mockImplementation(() => {});
    let config: NetworkConfig | undefined;
    jest.isolateModules(() => {
      config = require("../network").getNetworkConfig();
    });

    expect(config?.network).toBe("testnet");
    expect(errorSpy).toHaveBeenCalledTimes(1);
    errorSpy.mockRestore();
  });
});
