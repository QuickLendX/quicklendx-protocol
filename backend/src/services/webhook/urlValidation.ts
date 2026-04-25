import { isIP } from "node:net";
import type { WebhookEgressPolicy } from "./egressPolicy";
import {
  hostnameViolatesAllowPolicy,
  hostnameViolatesDenyPolicy,
} from "./egressPolicy";
import { isBlockedDestinationIP } from "./blockedAddress";

export class WebhookUrlValidationError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "WebhookUrlValidationError";
    this.code = code;
  }
}

/**
 * Validates a webhook target URL before DNS or transport.
 * - HTTPS only
 * - No userinfo (credentials in URL)
 * - Host allow/deny policy
 * - Literal IPs checked without DNS (mitigates obvious SSRF)
 */
export function validateWebhookUrl(
  rawUrl: string,
  policy: WebhookEgressPolicy,
): URL {
  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    throw new WebhookUrlValidationError("INVALID_URL", "Webhook URL is not valid");
  }

  if (url.protocol !== "https:") {
    throw new WebhookUrlValidationError(
      "INVALID_SCHEME",
      "Only https:// webhook targets are allowed",
    );
  }

  if (url.username || url.password) {
    throw new WebhookUrlValidationError(
      "USERINFO_FORBIDDEN",
      "Userinfo in webhook URLs is not allowed",
    );
  }

  if (!url.hostname) {
    throw new WebhookUrlValidationError("MISSING_HOST", "Webhook URL must include a host");
  }

  const hostForPolicy = url.hostname.startsWith("[")
    ? url.hostname.slice(1, -1)
    : url.hostname;

  if (hostnameViolatesDenyPolicy(hostForPolicy, policy)) {
    throw new WebhookUrlValidationError("HOST_DENIED", "Webhook host is not permitted");
  }

  if (hostnameViolatesAllowPolicy(hostForPolicy, policy)) {
    throw new WebhookUrlValidationError("HOST_NOT_ALLOWLISTED", "Webhook host is not allowlisted");
  }

  const literalKind = isIP(hostForPolicy);
  if (literalKind) {
    if (isBlockedDestinationIP(hostForPolicy)) {
      throw new WebhookUrlValidationError(
        "BLOCKED_LITERAL_IP",
        "Webhook target must not use a non-public IP literal",
      );
    }
  }

  return url;
}
