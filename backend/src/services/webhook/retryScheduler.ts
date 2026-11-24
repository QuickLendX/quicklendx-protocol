import { deliverWebhookJson, WebhookDeliveryError } from "./delivery";
import type { WebhookEgressPolicy } from "./egressPolicy";
import type { WebhookDeliveryOptions } from "./delivery";
import { webhookDeliveryRepo } from "../webhookDeliveryRepo";

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
  deadLetterId?: string;
}

const DEFAULT_RETRY_POLICY: Required<RetryPolicy> = {
  maxAttempts: 5,
  initialDelayMs: 500,
  maxDelayMs: 30000,
};

function persistDeadLetter(
  rawUrl: string,
  payload: unknown,
  attemptCount: number,
  maxAttempts: number,
  finalError: string,
  options?: WebhookDeliveryOptions
): string | undefined {
  try {
    const eventType =
      options?.eventType ??
      (payload && typeof payload === "object" && "event" in payload
        ? String((payload as any).event)
        : payload && typeof payload === "object" && "eventType" in payload
        ? String((payload as any).eventType)
        : payload && typeof payload === "object" && "type" in payload
        ? String((payload as any).type)
        : "webhook.delivery");

    const subscriberId =
      options?.subscriberId ??
      (payload && typeof payload === "object" && "subscriberId" in payload
        ? String((payload as any).subscriberId)
        : null);

    const deliveryId =
      options?.deliveryId ??
      (payload && typeof payload === "object" && "deliveryId" in payload
        ? String((payload as any).deliveryId)
        : payload && typeof payload === "object" && "id" in payload
        ? String((payload as any).id)
        : null);

    const record = webhookDeliveryRepo.recordDeadLetter({
      targetUrl: rawUrl,
      payload,
      attemptCount,
      maxAttempts,
      lastError: finalError,
      deliveryId,
      eventType,
      subscriberId,
    });
    return record.id;
  } catch (err) {
    console.error("Failed to persist exhausted delivery to webhook_dead_letters:", err);
    return undefined;
  }
}

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
        const finalError = `Permanent HTTP ${result.statusCode}`;
        const deadLetterId = persistDeadLetter(
          rawUrl,
          payload,
          attemptCount,
          resolved.maxAttempts,
          finalError,
          options
        );
        return {
          success: false,
          statusCode: result.statusCode,
          attemptCount,
          deadLettered: true,
          finalError,
          deadLetterId,
        };
      }

      if (isRetryableStatusCode(result.statusCode)) {
        const exhausted = attemptCount >= resolved.maxAttempts;
        if (exhausted) {
          const finalError = `Max attempts reached with HTTP ${result.statusCode}`;
          const deadLetterId = persistDeadLetter(
            rawUrl,
            payload,
            attemptCount,
            resolved.maxAttempts,
            finalError,
            options
          );
          return {
            success: false,
            statusCode: result.statusCode,
            attemptCount,
            deadLettered: true,
            finalError,
            deadLetterId,
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
        const deadLetterId = persistDeadLetter(
          rawUrl,
          payload,
          attemptCount,
          resolved.maxAttempts,
          msg,
          options
        );
        return {
          success: false,
          attemptCount,
          deadLettered: true,
          finalError: msg,
          deadLetterId,
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
