"use client";

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createFreighterAdapter } from "./freighter";
import type {
  WalletAdapter,
  WalletConnectionState,
  WalletErrorCode,
} from "./types";
import { WalletAdapterError } from "./types";

const TESTNET_PASSPHRASE = "Test SDF Network ; September 2015";
const AUTOCONNECT_KEY = "qlx.wallet.autoconnect";
const CONNECT_TIMEOUT_MS = 60_000;

export interface WalletContextValue {
  state: WalletConnectionState;
  address?: string;
  error?: WalletErrorCode;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  signTransaction: (xdr: string) => Promise<string>;
}

declare global {
  interface Window {
    __QUICKLENDX_TEST_WALLET__?: {
      state?: WalletConnectionState;
      address?: string;
    };
  }
}

function getInitialWalletState(): {
  state: WalletConnectionState;
  address?: string;
} {
  if (typeof window !== "undefined") {
    const injected = window.__QUICKLENDX_TEST_WALLET__;
    if (injected) {
      return {
        state:
          injected.state ?? (injected.address ? "CONNECTED" : "NOT_CONNECTED"),
        address: injected.address,
      };
    }
    try {
      const storedAddress =
        window.localStorage.getItem("quicklendx_wallet_address") || undefined;
      if (storedAddress) {
        return { state: "CONNECTED", address: storedAddress };
      }
    } catch {
      // Ignore storage access errors
    }
  }
  return { state: "NOT_CONNECTED", address: undefined };
}

export const WalletContext = createContext<WalletContextValue | undefined>(
  undefined
);

export interface WalletProviderProps {
  children: React.ReactNode;
  initialState?: WalletConnectionState;
  initialAddress?: string;
  adapter?: WalletAdapter;
}

export const WalletProvider: React.FC<WalletProviderProps> = ({
  children,
  initialState,
  initialAddress,
  adapter,
}) => {
  const walletAdapter = useMemo(
    () => adapter ?? createFreighterAdapter(),
    [adapter]
  );
  const [wallet, setWallet] = useState<{
    state: WalletConnectionState;
    address?: string;
    error?: WalletErrorCode;
  }>(() => {
    if (initialState || initialAddress) {
      return {
        state: initialState ?? (initialAddress ? "CONNECTED" : "NOT_CONNECTED"),
        address: initialAddress,
        error: undefined,
      };
    }
    return { state: "NOT_CONNECTED", address: undefined, error: undefined };
  });
  const unwatchRef = useRef<(() => void) | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      unwatchRef.current?.();
    };
  }, []);

  useEffect(() => {
    if (initialState || initialAddress) return;

    let restoreCancelled = false;
    const restore = async () => {
      if (typeof window === "undefined") return;
      let shouldRestore = false;
      try {
        shouldRestore = window.localStorage.getItem(AUTOCONNECT_KEY) === "1";
      } catch {
        return;
      }
      if (!shouldRestore) return;

      try {
        const restored = await walletAdapter.restore();
        if (restoreCancelled || !mountedRef.current || !restored) return;
        const passphrase = await walletAdapter.getNetworkPassphrase();
        if (restoreCancelled || !mountedRef.current) return;
        setWallet({
          state:
            passphrase === TESTNET_PASSPHRASE
              ? "CONNECTED"
              : "WRONG_NETWORK",
          address: restored.address,
          error: undefined,
        });
        unwatchRef.current?.();
        unwatchRef.current = walletAdapter.watch((event) => {
          setWallet({
            state:
              event.networkPassphrase === TESTNET_PASSPHRASE
                ? "CONNECTED"
                : "WRONG_NETWORK",
            address: event.address,
            error: undefined,
          });
        });
      } catch {
        if (!restoreCancelled && mountedRef.current) {
          setWallet({
            state: "NOT_CONNECTED",
            address: undefined,
            error: undefined,
          });
        }
      }
    };
    void restore();

    return () => {
      restoreCancelled = true;
    };
  }, [initialAddress, initialState, walletAdapter]);

  const connect = useCallback(async () => {
    setWallet({ state: "CONNECTING", address: undefined, error: undefined });
    let timeoutId: ReturnType<typeof setTimeout> | undefined;
    try {
      const installed = await walletAdapter.isInstalled();
      if (!installed) {
        throw new WalletAdapterError(
          "not_installed",
          "Freighter is not installed."
        );
      }

      const connection = await Promise.race([
        walletAdapter.connect(),
        new Promise<never>((_, reject) => {
          timeoutId = setTimeout(
            () =>
              reject(
                new WalletAdapterError("timeout", "Connection timed out")
              ),
            CONNECT_TIMEOUT_MS
          );
        }),
      ]);

      const passphrase = await walletAdapter.getNetworkPassphrase();
      if (!mountedRef.current) return;
      const isTestnet = passphrase === TESTNET_PASSPHRASE;
      setWallet({
        state: isTestnet ? "CONNECTED" : "WRONG_NETWORK",
        address: connection.address,
        error: undefined,
      });
      if (isTestnet) {
        try {
          window.localStorage.setItem(AUTOCONNECT_KEY, "1");
        } catch {
          // Ignore storage errors.
        }
      }
      unwatchRef.current?.();
      unwatchRef.current = walletAdapter.watch((event) => {
        setWallet({
          state:
            event.networkPassphrase === TESTNET_PASSPHRASE
              ? "CONNECTED"
              : "WRONG_NETWORK",
          address: event.address,
          error: undefined,
        });
      });
    } catch (error) {
      if (!mountedRef.current) return;
      const code =
        error instanceof WalletAdapterError ? error.code : "unknown";
      setWallet({ state: "ERROR", address: undefined, error: code });
    } finally {
      if (timeoutId !== undefined) clearTimeout(timeoutId);
    }
  }, [walletAdapter]);

  const disconnect = useCallback(async () => {
    setWallet((current) => ({
      ...current,
      state: "DISCONNECTING",
      error: undefined,
    }));
    unwatchRef.current?.();
    unwatchRef.current = null;
    try {
      window.localStorage.removeItem(AUTOCONNECT_KEY);
    } catch {
      // Ignore storage errors.
    }
    setWallet({ state: "NOT_CONNECTED", address: undefined, error: undefined });
  }, []);

  const signTransaction = useCallback(
    async (xdr: string) => {
      if (wallet.state === "WRONG_NETWORK") {
        throw new Error("wrong_network");
      }
      if (wallet.state !== "CONNECTED" || !wallet.address) {
        throw new Error("not_connected");
      }
      return walletAdapter.signTransaction(xdr, {
        networkPassphrase: TESTNET_PASSPHRASE,
        address: wallet.address,
      });
    },
    [wallet.address, wallet.state, walletAdapter]
  );

  const value = useMemo<WalletContextValue>(
    () => ({
      state: wallet.state,
      address: wallet.address,
      error: wallet.error,
      connect,
      disconnect,
      signTransaction,
    }),
    [wallet.state, wallet.address, wallet.error, connect, disconnect, signTransaction]
  );

  return (
    <WalletContext.Provider value={value}>{children}</WalletContext.Provider>
  );
};

export function useWallet(): WalletContextValue {
  const ctx = useContext(WalletContext);
  if (ctx) {
    return ctx;
  }
  // Fallback when used outside WalletProvider (supports test-only window injection)
  const initial = getInitialWalletState();
  return {
    ...initial,
    connect: async () => {},
    disconnect: async () => {},
    signTransaction: async () => {
      throw new Error("not_connected");
    },
  };
}
