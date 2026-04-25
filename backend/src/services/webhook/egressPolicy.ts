export type WebhookEgressPolicy = {
  /** If non-empty, hostname must match at least one allow rule. */
  hostAllowRules: string[];
  /** Hostnames blocked in addition to built-in defaults. */
  hostDenyRules: string[];
  maxRedirects: number;
  timeoutMs: number;
  maxResponseBytes: number;
};

const DEFAULT_MAX_REDIRECTS = 3;
const DEFAULT_TIMEOUT_MS = 10_000;
const DEFAULT_MAX_RESPONSE_BYTES = 65_536;

const BUILT_IN_DENY_HOSTS = new Set(
  ["localhost", "metadata.google.internal", "metadata.google"].map((h) =>
    h.toLowerCase(),
  ),
);

function parseHostList(raw: string | undefined): string[] {
  if (!raw || !raw.trim()) return [];
  return raw
    .split(",")
    .map((s) => s.trim().toLowerCase())
    .filter(Boolean);
}

function parsePositiveInt(raw: string | undefined, fallback: number): number {
  if (raw === undefined || raw === "") return fallback;
  const n = Number.parseInt(raw, 10);
  if (!Number.isFinite(n) || n < 0) return fallback;
  return n;
}

export function loadWebhookEgressPolicyFromEnv(
  env: NodeJS.ProcessEnv = process.env,
): WebhookEgressPolicy {
  const hostAllowRules = parseHostList(env.WEBHOOK_HOST_ALLOWLIST);
  const hostDenyRules = parseHostList(env.WEBHOOK_HOST_DENYLIST);

  return {
    hostAllowRules,
    hostDenyRules,
    maxRedirects: parsePositiveInt(env.WEBHOOK_MAX_REDIRECTS, DEFAULT_MAX_REDIRECTS),
    timeoutMs: parsePositiveInt(env.WEBHOOK_TIMEOUT_MS, DEFAULT_TIMEOUT_MS),
    maxResponseBytes: parsePositiveInt(
      env.WEBHOOK_MAX_RESPONSE_BYTES,
      DEFAULT_MAX_RESPONSE_BYTES,
    ),
  };
}

export function hostMatchesAllowRule(hostname: string, rule: string): boolean {
  const host = hostname.toLowerCase();
  const r = rule.toLowerCase();
  if (r.startsWith("*.")) {
    const suffix = r.slice(2);
    if (!suffix) return false;
    if (host === suffix) return false;
    return host.endsWith("." + suffix);
  }
  return host === r;
}

export function hostnameViolatesDenyPolicy(
  hostname: string,
  policy: WebhookEgressPolicy,
): boolean {
  const host = hostname.toLowerCase();

  if (host.endsWith(".local")) return true;

  for (const d of policy.hostDenyRules) {
    if (host === d || host.endsWith("." + d)) return true;
  }

  for (const d of BUILT_IN_DENY_HOSTS) {
    if (host === d || host.endsWith("." + d)) return true;
  }

  return false;
}

export function hostnameViolatesAllowPolicy(
  hostname: string,
  policy: WebhookEgressPolicy,
): boolean {
  if (policy.hostAllowRules.length === 0) return false;
  const host = hostname.toLowerCase();
  return !policy.hostAllowRules.some((rule) => hostMatchesAllowRule(host, rule));
}
