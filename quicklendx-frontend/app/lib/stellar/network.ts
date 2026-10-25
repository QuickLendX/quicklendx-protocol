import { z } from "zod";

export type StellarNetwork = "testnet" | "public";

export interface NetworkConfig {
  network: StellarNetwork;
  passphrase: string;
  rpcUrl: string;
  horizonUrl: string;
  explorerBase: string;
  contractId?: string;
}

const PASSPHRASES: Record<StellarNetwork, string> = {
  testnet: "Test SDF Network ; September 2015",
  public: "Public Global Stellar Network ; September 2015",
};

const DEFAULT_ENDPOINTS: Record<
  StellarNetwork,
  Pick<NetworkConfig, "rpcUrl" | "horizonUrl">
> = {
  testnet: {
    rpcUrl: "https://soroban-testnet.stellar.org",
    horizonUrl: "https://horizon-testnet.stellar.org",
  },
  public: {
    rpcUrl: "https://mainnet.sorobanrpc.com",
    horizonUrl: "https://horizon.stellar.org",
  },
};

const configSchema = z
  .object({
    network: z.preprocess(
      (value) => {
        const normalized = value ?? "testnet";
        return normalized === "mainnet" ? "public" : normalized;
      },
      z.enum(["testnet", "public"])
    ),
    rpcUrl: z.string().url().optional(),
    horizonUrl: z.string().url().optional(),
    contractId: z
      .string()
      .regex(/^C[A-Z2-7]{55}$/)
      .optional(),
  })
  .superRefine((config, context) => {
    if (config.network !== "public") return;
    for (const [key, value] of [
      ["rpcUrl", config.rpcUrl],
      ["horizonUrl", config.horizonUrl],
    ] as const) {
      if (value && new URL(value).protocol !== "https:") {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: [key],
          message: `${key} must use https on the public network`,
        });
      }
    }
  });

function loadNetworkConfig(): NetworkConfig {
  const parsed = configSchema.safeParse({
    network: process.env.NEXT_PUBLIC_NETWORK,
    rpcUrl: process.env.NEXT_PUBLIC_RPC_URL || undefined,
    horizonUrl: process.env.NEXT_PUBLIC_HORIZON_URL || undefined,
    contractId: process.env.NEXT_PUBLIC_CONTRACT_ID || undefined,
  });

  if (!parsed.success) {
    const message = `Invalid Stellar network configuration: ${parsed.error.message}`;
    if (process.env.NODE_ENV !== "production") throw new Error(message);
    console.error(message);
    return {
      network: "testnet",
      passphrase: PASSPHRASES.testnet,
      ...DEFAULT_ENDPOINTS.testnet,
      explorerBase: "https://stellar.expert/explorer/testnet",
    };
  }

  const { network, contractId } = parsed.data;
  return {
    network,
    passphrase: PASSPHRASES[network],
    rpcUrl: parsed.data.rpcUrl ?? DEFAULT_ENDPOINTS[network].rpcUrl,
    horizonUrl: parsed.data.horizonUrl ?? DEFAULT_ENDPOINTS[network].horizonUrl,
    explorerBase: `https://stellar.expert/explorer/${network}`,
    ...(contractId ? { contractId } : {}),
  };
}

// Parse at import time so invalid deployment configuration fails during startup/build.
const networkConfig = loadNetworkConfig();

export function getNetworkConfig(): NetworkConfig {
  return networkConfig;
}
