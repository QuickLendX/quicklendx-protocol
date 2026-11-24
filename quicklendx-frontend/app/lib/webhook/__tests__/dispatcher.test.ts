import { createHmac } from "crypto";
import type { SubscriberConfig } from "../types";

const CURSOR = { ledger_seq: 42, tx_hash: "abc123", event_index: 3 };
const SECRET = "test-webhook-secret";

function makeSubscriber(
  overrides: Partial<SubscriberConfig> = {}
): SubscriberConfig {
  return {
    subscriber_id: "subscriber-1",
    endpoint_url: "https://example.test/webhook",
    pinned_version: 2,
    pin_expires_at: null,
    secret: SECRET,
    event_types: [],
    ...overrides,
  };
}

describe("webhook dispatcher", () => {
  let dispatcher: typeof import("../dispatcher");
  let fetchMock: jest.SpyInstance;

  beforeEach(() => {
    jest.resetModules();
    dispatcher = require("../dispatcher") as typeof import("../dispatcher");
    fetchMock = jest
      .spyOn(globalThis, "fetch")
      .mockResolvedValue({ status: 204 } as Response);
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("validates subscriber identity, endpoint, secret, and version", () => {
    const invalidRequiredFields: Partial<SubscriberConfig>[] = [
      { subscriber_id: "" },
      { endpoint_url: "" },
      { secret: "" },
    ];

    for (const invalidFields of invalidRequiredFields) {
      expect(() =>
        dispatcher.registerSubscriber(makeSubscriber(invalidFields))
      ).toThrow(/required/i);
    }

    for (const pinnedVersion of [0, 3]) {
      const invalidVersion = {
        ...makeSubscriber(),
        pinned_version: pinnedVersion,
      } as SubscriberConfig;
      expect(() => dispatcher.registerSubscriber(invalidVersion)).toThrow(
        /pinned_version must be between/i
      );
    }
  });

  it("sends the expected headers and an HMAC signature of the exact body", async () => {
    const subscriber = makeSubscriber();
    dispatcher.registerSubscriber(subscriber);

    const results = await dispatcher.dispatchEvent({
      cursor: CURSOR,
      event_type: "invoice.uploaded",
      payload: { invoice_id: "invoice-1" },
      nowMs: 1_000,
    });

    const [url, request] = fetchMock.mock.calls[0];
    expect(url).toBe(subscriber.endpoint_url);
    expect(request.method).toBe("POST");
    expect(request.headers).toEqual({
      "Content-Type": "application/json",
      "X-QuickLendX-Signature": `sha256=${createHmac("sha256", SECRET)
        .update(request.body)
        .digest("hex")}`,
      "X-QuickLendX-Version": "2",
    });
    expect(results).toEqual([
      {
        subscriber_id: subscriber.subscriber_id,
        status: "delivered",
        http_status: 204,
      },
    ]);
  });

  it("downgrades the payload for a subscriber pinned to v1", async () => {
    dispatcher.registerSubscriber(
      makeSubscriber({ pinned_version: 1, pin_expires_at: 10_000 })
    );

    await dispatcher.dispatchEvent({
      cursor: CURSOR,
      event_type: "invoice.uploaded",
      payload: { invoice_id: "invoice-1" },
      nowMs: 1_000,
    });

    const request = fetchMock.mock.calls[0][1];
    const envelope = JSON.parse(request.body);
    expect(envelope).toMatchObject({
      version: 1,
      ledger_seq: CURSOR.ledger_seq,
      tx_hash: CURSOR.tx_hash,
      event_type: "invoice.uploaded",
      payload: { invoice_id: "invoice-1" },
    });
    expect(envelope.cursor).toBeUndefined();
  });

  it("force-upgrades an expired pin and persists the new version", async () => {
    const subscriber = makeSubscriber({
      pinned_version: 1,
      pin_expires_at: 100,
    });
    dispatcher.registerSubscriber(subscriber);
    const warn = jest.spyOn(console, "warn").mockImplementation(() => {});

    await dispatcher.dispatchEvent({
      cursor: CURSOR,
      event_type: "invoice.uploaded",
      payload: {},
      compatWindow: {
        window_seconds: 90,
        force_upgrade_expired_pins: true,
      },
      nowMs: 100_000,
    });

    expect(JSON.parse(fetchMock.mock.calls[0][1].body).version).toBe(2);
    expect(dispatcher.getSubscriber(subscriber.subscriber_id)).toMatchObject({
      pinned_version: 2,
      pin_expires_at: null,
    });
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining("auto-upgraded to v2")
    );
  });

  it("warns but preserves an expired pin when force-upgrade is disabled", async () => {
    const subscriber = makeSubscriber({
      pinned_version: 1,
      pin_expires_at: 100,
    });
    dispatcher.registerSubscriber(subscriber);
    const warn = jest.spyOn(console, "warn").mockImplementation(() => {});

    await dispatcher.dispatchEvent({
      cursor: CURSOR,
      event_type: "invoice.uploaded",
      payload: {},
      compatWindow: {
        window_seconds: 90,
        force_upgrade_expired_pins: false,
      },
      nowMs: 100_000,
    });

    expect(JSON.parse(fetchMock.mock.calls[0][1].body).version).toBe(1);
    expect(dispatcher.getSubscriber(subscriber.subscriber_id)).toEqual(
      subscriber
    );
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining("force_upgrade_expired_pins is disabled")
    );
  });

  it("continues fan-out after one subscriber's delivery fails", async () => {
    const first = makeSubscriber({ subscriber_id: "subscriber-fails" });
    const second = makeSubscriber({ subscriber_id: "subscriber-succeeds" });
    dispatcher.registerSubscriber(first);
    dispatcher.registerSubscriber(second);
    fetchMock
      .mockRejectedValueOnce(new Error("connection refused"))
      .mockResolvedValueOnce({ status: 200 } as Response);

    const results = await dispatcher.dispatchEvent({
      cursor: CURSOR,
      event_type: "invoice.uploaded",
      payload: {},
      nowMs: 1_000,
    });

    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(results).toEqual([
      {
        subscriber_id: first.subscriber_id,
        status: "failed",
        error: "connection refused",
      },
      {
        subscriber_id: second.subscriber_id,
        status: "delivered",
        http_status: 200,
      },
    ]);
  });
});
