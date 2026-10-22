"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { WalletConnect } from "@/app/components/WalletConnect";

export interface Wallet {
  state: "connected" | "disconnected";
  address?: string;
}

interface WalletContextValue extends Wallet {
  connect: (address: string) => void;
}

declare global {
  interface Window {
    __QLX_TEST_WALLET__?: string;
  }
}

const WalletContext = createContext<WalletContextValue | null>(null);

export function WalletProvider({ children }: { children: ReactNode }) {
  const [address, setAddress] = useState<string>();

  useEffect(() => {
    if (process.env.NEXT_PUBLIC_E2E === "1" && window.__QLX_TEST_WALLET__) {
      setAddress(window.__QLX_TEST_WALLET__);
    }
  }, []);

  const connect = useCallback((connectedAddress: string) => {
    setAddress(connectedAddress);
  }, []);
  const value = useMemo<WalletContextValue>(
    () => ({
      state: address ? "connected" : "disconnected",
      ...(address ? { address } : {}),
      connect,
    }),
    [address, connect]
  );

  return (
    <WalletContext.Provider value={value}>{children}</WalletContext.Provider>
  );
}

export function useWallet(): Wallet {
  const wallet = useContext(WalletContext);
  if (!wallet) {
    throw new Error("useWallet must be used within WalletProvider");
  }
  return { state: wallet.state, address: wallet.address };
}

export function WalletConnectControl() {
  const wallet = useContext(WalletContext);
  if (!wallet) {
    throw new Error("WalletConnectControl must be used within WalletProvider");
  }
  return (
    <WalletConnect
      onConnect={wallet.connect}
      onDisconnect={() => wallet.connect("")}
    />
  );
}
