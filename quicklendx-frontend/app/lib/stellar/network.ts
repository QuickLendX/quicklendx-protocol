/**
 * network.ts — Stellar network configuration helper.
 *
 * Reads NEXT_PUBLIC_NETWORK from the environment and resolves it to one of two
 * networks:
 *
 *   value (case-insensitive, surrounding whitespace ignored)   network
 *   ---------------------------------------------------------   -------
 *   "testnet"                                                   testnet
 *   "public"                                                    public
 *   "mainnet"  (alias of "public")                              public
 *
 * Behaviour for everything else:
 *
 *   - Unset, empty or whitespace-only  -> "testnet", silently. This is the
 *     documented default, and an empty value is what an unfilled template or
 *     CI variable looks like.
 *   - Any other value (a typo such as "mainet", "production", "live")
 *     -> still "testnet", so the app keeps working, but a console.warn is
 *     emitted. The fallback is deliberately unchanged: the point of the warning
 *     is that a typo in a production deploy would otherwise quietly sign
 *     against the testnet passphrase with nothing in the logs to say so.
 *
 * The warning fires once per distinct unrecognised value per process, not on
 * every call, so a hot path that reads the config does not flood the console.
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

const DEFAULT_NETWORK: StellarNetwork = "testnet";

/**
 * Accepted spellings, keyed by their lower-cased form. A Map rather than an
 * object literal, so a value like "constructor" or "__proto__" cannot resolve to
 * something inherited from Object.prototype.
 */
const NETWORK_ALIASES: ReadonlyMap<string, StellarNetwork> = new Map<
  string,
  StellarNetwork
>([
  ["testnet", "testnet"],
  ["public", "public"],
  ["mainnet", "public"],
]);

/** Longest raw value echoed back in the warning. */
const MAX_LOGGED_LENGTH = 64;

/** Unrecognised values already warned about, so each is reported once. */
const warnedValues = new Set<string>();

function warnUnrecognised(value: string): void {
  if (warnedValues.has(value)) return;
  warnedValues.add(value);

  const shown =
    value.length > MAX_LOGGED_LENGTH
      ? `${value.slice(0, MAX_LOGGED_LENGTH)}…`
      : value;
  console.warn(
    `[quicklendx] Unrecognised NEXT_PUBLIC_NETWORK ${JSON.stringify(shown)}; ` +
      `falling back to "${DEFAULT_NETWORK}". ` +
      `Use "testnet", "public" (or "mainnet").`
  );
}

/**
 * Returns the Stellar network configuration derived from the
 * NEXT_PUBLIC_NETWORK environment variable.
 *
 * Call this on both the server and the client — it never accesses `window`.
 */
export function getNetworkConfig(): NetworkConfig {
  const raw = process.env.NEXT_PUBLIC_NETWORK?.trim();

  let network: StellarNetwork = DEFAULT_NETWORK;
  if (raw) {
    const resolved = NETWORK_ALIASES.get(raw.toLowerCase());
    if (resolved) {
      network = resolved;
    } else {
      warnUnrecognised(raw);
    }
  }
  const raw = process.env.NEXT_PUBLIC_NETWORK ?? "testnet";
  const network: StellarNetwork = raw === "public" ? "public" : "testnet";

  return {
    network,
    passphrase: PASSPHRASES[network],
  };
}
