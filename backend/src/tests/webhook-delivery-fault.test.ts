import * as dns from "node:dns";
import { EventEmitter } from "node:events";
import type { IncomingMessage } from "node:http";
import {
  deliverWebhookJson,
  readBodyWithByteLimit,
  WebhookDeliveryError,
} from "../services/webhook/delivery";
import type { WebhookEgressPolicy } from "../services/webhook/egressPolicy";
import { FaultyDns, originalDnsLookup } from "./helpers/faultInjector";

const basePolicy: WebhookEgressPolicy = {
  timeoutMs: 1000,
  maxResponseBytes: 100,
  maxRedirects: 3,
  hostAllowRules: [],
  hostDenyRules: [],
};

describe("Webhook Delivery - Fault Injection Tests", () => {
  let faultyDns: FaultyDns;

  beforeEach(() => {
    faultyDns = new FaultyDns();
  });

  afterEach(() => {
    faultyDns.restore();
  });

  it("fails delivery when DNS resolves to a blocked/private IP", async () => {
    // Resolve to private IPv4
    faultyDns.setup(["127.0.0.1"]);

    await expect(
      deliverWebhookJson("https://any-domain-resolved-to-local.com/webhook", { foo: "bar" }, basePolicy)
    ).rejects.toThrow(WebhookDeliveryError);

    try {
      await deliverWebhookJson("https://any-domain-resolved-to-local.com/webhook", { foo: "bar" }, basePolicy);
    } catch (err: any) {
      expect(err.code).toBe("EGRESS_BLOCKED");
    }
  });

  it("fails delivery when DNS lookup itself fails", async () => {
    faultyDns.setup([], true);

    await expect(
      deliverWebhookJson("https://broken-dns-domain.com/webhook", { foo: "bar" }, basePolicy)
    ).rejects.toThrow(WebhookDeliveryError);
  });

  it("verifies no test leaks the global dns.lookup mock", () => {
    // Setup and restore
    faultyDns.setup(["1.1.1.1"]);
    expect(dns.lookup).not.toBe(originalDnsLookup);

    faultyDns.restore();
    expect(dns.lookup).toBe(originalDnsLookup);
  });

  it("falls back to actual dns.lookup and supports two-argument call signature", (done) => {
    // Setup with empty config to force fallback to original Lookup
    faultyDns.setup();

    // Call two-argument signature dns.lookup(hostname, callback)
    dns.lookup("localhost", (err, address, family) => {
      expect(err).toBeNull();
      expect(address).toBeDefined();
      done();
    });
  });

  describe("readBodyWithByteLimit truncation", () => {
    it("successfully reads body within limit", async () => {
      const mockStream = new EventEmitter() as any;
      mockStream.destroy = jest.fn();

      const promise = readBodyWithByteLimit(mockStream, 15);

      mockStream.emit("data", Buffer.from("hello "));
      mockStream.emit("data", Buffer.from("world"));
      mockStream.emit("end");

      const body = await promise;
      expect(body.toString()).toBe("hello world");
      expect(mockStream.destroy).not.toHaveBeenCalled();
    });

    it("truncates and throws RESPONSE_TOO_LARGE when body exceeds limit", async () => {
      const mockStream = new EventEmitter() as any;
      mockStream.destroy = jest.fn();

      const promise = readBodyWithByteLimit(mockStream, 5);

      mockStream.emit("data", Buffer.from("hello ")); // length 6, which is > 5

      await expect(promise).rejects.toThrow(WebhookDeliveryError);
      await expect(promise).rejects.toThrow("Webhook response exceeded configured size limit");
      
      try {
        await promise;
      } catch (err: any) {
        expect(err.code).toBe("RESPONSE_TOO_LARGE");
      }

      expect(mockStream.destroy).toHaveBeenCalled();
    });

    it("propagates stream errors correctly", async () => {
      const mockStream = new EventEmitter() as any;
      mockStream.destroy = jest.fn();

      const promise = readBodyWithByteLimit(mockStream, 100);

      mockStream.emit("error", new Error("stream read failure"));

      await expect(promise).rejects.toThrow("stream read failure");
    });
  });
});
