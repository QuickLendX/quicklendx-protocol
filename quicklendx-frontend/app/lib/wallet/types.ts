/**
 * types.ts — shared wallet types.
 *
 * WalletAdapter is the interface every wallet implementation must satisfy.
 * WalletErrorCode is a discriminated union of all error states the provider
 * can surface.
 */

export type WalletErrorCode =
  | "not_installed"   // wallet extension is not present
  | "rejected"        // user declined the connection prompt
  | "timeout"         // connect() took longer than CONNECT_TIMEOUT_MS
  | "unknown";        // any other error

/** Wallet connection states surfaced through useWallet(). */
export type WalletConnectionState =
  | "NOT_CONNECTED"
  | "CONNECTING"
  | "CONNECTED"
  | "WRONG_NETWORK"
  | "ERROR"
  | "DISCONNECTING";

/**
 * Canonical interface every wallet adapter must implement.
 * The Freighter adapter is in freighter.ts.
 * Implement this interface to add a second wallet (e.g. Albedo, xBull).
 */
export interface WalletAdapter {
  /**
   * Returns true when the wallet extension / app is installed and available.
   * Must never throw; returns false on any error.
   */
  isInstalled(): Promise<boolean>;

  /**
   * Prompts the user to grant access and returns their public key.
   * Rejects with a WalletAdapterError when the user declines or times out.
   */
  connect(): Promise<{ address: string }>;

  /**
   * Silent restore: returns the address if the user previously granted access,
   * null if not. Never prompts the user.
   */
  restore(): Promise<{ address: string } | null>;

  /** Returns the network passphrase currently active in the wallet. */
  getNetworkPassphrase(): Promise<string>;

  /**
   * Registers a callback for account/network changes.
   * Returns an unsubscribe function.
   */
  watch(
    cb: (event: { address: string; networkPassphrase: string }) => void
  ): () => void;

  /**
   * Signs an XDR transaction envelope and returns the signed XDR.
   * Throws WalletAdapterError on rejection or wrong network.
   */
  signTransaction(
    xdr: string,
    opts: { networkPassphrase: string; address: string }
  ): Promise<string>;
}

/** Typed error thrown by wallet adapters. */
export class WalletAdapterError extends Error {
  constructor(
    public readonly code: WalletErrorCode,
    message: string
  ) {
    super(message);
    this.name = "WalletAdapterError";
  }
}
