import { deliverWithRetry } from "../services/webhook/retryScheduler";
import type { WebhookEgressPolicy } from "../services/webhook/egressPolicy";
import { WebhookDeliveryError } from "../services/webhook/delivery";

const basePolicy: WebhookEgressPolicy = {
  timeoutMs: 3000,
  maxResponseBytes: 1024,
  maxRedirects: 3,
  hostAllowRules: [],
  hostDenyRules: [],
};

const retryPolicy = {
  maxAttempts: 3,
  initialDelayMs: 10,
  maxDelayMs: 100,
};

describe("deliverWithRetry", () => {
  it("succeeds on first attempt", async () => {
    const mockRequest = jest.fn().mockResolvedValue({
      statusCode: 200,
      headers: {},
      body: Buffer.from("ok"),
    });

    const result = await deliverWithRetry(
      "https://example.com/webhook",
      { event: "test" },
      basePolicy,
      retryPolicy,
      { requestImpl: mockRequest, createAgent: () => ({} as any) },
    );

    expect(result.success).toBe(true);
    expect(result.attemptCount).toBe(1);
    expect(result.deadLettered).toBe(false);
  });

  it("retries on 5xx then succeeds", async () => {
    const mockRequest = jest.fn()
      .mockResolvedValueOnce({ statusCode: 503, headers: {}, body: Buffer.from("") })
      .mockResolvedValueOnce({ statusCode: 200, headers: {}, body: Buffer.from("ok") });

    const result = await deliverWithRetry(
      "https://example.com/webhook",
      { event: "test" },
      basePolicy,
      retryPolicy,
      { requestImpl: mockRequest, createAgent: () => ({} as any) },
    );

    expect(result.success).toBe(true);
    expect(result.attemptCount).toBe(2);
  });

  it("does not retry on permanent 4xx", async () => {
    const mockRequest = jest.fn().mockResolvedValue({
      statusCode: 404,
      headers: {},
      body: Buffer.from("not found"),
    });

    const result = await deliverWithRetry(
      "https://example.com/webhook",
      { event: "test" },
      basePolicy,
      retryPolicy,
      { requestImpl: mockRequest, createAgent: () => ({} as any) },
    );

    expect(result.success).toBe(false);
    expect(result.deadLettered).toBe(true);
    expect(result.attemptCount).toBe(1);
  });

  it("dead-letters after max attempts exhausted", async () => {
    const mockRequest = jest.fn().mockRejectedValue(
      new WebhookDeliveryError("TIMEOUT", "Webhook delivery exceeded timeout")
    );

    const result = await deliverWithRetry(
      "https://example.com/webhook",
      { event: "test" },
      basePolicy,
      retryPolicy,
      { requestImpl: mockRequest, createAgent: () => ({} as any) },
    );

    expect(result.success).toBe(false);
    expect(result.deadLettered).toBe(true);
    expect(result.attemptCount).toBe(3);
  });

  it("does not retry non-retryable errors", async () => {
    const mockRequest = jest.fn().mockRejectedValue(
      new WebhookDeliveryError("URL_INVALID", "Bad URL")
    );

    const result = await deliverWithRetry(
      "https://example.com/webhook",
      { event: "test" },
      basePolicy,
      retryPolicy,
      { requestImpl: mockRequest, createAgent: () => ({} as any) },
    );

    expect(result.deadLettered).toBe(true);
    expect(result.attemptCount).toBe(1);
  });
});
