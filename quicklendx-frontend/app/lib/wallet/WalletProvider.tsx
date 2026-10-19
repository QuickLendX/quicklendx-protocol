import React, {
  createContext,
  useContext,
  useState,
  useMemo,
  useCallback,
} from "react";

export type WalletStatus =
  | "NOT_CONNECTED"
  | "CONNECTING"
  | "CONNECTED"
  | "WRONG_NETWORK"
  | "ERROR";

export interface WalletContextType {
  status: WalletStatus;
  address: string | null;
  network: string | null;
  expectedNetwork: string;
  signTransaction: (xdr: string) => Promise<string>;
  connect: () => Promise<void>;
  disconnect: () => void;
  setStatus?: (status: WalletStatus) => void;
  setAddress?: (address: string | null) => void;
  setNetwork?: (network: string | null) => void;
}

const defaultContext: WalletContextType = {
  status: "NOT_CONNECTED",
  address: null,
  network: null,
  expectedNetwork: "testnet",
  signTransaction: async () => {
    throw new Error("Wallet not connected");
  },
  connect: async () => {},
  disconnect: () => {},
};

const WalletContext = createContext<WalletContextType>(defaultContext);

export interface WalletProviderProps {
  children: React.ReactNode;
  initialAddress?: string | null;
  initialStatus?: WalletStatus;
  initialNetwork?: string | null;
  expectedNetwork?: string;
  signTransaction?: (xdr: string) => Promise<string>;
}

export const WalletProvider: React.FC<WalletProviderProps> = ({
  children,
  initialAddress = null,
  initialStatus = "NOT_CONNECTED",
  initialNetwork = "testnet",
  expectedNetwork = "testnet",
  signTransaction: customSignTransaction,
}) => {
  const [status, setStatus] = useState<WalletStatus>(initialStatus);
  const [address, setAddress] = useState<string | null>(initialAddress);
  const [network, setNetwork] = useState<string | null>(initialNetwork);

  const signTransaction = useMemo(() => {
    if (customSignTransaction) {
      return customSignTransaction;
    }
    return async (xdr: string) => {
      if (status !== "CONNECTED" || !address) {
        throw new Error("Wallet not connected");
      }
      return xdr;
    };
  }, [customSignTransaction, status, address]);

  const connect = useCallback(async () => {
    setStatus("CONNECTED");
    setAddress(
      initialAddress ||
        "GARFWKJWKJG7DFIOVV7CZRPAQCFPEU5RQLHJ6UL33GDU6N7JIUKAMPP6"
    );
    setNetwork(expectedNetwork);
  }, [expectedNetwork, initialAddress]);

  const disconnect = useCallback(() => {
    setStatus("NOT_CONNECTED");
    setAddress(null);
  }, []);

  const value = useMemo(
    () => ({
      status,
      address,
      network,
      expectedNetwork,
      signTransaction,
      connect,
      disconnect,
      setStatus,
      setAddress,
      setNetwork,
    }),
    [
      status,
      address,
      network,
      expectedNetwork,
      signTransaction,
      connect,
      disconnect,
    ]
  );

  return (
    <WalletContext.Provider value={value}>{children}</WalletContext.Provider>
  );
};

export function useWallet(): WalletContextType {
  return useContext(WalletContext);
}
