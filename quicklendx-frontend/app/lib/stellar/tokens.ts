import { ConfiguredToken } from "../validation";

export const DEFAULT_CONFIGURED_TOKENS: ConfiguredToken[] = [
  {
    code: "USDC",
    address: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
    decimals: 7,
  },
  {
    code: "EURC",
    address: "CBAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
    decimals: 7,
  },
  {
    code: "XLM",
    address: "CDAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
    decimals: 7,
  },
];

export function getConfiguredTokens(): ConfiguredToken[] {
  const envTokens = process.env.NEXT_PUBLIC_TOKENS;
  if (envTokens) {
    try {
      const parsed = JSON.parse(envTokens);
      if (Array.isArray(parsed) && parsed.length > 0) {
        return parsed;
      }
    } catch {
      // fallback to default
    }
  }
  return DEFAULT_CONFIGURED_TOKENS;
}
