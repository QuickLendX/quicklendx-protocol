import { deliverWebhookJson, WebhookDeliveryError } from "./delivery";
import type { WebhookEgressPolicy } from "./egressPolicy";
import type { WebhookDeliveryOptions } from "./delivery";

export interface RetryPolicy {
  maxAttempts?: number;
  initialDelayMs?: number;
  maxDelayMs?: number;
}

export interface DeliveryAttemptResult {
  success: boolean;
  statusCode?: number;
  attemptCount: number;
  deadLettered: boolean;
  finalError?: string;
}

const DEFAULT_RETRY_POLICY: Required<RetryPolicy> = {
  maxAttempts: 5,
  initialDelayMs: 500,
  maxDelayMs: 30000,
};

function calculateBackoff(attempt: number, policy: Required<RetryPolicy>): number {
  const base = policy.initialDelayMs * Math.pow(2, attempt);
  const jitter = Math.random() * base;
  return Math.min(base + jitter, policy.maxDelayMs);
}

function isPermanentFailure(statusCode?: number): boolean {
  if (!statusCode) return false;
  return statusCode >= 400 && statusCode < 500 && statusCode !== 429;
}

function isRetryableStatusCode(statusCode: number): boolean {
  return statusCode >= 500 || statusCode === 429;
}

function isRetryableError(error: unknown): boolean {
  if (error instanceof WebhookDeliveryError) {
    return ["TIMEOUT", "TRANSPORT_ERROR", "EGRESS_BLOCKED"].includes(error.code);
  }
  return true;
}

export async function deliverWithRetry(
  rawUrl: string,
  payload: unknown,
  policy: WebhookEgressPolicy,
  retryPolicy: RetryPolicy = {},
  options?: WebhookDeliveryOptions,
): Promise<DeliveryAttemptResult> {
  const resolved: Required<RetryPolicy> = {
    ...DEFAULT_RETRY_POLICY,
    ...retryPolicy,
  };

  let attemptCount = 0;

  for (;;) {
    attemptCount++;

    try {
      const result = await deliverWebhookJson(rawUrl, payload, policy, options);

      if (isPermanentFailure(result.statusCode)) {
        console.log(
          `WebhookRetry: Permanent failure ${result.statusCode} for ${rawUrl} — no retry`
        );
        return {
          success: false,
          statusCode: result.statusCode,
          attemptCount,
          deadLettered: true,
          finalError: `Permanent HTTP ${result.statusCode}`,
        };
      }

      if (isRetryableStatusCode(result.statusCode)) {
        const exhausted = attemptCount >= resolved.maxAttempts;
        if (exhausted) {
          return {
            success: false,
            statusCode: result.statusCode,
            attemptCount,
            deadLettered: true,
            finalError: `Max attempts reached with HTTP ${result.statusCode}`,
          };
        }
        const delay = calculateBackoff(attemptCount - 1, resolved);
        console.log(
          `WebhookRetry: Attempt ${attemptCount} got ${result.statusCode} for ${rawUrl} — retrying in ${Math.round(delay)}ms`
        );
        await new Promise((resolve) => setTimeout(resolve, delay));
        continue;
      }

      return {
        success: true,
        statusCode: result.statusCode,
        attemptCount,
        deadLettered: false,
      };

    } catch (error) {
      const isRetryable = isRetryableError(error);
      const exhausted = attemptCount >= resolved.maxAttempts;

      if (!isRetryable || exhausted) {
        const msg = error instanceof Error ? error.message : "Unknown error";
        console.log(
          `WebhookRetry: ${exhausted ? "Max attempts reached" : "Non-retryable error"} for ${rawUrl} — dead-lettering. Error: ${msg}`
        );
        return {
          success: false,
          attemptCount,
          deadLettered: true,
          finalError: msg,
        };
      }

      const delay = calculateBackoff(attemptCount - 1, resolved);
      console.log(
        `WebhookRetry: Attempt ${attemptCount} failed for ${rawUrl} — retrying in ${Math.round(delay)}ms`
      );
      await new Promise((resolve) => setTimeout(resolve, delay));
    }
  }
}
