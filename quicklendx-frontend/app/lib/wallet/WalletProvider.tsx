"use client";

/**
 * WalletProvider.tsx
 *
 * React context + provider that owns the single wallet session for the app.
 * All pages and components read and drive wallet state through useWallet().
 *
 * Features:
 * - Network check: after connect, restore and every watch event, compare the
 *   passphrase with getNetworkConfig().passphrase. Mismatch → WRONG_NETWORK.
 * - Silent reconnect: if localStorage["qlx.wallet.autoconnect"] === "1",
 *   calls adapter.restore() on mount. Never persists the address.
 * - Account/network watch: a WatchWalletChanges subscription updates state
 *   automatically. Switching to the expected network from WRONG_NETWORK moves
 *   to CONNECTED without a page reload.
 * - signTransaction rejects in WRONG_NETWORK state.
 */

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { getNetworkConfig } from "../stellar/network";
import { createFreighterAdapter } from "./freighter";
import {
  WalletAdapter,
  WalletAdapterError,
  WalletConnectionState,
  WalletErrorCode,
} from "./types";
import {
  trackWalletConnectAttempt,
  trackWalletConnected,
  trackWalletConnectFailed,
  trackWalletDisconnected,
} from "../wallet-telemetry";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const AUTOCONNECT_KEY = "qlx.wallet.autoconnect";
const NETWORK_CFG = getNetworkConfig();

// ---------------------------------------------------------------------------
// Context shape
// ---------------------------------------------------------------------------

export interface WalletContextValue {
  state: WalletConnectionState;
  address?: string;
  error?: WalletErrorCode;
  expectedNetwork: "testnet" | "public";
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  signTransaction: (
    xdr: string,
    opts?: { networkPassphrase?: string; address?: string }
  ) => Promise<string>;
}

const WalletContext = createContext<WalletContextValue | null>(null);

// ---------------------------------------------------------------------------
// Provider props
// ---------------------------------------------------------------------------

export interface WalletProviderProps {
  children: React.ReactNode;
  /** Override the adapter (used in tests to inject a fake). */
  adapter?: WalletAdapter;
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

export function WalletProvider({ children, adapter: adapterProp }: WalletProviderProps) {
  const adapter = useRef<WalletAdapter>(adapterProp ?? createFreighterAdapter());

  const [state, setState] = useState<WalletConnectionState>("NOT_CONNECTED");
  const [address, setAddress] = useState<string | undefined>(undefined);
  const [error, setError] = useState<WalletErrorCode | undefined>(undefined);

  // Keep track of the unsubscribe fn from adapter.watch()
  const unwatchRef = useRef<(() => void) | null>(null);

  // --------------------------------------------------------------------------
  // Shared post-connect logic: network check + telemetry
  // --------------------------------------------------------------------------
  const applyConnected = useCallback((addr: string, passphrase: string) => {
    if (passphrase !== NETWORK_CFG.passphrase) {
      setState("WRONG_NETWORK");
      setAddress(addr);
      setError(undefined);
    } else {
      setState("CONNECTED");
      setAddress(addr);
      setError(undefined);
      trackWalletConnected({
        address: addr,
        walletType: "freighter",
        network: NETWORK_CFG.network,
      });
    }
  }, []);

  // --------------------------------------------------------------------------
  // Watch subscription — starts after first successful connection
  // --------------------------------------------------------------------------
  const startWatch = useCallback((currentAddr: string) => {
    // Stop any previous subscription
    unwatchRef.current?.();
    unwatchRef.current = adapter.current.watch((evt) => {
      applyConnected(evt.address, evt.networkPassphrase);
      // If address changed, emit telemetry
      if (evt.address !== currentAddr) {
        trackWalletConnected({
          address: evt.address,
          walletType: "freighter",
          network: NETWORK_CFG.network,
        });
      }
    });
  }, [applyConnected]);

  // --------------------------------------------------------------------------
  // Silent restore on mount
  // --------------------------------------------------------------------------
  useEffect(() => {
    let cancelled = false;

    async function tryRestore() {
      if (typeof window === "undefined") return;
      if (localStorage.getItem(AUTOCONNECT_KEY) !== "1") return;

      try {
        const result = await adapter.current.restore();
        if (cancelled || !result) return;

        const passphrase = await adapter.current.getNetworkPassphrase();
        if (cancelled) return;

        applyConnected(result.address, passphrase);
        startWatch(result.address);
      } catch {
        // Silent restore failures are not surfaced to the user
      }
    }

    tryRestore();
    return () => { cancelled = true; };
  }, [applyConnected, startWatch]);

  // --------------------------------------------------------------------------
  // connect()
  // --------------------------------------------------------------------------
  const connect = useCallback(async () => {
    setState("CONNECTING");
    setError(undefined);
    trackWalletConnectAttempt({
      walletType: "freighter",
      network: NETWORK_CFG.network,
    });

    try {
      const result = await adapter.current.connect();
      const passphrase = await adapter.current.getNetworkPassphrase();

      applyConnected(result.address, passphrase);
      localStorage.setItem(AUTOCONNECT_KEY, "1");
      startWatch(result.address);
    } catch (err) {
      const code: WalletErrorCode =
        err instanceof WalletAdapterError ? err.code : "unknown";
      const msg = err instanceof Error ? err.message : "Unknown error";
      setState("ERROR");
      setError(code);
      trackWalletConnectFailed({
        error: msg,
        walletType: "freighter",
        network: NETWORK_CFG.network,
      });
    }
  }, [applyConnected, startWatch]);

  // --------------------------------------------------------------------------
  // disconnect()
  // --------------------------------------------------------------------------
  const disconnect = useCallback(async () => {
    setState("DISCONNECTING");
    const prevAddress = address ?? "";
    trackWalletDisconnected({
      address: prevAddress,
      network: NETWORK_CFG.network,
    });

    // Stop watching
    unwatchRef.current?.();
    unwatchRef.current = null;

    // Remove the auto-connect flag (never persists address)
    localStorage.removeItem(AUTOCONNECT_KEY);

    setAddress(undefined);
    setError(undefined);
    setState("NOT_CONNECTED");
  }, [address]);

  // --------------------------------------------------------------------------
  // signTransaction()
  // --------------------------------------------------------------------------
  const signTransaction = useCallback(
    async (
      xdr: string,
      opts?: { networkPassphrase?: string; address?: string }
    ): Promise<string> => {
      if (state === "WRONG_NETWORK") {
        throw new Error("wrong_network");
      }
      if (state !== "CONNECTED" || !address) {
        throw new Error("not_connected");
      }
      return adapter.current.signTransaction(xdr, {
        networkPassphrase: opts?.networkPassphrase ?? NETWORK_CFG.passphrase,
        address: opts?.address ?? address,
      });
    },
    [state, address]
  );

  // Cleanup watch on unmount
  useEffect(() => {
    return () => { unwatchRef.current?.(); };
  }, []);

  const value: WalletContextValue = {
    state,
    address,
    error,
    expectedNetwork: NETWORK_CFG.network,
    connect,
    disconnect,
    signTransaction,
  };

  return (
    <WalletContext.Provider value={value}>
      {children}
    </WalletContext.Provider>
  );
}

// ---------------------------------------------------------------------------
// useWallet hook
// ---------------------------------------------------------------------------

/**
 * Access the wallet context from any client component.
 *
 * @example
 *   const { state, address, connect, disconnect } = useWallet();
 */
export function useWallet(): WalletContextValue {
  const ctx = useContext(WalletContext);
  if (!ctx) {
    throw new Error("useWallet must be used inside <WalletProvider>");
  }
  return ctx;
}
