import https from "node:https";
import type { IncomingMessage } from "node:http";
import type { WebhookEgressPolicy } from "./egressPolicy";
import { validateWebhookUrl, WebhookUrlValidationError } from "./urlValidation";
import { createWebhookSecureLookup } from "./secureLookup";

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
    const req = https.request(
      target,
      {
        method: "POST",
        agent,
        headers: {
          "content-type": "application/json",
          "content-length": Buffer.byteLength(jsonBody),
        },
        timeout: policy.timeoutMs,
      },
      (res) => {
        readBodyWithByteLimit(res, policy.maxResponseBytes)
          .then((body) => {
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
      reject(
        new WebhookDeliveryError("TIMEOUT", "Webhook delivery exceeded timeout"),
      );
    });

    req.on("error", (err) => {
      if ((err as NodeJS.ErrnoException).code === "EADDRNOTAVAIL") {
        reject(
          new WebhookDeliveryError(
            "EGRESS_BLOCKED",
            "Webhook target resolved to a non-public address",
          ),
        );
        return;
      }
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
 * Only https targets are followed; each hop re-validates URL policy and DNS
 * resolves to public addresses only (mitigates DNS rebinding across redirects).
 */
export async function deliverWebhookJson(
  rawUrl: string,
  payload: unknown,
  policy: WebhookEgressPolicy,
  options?: WebhookDeliveryOptions,
): Promise<WebhookDeliveryResult> {
  const body = JSON.stringify(payload);
  const agent = options?.createAgent?.() ?? createWebhookAgent();
  const doRequest = options?.requestImpl ?? requestOnceHttps;

  let currentUrl = rawUrl;
  let redirectCount = 0;

  for (;;) {
    let validated: URL;
    try {
      validated = validateWebhookUrl(currentUrl, policy);
    } catch (e) {
      if (e instanceof WebhookUrlValidationError) {
        throw new WebhookDeliveryError(e.code, e.message);
      }
      throw new WebhookDeliveryError(
        "VALIDATION_FAILED",
        e instanceof Error ? e.message : "Webhook URL validation failed",
      );
    }

    const res = await doRequest(validated, body, policy, agent);

    if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
      if (redirectCount >= policy.maxRedirects) {
        throw new WebhookDeliveryError(
          "TOO_MANY_REDIRECTS",
          "Webhook exceeded maximum redirect count",
        );
      }
      redirectCount += 1;
      currentUrl = new URL(res.headers.location, validated).href;
      continue;
    }

    return {
      finalUrl: validated.href,
      statusCode: res.statusCode,
      redirectCount,
      responseBodyBytes: res.body.length,
    };
  }
}
