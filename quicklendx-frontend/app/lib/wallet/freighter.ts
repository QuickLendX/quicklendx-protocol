/**
 * freighter.ts — WalletAdapter implementation for @stellar/freighter-api.
 *
 * SSR-safe: @stellar/freighter-api is only imported inside async functions
 * that run in the browser (never at module evaluation time), so Next.js
 * server-side rendering never touches the `window`-dependent SDK.
 *
 * Freighter API v4+ returns { address, error } / { isConnected, error }
 * objects rather than throwing.
 *
 * Spec: docs/ux/wallet-connect.md §1, §3.2–§3.6
 * Docs: https://docs.freighter.app/docs/guide/usingFreighterWebApp
 */

import type { WalletAdapter } from "./types";
import { WalletAdapterError } from "./types";

const CONNECT_TIMEOUT_MS = 60_000;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/**
 * Lazy-loads @stellar/freighter-api.
 * Called only from within async browser-side code.
 */
async function getFreighter() {
  // Dynamic import prevents the module from being bundled into SSR chunks.
  const mod = await import("@stellar/freighter-api");
  return mod;
}

/** Maps a Freighter error string to a WalletErrorCode. */
function mapError(
  errorMsg: string | undefined
): "not_installed" | "rejected" | "unknown" {
  if (!errorMsg) return "unknown";
  const lower = errorMsg.toLowerCase();
  if (
    lower.includes("not installed") ||
    lower.includes("not connected") ||
    lower.includes("could not connect")
  ) {
    return "not_installed";
  }
  if (
    lower.includes("declined") ||
    lower.includes("rejected") ||
    lower.includes("user declined") ||
    lower.includes("user rejected")
  ) {
    return "rejected";
  }
  return "unknown";
}

/** Wraps a promise with a timeout. */
function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new WalletAdapterError("timeout", "Connection timed out")),
      ms
    );
    promise.then(
      (v) => { clearTimeout(timer); resolve(v); },
      (e) => { clearTimeout(timer); reject(e); }
    );
  });
}

// ---------------------------------------------------------------------------
// FreighterAdapter
// ---------------------------------------------------------------------------

/**
 * Creates a WalletAdapter backed by @stellar/freighter-api.
 *
 * Usage:
 *   import { createFreighterAdapter } from "@/app/lib/wallet/freighter";
 *   const adapter = createFreighterAdapter();
 */
export function createFreighterAdapter(): WalletAdapter {
  return {
    async isInstalled(): Promise<boolean> {
      try {
        const { isConnected } = await getFreighter();
        const result = await isConnected();
        // v4 API returns { isConnected: boolean; error?: string }
        if (typeof result === "object" && result !== null) {
          return (result as { isConnected: boolean }).isConnected === true;
        }
        return Boolean(result);
      } catch {
        return false;
      }
    },

    async connect(): Promise<{ address: string }> {
      const connectPromise = (async () => {
        const freighter = await getFreighter();

        // Check installation first
        const connResult = await freighter.isConnected();
        const isConn =
          typeof connResult === "object" && connResult !== null
            ? (connResult as { isConnected: boolean }).isConnected
            : Boolean(connResult);

        if (!isConn) {
          throw new WalletAdapterError(
            "not_installed",
            "Freighter is not installed."
          );
        }

        // Request access (prompts the user)
        const accessResult = await freighter.requestAccess();
        if (accessResult?.error) {
          const code = mapError(accessResult.error);
          throw new WalletAdapterError(
            code,
            accessResult.error
          );
        }

        // Get the address
        const addrResult = await freighter.getAddress();
        if (addrResult?.error) {
          throw new WalletAdapterError(
            mapError(addrResult.error),
            addrResult.error
          );
        }

        const address = (addrResult as { address: string }).address;
        if (!address) {
          throw new WalletAdapterError("unknown", "No address returned.");
        }

        return { address };
      })();

      return withTimeout(connectPromise, CONNECT_TIMEOUT_MS);
    },

    async restore(): Promise<{ address: string } | null> {
      try {
        const freighter = await getFreighter();

        // isAllowed returns true only when the user has previously granted access
        const allowedResult = await freighter.isAllowed();
        const isAllowed =
          typeof allowedResult === "object" && allowedResult !== null
            ? (allowedResult as { isAllowed: boolean }).isAllowed
            : Boolean(allowedResult);

        if (!isAllowed) return null;

        const addrResult = await freighter.getAddress();
        if (addrResult?.error || !(addrResult as { address?: string }).address) {
          return null;
        }

        return { address: (addrResult as { address: string }).address };
      } catch {
        return null;
      }
    },

    async getNetworkPassphrase(): Promise<string> {
      const freighter = await getFreighter();
      const details = await freighter.getNetworkDetails();
      if ((details as { error?: string }).error) {
        throw new WalletAdapterError(
          "unknown",
          (details as { error: string }).error
        );
      }
      return (details as { networkPassphrase: string }).networkPassphrase;
    },

    watch(
      cb: (event: { address: string; networkPassphrase: string }) => void
    ): () => void {
      // WatchWalletChanges polls for account/network changes.
      // We start it lazily because it touches window.
      let stopFn: (() => void) | null = null;
      let cancelled = false;

      getFreighter().then((freighter) => {
        if (cancelled) return;
        if (typeof freighter.WatchWalletChanges !== "function") return;

        const watcher = new freighter.WatchWalletChanges(3_000);
        watcher.watch((evt: { address: string; networkPassphrase: string }) => {
          cb(evt);
        });

        stopFn = () => {
          try { watcher.stop(); } catch { /* ignore */ }
        };
      }).catch(() => { /* module load failure is silent */ });

      return () => {
        cancelled = true;
        stopFn?.();
      };
    },

    async signTransaction(
      xdr: string,
      opts: { networkPassphrase: string; address: string }
    ): Promise<string> {
      const freighter = await getFreighter();
      const result = await freighter.signTransaction(xdr, {
        networkPassphrase: opts.networkPassphrase,
        address: opts.address,
      });

      if ((result as { error?: string }).error) {
        const code = mapError((result as { error: string }).error);
        throw new WalletAdapterError(code, (result as { error: string }).error);
      }

      return (result as { signedTxXdr: string }).signedTxXdr ?? (result as unknown as string);
    },
  };
}
