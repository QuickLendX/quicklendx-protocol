export interface CspEnv {
  NODE_ENV?: string;
  NEXT_PUBLIC_API_URL?: string;
  CSP_ENFORCE?: string;
}

export const CSP_HEADER = "Content-Security-Policy";
export const CSP_REPORT_ONLY_HEADER = "Content-Security-Policy-Report-Only";

const DEFAULT_API_URL = "http://localhost:3001/api/v1";
const STELLAR_CONNECT_SOURCE = "https://*.stellar.org";

export function getApiOrigin(apiUrl?: string): string | null {
  const candidate = apiUrl || DEFAULT_API_URL;

  try {
    const url = new URL(candidate);
    if (url.protocol !== "http:" && url.protocol !== "https:") {
      return null;
    }
    return url.origin;
  } catch {
    return null;
  }
}

function isEnforced(env: CspEnv): boolean {
  const value = env.CSP_ENFORCE?.trim().toLowerCase();
  return value === "1" || value === "true" || value === "yes" || value === "on";
}

export function buildCsp(env: CspEnv = process.env): string {
  const apiOrigin = getApiOrigin(env.NEXT_PUBLIC_API_URL);
  const connectSources = ["'self'"];

  if (apiOrigin) {
    connectSources.push(apiOrigin);
  }
  connectSources.push(STELLAR_CONNECT_SOURCE);

  const scriptSources =
    env.NODE_ENV === "development" ? ["'self'", "'unsafe-eval'"] : ["'self'"];

  return [
    "default-src 'self'",
    `script-src ${scriptSources.join(" ")}`,
    `connect-src ${connectSources.join(" ")}`,
    "img-src 'self' data:",
    "frame-ancestors 'self'",
    "object-src 'none'",
  ].join("; ");
}

export function buildCspHeader(env: CspEnv = process.env): {
  key: string;
  value: string;
} {
  return {
    key: isEnforced(env) ? CSP_HEADER : CSP_REPORT_ONLY_HEADER,
    value: buildCsp(env),
  };
}
