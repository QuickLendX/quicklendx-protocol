/**
 * network.ts — Stellar network configuration helper.
 *
 * Reads NEXT_PUBLIC_NETWORK ("testnet" | "public") from the environment.
 * Defaults to "testnet" if unset.
 *
 * Usage:
 *   import { getNetworkConfig } from "@/app/lib/stellar/network";
 *   const { network, passphrase } = getNetworkConfig();
 */

export type StellarNetwork = "testnet" | "public";

export interface NetworkConfig {
  network: StellarNetwork;
  /** Full Stellar network passphrase used in transaction envelopes. */
  passphrase: string;
}

const PASSPHRASES: Record<StellarNetwork, string> = {
  testnet: "Test SDF Network ; September 2015",
  public: "Public Global Stellar Network ; September 2015",
};

/**
 * Returns the Stellar network configuration derived from the
 * NEXT_PUBLIC_NETWORK environment variable.
 *
 * Call this on both the server and the client — it never accesses `window`.
 */
export function getNetworkConfig(): NetworkConfig {
  const raw = process.env.NEXT_PUBLIC_NETWORK ?? "testnet";
  const network: StellarNetwork =
    raw === "public" ? "public" : "testnet";

  return {
    network,
    passphrase: PASSPHRASES[network],
  };
}
