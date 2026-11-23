"use client";

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import type { WalletConnectionState } from "@/app/components/WalletConnect";

export interface WalletContextValue {
  state: WalletConnectionState;
  address?: string;
  connect?: (address: string) => void;
  disconnect?: () => void;
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
}

export const WalletProvider: React.FC<WalletProviderProps> = ({
  children,
  initialState,
  initialAddress,
}) => {
  const [wallet, setWallet] = useState<{
    state: WalletConnectionState;
    address?: string;
  }>(() => {
    if (initialState || initialAddress) {
      return {
        state: initialState ?? (initialAddress ? "CONNECTED" : "NOT_CONNECTED"),
        address: initialAddress,
      };
    }
    return getInitialWalletState();
  });

  useEffect(() => {
    if (initialState || initialAddress) return;
    const detected = getInitialWalletState();
    setWallet((prev) => {
      if (prev.state === detected.state && prev.address === detected.address) {
        return prev;
      }
      return detected;
    });
  }, [initialState, initialAddress]);

  const connect = useCallback((address: string) => {
    setWallet({ state: "CONNECTED", address });
    if (typeof window !== "undefined") {
      try {
        window.localStorage.setItem("quicklendx_wallet_address", address);
      } catch {
        // Ignore storage errors
      }
    }
  }, []);

  const disconnect = useCallback(() => {
    setWallet({ state: "NOT_CONNECTED", address: undefined });
    if (typeof window !== "undefined") {
      try {
        window.localStorage.removeItem("quicklendx_wallet_address");
      } catch {
        // Ignore storage errors
      }
    }
  }, []);

  const value = useMemo<WalletContextValue>(
    () => ({
      state: wallet.state,
      address: wallet.address,
      connect,
      disconnect,
    }),
    [wallet.state, wallet.address, connect, disconnect]
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
  return getInitialWalletState();
}
