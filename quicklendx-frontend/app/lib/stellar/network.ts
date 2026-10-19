export interface NetworkConfig {
  network: "testnet" | "public";
  rpcUrl: string;
  passphrase: string;
  contractId: string;
}

export function getNetworkConfig(): NetworkConfig {
  const network = (process.env.NEXT_PUBLIC_STELLAR_NETWORK || "testnet") as
    | "testnet"
    | "public";

  const defaultRpcUrl =
    network === "public"
      ? "https://mainnet.sorobanrpc.com"
      : "https://soroban-testnet.stellar.org";

  const defaultPassphrase =
    network === "public"
      ? "Public Global Stellar Network ; September 2015"
      : "Test SDF Network ; September 2015";

  return {
    network,
    rpcUrl:
      process.env.NEXT_PUBLIC_STELLAR_RPC_URL ||
      process.env.STELLAR_RPC_URL ||
      defaultRpcUrl,
    passphrase:
      process.env.NEXT_PUBLIC_STELLAR_NETWORK_PASSPHRASE ||
      process.env.STELLAR_NETWORK_PASSPHRASE ||
      defaultPassphrase,
    contractId:
      process.env.NEXT_PUBLIC_CONTRACT_ID ||
      process.env.CONTRACT_ID ||
      "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
  };
}
