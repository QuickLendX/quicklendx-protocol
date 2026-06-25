import * as dns from "node:dns";
import https from "node:https";
import { isIP } from "node:net";
import type { IncomingMessage } from "node:http";
import type { WebhookEgressPolicy } from "./egressPolicy";
import { validateWebhookUrl, WebhookUrlValidationError } from "./urlValidation";
import { createWebhookSecureLookup } from "./secureLookup";
import {
  areAllDnsResultsPublicForWebhook,
  isBlockedDestinationIP,
} from "./blockedAddress";
import { getCorrelationId } from "../../lib/requestContext";

export class WebhookDeliveryError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "WebhookDeliveryError";
    this.code = code;
  }
}

export type WebhookDeliveryResult = {
  finalUrl: string;
  statusCode: number;
  redirectCount: number;
  responseBodyBytes: number;
};

type SecureAgent = https.Agent;

/**
 * Resolve a hostname to a single IPv4 or IPv6 address and verify that
 * every resolved address is a public unicast IP.  Throws if the
 * hostname has no addresses or any address is non-public.
 */
function resolveHostnameToPinnedIp(hostname: string): Promise<string> {
  return new Promise((resolve, reject) => {
    dns.lookup(hostname, { all: true, verbatim: true }, (err, addresses) => {
      if (err) {
        reject(
          new WebhookDeliveryError(
            "TRANSPORT_ERROR",
            `DNS resolution failed: ${err.message}`,
          ),
        );
        return;
      }
      if (!Array.isArray(addresses) || addresses.length === 0) {
        reject(
          new WebhookDeliveryError(
            "EGRESS_BLOCKED",
            "Webhook target DNS returned no addresses",
          ),
        );
        return;
      }
      if (!areAllDnsResultsPublicForWebhook(addresses)) {
        reject(
          new WebhookDeliveryError(
            "EGRESS_BLOCKED",
            "Webhook target resolved to a non-public address",
          ),
        );
        return;
      }
      resolve(addresses[0].address);
    });
  });
}

/**
 * Create an https.Agent whose custom `lookup` always returns `pinnedIp`
 * and re-validates it via `isBlockedDestinationIP` immediately before
 * every socket connect.  The agent never re-resolves DNS.
 */
function createPinnedAgent(pinnedIp: string): SecureAgent {
  return new https.Agent({
    keepAlive: false,
    maxSockets: 1,
    lookup: (_hostname: string, _opts: any, cb: (err: Error | null, address: string, family: number) => void) => {
      if (isBlockedDestinationIP(pinnedIp)) {
        const err = new Error("WEBHOOK_EGRESS_BLOCKED");
        (err as NodeJS.ErrnoException).code = "EADDRNOTAVAIL";
        cb(err, "", 0);
        return;
      }
      cb(null, pinnedIp, pinnedIp.includes(":") ? 6 : 4);
    },
  });
}

function createWebhookAgent(): SecureAgent {
  return new https.Agent({
    keepAlive: false,
    maxSockets: 1,
    lookup: createWebhookSecureLookup(),
  });
}

export function readBodyWithByteLimit(
  res: IncomingMessage,
  maxBytes: number,
): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    let total = 0;

    res.on("data", (chunk: Buffer) => {
      total += chunk.length;
      if (total > maxBytes) {
        res.destroy();
        reject(
          new WebhookDeliveryError(
            "RESPONSE_TOO_LARGE",
            "Webhook response exceeded configured size limit",
          ),
        );
        return;
      }
      chunks.push(chunk);
    });

    res.on("end", () => resolve(Buffer.concat(chunks)));
    res.on("error", (e) => reject(e));
  });
}

export type OnceResult = {
  statusCode: number;
  headers: IncomingMessage["headers"];
  body: Buffer;
};

export type WebhookDeliveryOptions = {
  /** Injected for tests; production callers should omit this. */
  requestImpl?: (
    target: URL,
    jsonBody: string,
    policy: WebhookEgressPolicy,
    agent: SecureAgent,
  ) => Promise<OnceResult>;
  /** Injected for tests when using a custom `requestImpl`. */
  createAgent?: () => SecureAgent;
};

function requestOnceHttps(
  target: URL,
  jsonBody: string,
  policy: WebhookEgressPolicy,
  agent: SecureAgent,
): Promise<OnceResult> {
  return new Promise((resolve, reject) => {
    const correlationId = getCorrelationId();
    const correlationPrefix = correlationId ? `[${correlationId}] ` : "";
    
    const req = https.request(
      target,
      {
        method: "POST",
        agent,
        headers: {
          "content-type": "application/json",
          "content-length": Buffer.byteLength(jsonBody),
          ...(correlationId ? { "x-request-id": correlationId } : {}),
        },
        timeout: policy.timeoutMs,
        servername: target.hostname,
      },
      (res) => {
        readBodyWithByteLimit(res, policy.maxResponseBytes)
          .then((body) => {
            console.log(`${correlationPrefix}WebhookDelivery: Response ${res.statusCode} from ${target.href}`);
            resolve({
              statusCode: res.statusCode ?? 0,
              headers: res.headers,
              body,
            });
          })
          .catch(reject);
      },
    );

    req.on("timeout", () => {
      req.destroy();
      console.log(`${correlationPrefix}WebhookDelivery: Timeout for ${target.href}`);
      reject(
        new WebhookDeliveryError("TIMEOUT", "Webhook delivery exceeded timeout"),
      );
    });

    req.on("error", (err) => {
      if ((err as NodeJS.ErrnoException).code === "EADDRNOTAVAIL") {
        console.log(`${correlationPrefix}WebhookDelivery: Egress blocked for ${target.href}`);
        reject(
          new WebhookDeliveryError(
            "EGRESS_BLOCKED",
            "Webhook target resolved to a non-public address",
          ),
        );
        return;
      }
      console.log(`${correlationPrefix}WebhookDelivery: Transport error for ${target.href}: ${err instanceof Error ? err.message : "unknown"}`);
      reject(
        new WebhookDeliveryError(
          "TRANSPORT_ERROR",
          err instanceof Error ? err.message : "Webhook transport failed",
        ),
      );
    });

    req.write(jsonBody);
    req.end();
  });
}

/**
 * POST JSON to an HTTPS webhook URL with SSRF-oriented egress controls.
 *
 * Security model: the delivery network and remote TLS endpoint are untrusted.
 * Before every connection the target hostname is resolved to a pinned IP;
 * the IP is re-validated immediately before the socket connects.  A single
 * 3xx redirect is followed if the destination passes all URL, DNS, and
 * IP-blocklist checks.  Beyond one hop any further 3xx is rejected.
 */
export async function deliverWebhookJson(
  rawUrl: string,
  payload: unknown,
  policy: WebhookEgressPolicy,
  options?: WebhookDeliveryOptions,
): Promise<WebhookDeliveryResult> {
  const correlationId = getCorrelationId();
  const correlationPrefix = correlationId ? `[${correlationId}] ` : "";

  const body = JSON.stringify(payload);

  let currentUrl = rawUrl;
  let redirectCount = 0;

  console.log(`${correlationPrefix}WebhookDelivery: Starting delivery to ${rawUrl}`);

  for (;;) {
    let validated: URL;
    try {
      validated = validateWebhookUrl(currentUrl, policy);
    } catch (e) {
      if (e instanceof WebhookUrlValidationError) {
        console.log(`${correlationPrefix}WebhookDelivery: URL validation failed for ${currentUrl}: ${e.message}`);
        throw new WebhookDeliveryError(e.code, e.message);
      }
      console.log(`${correlationPrefix}WebhookDelivery: Unexpected validation error for ${currentUrl}`);
      throw new WebhookDeliveryError(
        "VALIDATION_FAILED",
        e instanceof Error ? e.message : "Webhook URL validation failed",
      );
    }

    const hostname = validated.hostname;

    // Resolve and pin the IP before every request.  For IP literals skip
    // DNS and verify the literal directly (already validated above).
    let agent: SecureAgent;
    try {
      const literalKind = isIP(hostname);

      if (options?.requestImpl) {
        agent = options?.createAgent?.() ?? createWebhookAgent();
      } else if (literalKind) {
        if (isBlockedDestinationIP(hostname)) {
          throw new WebhookDeliveryError(
            "EGRESS_BLOCKED",
            "Webhook target resolved to a non-public address",
          );
        }
        agent = createPinnedAgent(hostname);
      } else {
        const pinnedIp = await resolveHostnameToPinnedIp(hostname);
        agent = createPinnedAgent(pinnedIp);
      }
    } catch (e) {
      // During a redirect follow, convert DNS/block errors to a
      // redirect-specific error so callers can distinguish a bad
      // destination from an initial delivery failure.
      if (redirectCount > 0 && e instanceof WebhookDeliveryError) {
        throw new WebhookDeliveryError(
          "REDIRECT_NOT_ALLOWED",
          `Redirect target validation failed: ${e.message}`,
        );
      }
      throw e;
    }

    const doRequest = options?.requestImpl ?? requestOnceHttps;
    const res = await doRequest(validated, body, policy, agent);

    if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
      if (redirectCount >= policy.maxRedirects) {
        console.log(`${correlationPrefix}WebhookDelivery: Too many redirects for ${validated.href}`);
        throw new WebhookDeliveryError(
          "TOO_MANY_REDIRECTS",
          "Webhook exceeded maximum redirect count",
        );
      }
      redirectCount += 1;
      let redirectTarget: string;
      try {
        redirectTarget = new URL(res.headers.location, validated).href;
      } catch {
        console.log(
          `${correlationPrefix}WebhookDelivery: Invalid redirect Location header from ${validated.href}`,
        );
        throw new WebhookDeliveryError(
          "REDIRECT_NOT_ALLOWED",
          "Redirect target URL is invalid",
        );
      }
      // Validate the redirect destination through the same pipeline as
      // the initial URL — host allow/deny rules, scheme check, IP-blocklist.
      try {
        validateWebhookUrl(redirectTarget, policy);
      } catch (e) {
        if (e instanceof WebhookUrlValidationError) {
          console.log(
            `${correlationPrefix}WebhookDelivery: Redirect target validation failed for ${redirectTarget}: ${e.message}`,
          );
          throw new WebhookDeliveryError(
            e.code,
            `Redirect target validation failed: ${e.message}`,
          );
        }
        console.log(
          `${correlationPrefix}WebhookDelivery: Redirect validation error for ${redirectTarget}`,
        );
        throw new WebhookDeliveryError(
          "VALIDATION_FAILED",
          e instanceof Error ? e.message : "Redirect URL validation failed",
        );
      }
      console.log(`${correlationPrefix}WebhookDelivery: Following redirect to ${redirectTarget}`);
      currentUrl = redirectTarget;
      continue;
    }

    if (res.statusCode >= 300 && res.statusCode < 400) {
      console.log(`${correlationPrefix}WebhookDelivery: Redirect blocked for ${validated.href}`);
      throw new WebhookDeliveryError(
        "REDIRECT_NOT_ALLOWED",
        "Webhook redirects are disallowed",
      );
    }

    console.log(`${correlationPrefix}WebhookDelivery: Completed delivery to ${validated.href} with status ${res.statusCode}`);
    return {
      finalUrl: validated.href,
      statusCode: res.statusCode,
      redirectCount,
      responseBodyBytes: res.body.length,
    };
  }
}
