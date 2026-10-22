"use client";

import React, { createContext, useContext, useState, useEffect } from "react";

export interface WalletContextType {
  address: string | null;
  setAddress: (address: string | null) => void;
  connect: (address?: string) => void;
  disconnect: () => void;
}

const WalletContext = createContext<WalletContextType>({
  address: null,
  setAddress: () => {},
  connect: () => {},
  disconnect: () => {},
});

export const WalletProvider: React.FC<{
  children: React.ReactNode;
  initialAddress?: string | null;
}> = ({ children, initialAddress = null }) => {
  const [address, setAddress] = useState<string | null>(() => {
    if (initialAddress) return initialAddress;
    if (typeof window !== "undefined") {
      return (
        window.localStorage.getItem("wallet_address") ||
        (window as unknown as { __TEST_WALLET_ADDRESS__?: string })
          .__TEST_WALLET_ADDRESS__ ||
        null
      );
    }
    return null;
  });

  useEffect(() => {
    if (typeof window !== "undefined") {
      const checkTestWallet = () => {
        const testAddr = (
          window as unknown as { __TEST_WALLET_ADDRESS__?: string }
        ).__TEST_WALLET_ADDRESS__;
        if (testAddr && testAddr !== address) {
          setAddress(testAddr);
        }
      };
      checkTestWallet();
      window.addEventListener("storage", checkTestWallet);
      return () => window.removeEventListener("storage", checkTestWallet);
    }
  }, [address]);

  const connect = (newAddress?: string) => {
    const addr =
      newAddress || "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ";
    setAddress(addr);
    if (typeof window !== "undefined") {
      window.localStorage.setItem("wallet_address", addr);
    }
  };

  const disconnect = () => {
    setAddress(null);
    if (typeof window !== "undefined") {
      window.localStorage.removeItem("wallet_address");
      delete (window as unknown as { __TEST_WALLET_ADDRESS__?: string })
        .__TEST_WALLET_ADDRESS__;
    }
  };

  return (
    <WalletContext.Provider
      value={{ address, setAddress, connect, disconnect }}
    >
      {children}
    </WalletContext.Provider>
  );
};

export function useWallet(): WalletContextType {
  return useContext(WalletContext);
}
