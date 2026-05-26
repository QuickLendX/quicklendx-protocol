import { describe, expect, it } from "@jest/globals";
import https from "node:https";
import { Readable } from "node:stream";
import type { IncomingMessage } from "node:http";
import {
  areAllDnsResultsPublicForWebhook,
  isBlockedDestinationIP,
} from "../src/services/webhook/blockedAddress";
import {
  hostMatchesAllowRule,
  loadWebhookEgressPolicyFromEnv,
  hostnameViolatesAllowPolicy,
  hostnameViolatesDenyPolicy,
} from "../src/services/webhook/egressPolicy";
import {
  deliverWebhookJson,
  readBodyWithByteLimit,
  WebhookDeliveryError,
  type OnceResult,
} from "../src/services/webhook/delivery";
import {
  validateWebhookUrl,
  WebhookUrlValidationError,
} from "../src/services/webhook/urlValidation";
import type { WebhookEgressPolicy } from "../src/services/webhook/egressPolicy";
import {
  createWebhookSecureLookup,
  type DnsLookupFn,
} from "../src/services/webhook/secureLookup";

const basePolicy: WebhookEgressPolicy = {
  hostAllowRules: [],
  hostDenyRules: [],
  maxRedirects: 3,
  timeoutMs: 5000,
  maxResponseBytes: 4096,
};

function asIncomingMessage(r: Readable): IncomingMessage {
  return r as unknown as IncomingMessage;
}

describe("webhook SSRF defenses", () => {
  describe("isBlockedDestinationIP", () => {
    it("blocks RFC1918 and loopback IPv4", () => {
      expect(isBlockedDestinationIP("127.0.0.1")).toBe(true);
      expect(isBlockedDestinationIP("10.0.0.1")).toBe(true);
      expect(isBlockedDestinationIP("172.16.0.1")).toBe(true);
      expect(isBlockedDestinationIP("192.168.4.4")).toBe(true);
      expect(isBlockedDestinationIP("169.254.1.1")).toBe(true);
      expect(isBlockedDestinationIP("100.64.0.1")).toBe(true);
      expect(isBlockedDestinationIP("0.0.0.0")).toBe(true);
      expect(isBlockedDestinationIP("224.0.0.1")).toBe(true);
    });

    it("allows common public IPv4", () => {
      expect(isBlockedDestinationIP("8.8.8.8")).toBe(false);
      expect(isBlockedDestinationIP("1.1.1.1")).toBe(false);
    });

    it("blocks documentation and TEST-NET IPv4", () => {
      expect(isBlockedDestinationIP("192.0.2.1")).toBe(true);
      expect(isBlockedDestinationIP("198.51.100.2")).toBe(true);
      expect(isBlockedDestinationIP("203.0.113.9")).toBe(true);
    });

    it("blocks loopback and ULA IPv6", () => {
      expect(isBlockedDestinationIP("::1")).toBe(true);
      expect(isBlockedDestinationIP("fd00::1")).toBe(true);
      expect(isBlockedDestinationIP("fc01::1")).toBe(true);
      expect(isBlockedDestinationIP("fe80::1")).toBe(true);
      expect(isBlockedDestinationIP("ff02::1")).toBe(true);
    });

    it("blocks IPv4-mapped SSRF targets", () => {
      expect(isBlockedDestinationIP("::ffff:127.0.0.1")).toBe(true);
      expect(isBlockedDestinationIP("::ffff:192.168.0.1")).toBe(true);
    });

    it("treats malformed strings as blocked", () => {
      expect(isBlockedDestinationIP("not-an-ip")).toBe(true);
    });

    it("allows routable global unicast IPv6", () => {
      expect(isBlockedDestinationIP("2001:4860:4860::8888")).toBe(false);
    });
  });

  describe("areAllDnsResultsPublicForWebhook", () => {
    it("rejects empty and mixed public/private DNS answers", () => {
      expect(areAllDnsResultsPublicForWebhook([])).toBe(false);
      expect(
        areAllDnsResultsPublicForWebhook([
          { address: "1.1.1.1" },
          { address: "10.0.0.1" },
        ]),
      ).toBe(false);
      expect(areAllDnsResultsPublicForWebhook([{ address: "1.1.1.1" }])).toBe(true);
    });
  });

  describe("egress policy", () => {
    it("parses environment overrides", () => {
      const p = loadWebhookEgressPolicyFromEnv({
        WEBHOOK_HOST_ALLOWLIST: "*.hooks.example.com,static.dev",
        WEBHOOK_HOST_DENYLIST: "evil.com",
        WEBHOOK_MAX_REDIRECTS: "1",
        WEBHOOK_TIMEOUT_MS: "2500",
        WEBHOOK_MAX_RESPONSE_BYTES: "1024",
      });
      expect(p.hostAllowRules).toEqual(["*.hooks.example.com", "static.dev"]);
      expect(p.hostDenyRules).toEqual(["evil.com"]);
      expect(p.maxRedirects).toBe(1);
      expect(p.timeoutMs).toBe(2500);
      expect(p.maxResponseBytes).toBe(1024);
    });

    it("falls back to defaults when numeric env values are invalid", () => {
      const p = loadWebhookEgressPolicyFromEnv({
        WEBHOOK_MAX_REDIRECTS: "not-a-number",
        WEBHOOK_TIMEOUT_MS: "-5",
        WEBHOOK_MAX_RESPONSE_BYTES: "NaN",
      });
      expect(p.maxRedirects).toBe(3);
      expect(p.timeoutMs).toBe(10_000);
      expect(p.maxResponseBytes).toBe(65_536);
    });

    it("matches wildcard allow rules", () => {
      expect(hostMatchesAllowRule("a.hooks.example.com", "*.hooks.example.com")).toBe(
        true,
      );
      expect(hostMatchesAllowRule("hooks.example.com", "*.hooks.example.com")).toBe(
        false,
      );
      expect(hostMatchesAllowRule("other.com", "*.hooks.example.com")).toBe(false);
    });

    it("flags built-in deny hosts and .local", () => {
      expect(hostnameViolatesDenyPolicy("localhost", basePolicy)).toBe(true);
      expect(hostnameViolatesDenyPolicy("metadata.google.internal", basePolicy)).toBe(
        true,
      );
      expect(hostnameViolatesDenyPolicy("printer.local", basePolicy)).toBe(true);
      expect(hostnameViolatesDenyPolicy("api.example.com", basePolicy)).toBe(false);
    });

    it("honours explicit deny list suffixes", () => {
      const p: WebhookEgressPolicy = {
        ...basePolicy,
        hostDenyRules: ["blocked.test"],
      };
      expect(hostnameViolatesDenyPolicy("x.blocked.test", p)).toBe(true);
      expect(hostnameViolatesDenyPolicy("blocked.test", p)).toBe(true);
      expect(hostnameViolatesDenyPolicy("safe.test", p)).toBe(false);
    });

    it("requires allowlist when configured", () => {
      const p: WebhookEgressPolicy = {
        ...basePolicy,
        hostAllowRules: ["hooks.partner.com", "*.cdn.example"],
      };
      expect(hostnameViolatesAllowPolicy("hooks.partner.com", p)).toBe(false);
      expect(hostnameViolatesAllowPolicy("x.cdn.example", p)).toBe(false);
      expect(hostnameViolatesAllowPolicy("evil.com", p)).toBe(true);
    });
  });

  describe("validateWebhookUrl", () => {
    it("accepts a normal HTTPS URL", () => {
      const u = validateWebhookUrl("https://hooks.slack.com/services/ABC", basePolicy);
      expect(u.hostname).toBe("hooks.slack.com");
    });

    it("rejects non-https schemes", () => {
      expect(() => validateWebhookUrl("http://example.com/hook", basePolicy)).toThrow(
        WebhookUrlValidationError,
      );
    });

    it("rejects credentials in URL", () => {
      expect(() =>
        validateWebhookUrl("https://user:pass@example.com/hook", basePolicy),
      ).toThrow(WebhookUrlValidationError);
    });

    it("rejects literal private IPs", () => {
      expect(() =>
        validateWebhookUrl("https://192.168.1.1/webhook", basePolicy),
      ).toThrow(WebhookUrlValidationError);
    });

    it("rejects hosts that fail allowlist", () => {
      const p: WebhookEgressPolicy = {
        ...basePolicy,
        hostAllowRules: ["only.example"],
      };
      expect(() =>
        validateWebhookUrl("https://other.example/path", p),
      ).toThrow(WebhookUrlValidationError);
    });

    it("rejects malformed URLs", () => {
      expect(() => validateWebhookUrl("https://[broken", basePolicy)).toThrow(
        WebhookUrlValidationError,
      );
    });

    it("rejects built-in deny hosts before DNS", () => {
      expect(() =>
        validateWebhookUrl("https://localhost/webhook", basePolicy),
      ).toThrow(WebhookUrlValidationError);
    });

    it("accepts literal public IPv4 in URL", () => {
      const u = validateWebhookUrl("https://1.1.1.1/path", basePolicy);
      expect(u.hostname).toBe("1.1.1.1");
    });
  });

  describe("deliverWebhookJson with injected transport", () => {
    it("wraps validation failures as WebhookDeliveryError", async () => {
      await expect(
        deliverWebhookJson("http://bad/http-only", {}, basePolicy, {
          requestImpl: async () =>
            Promise.resolve({
              statusCode: 200,
              headers: {},
              body: Buffer.from(""),
            }),
        }),
      ).rejects.toMatchObject({ code: "INVALID_SCHEME" });
    });

    it("follows redirects with full re-validation", async () => {
      const calls: string[] = [];
      const requestImpl = async (
        target: URL,
        _body: string,
        _policy: WebhookEgressPolicy,
        _agent: https.Agent,
      ): Promise<OnceResult> => {
        calls.push(target.href);
        if (calls.length === 1) {
          return {
            statusCode: 302,
            headers: { location: "/two" },
            body: Buffer.from(""),
          };
        }
        if (calls.length === 2) {
          return {
            statusCode: 302,
            headers: { location: "https://hooks.partner.com/final" },
            body: Buffer.from(""),
          };
        }
        return {
          statusCode: 204,
          headers: {},
          body: Buffer.from(""),
        };
      };

      const p: WebhookEgressPolicy = {
        ...basePolicy,
        hostAllowRules: ["start.example", "*.partner.com"],
      };

      const result = await deliverWebhookJson(
        "https://start.example/one",
        { ok: true },
        p,
        {
          requestImpl,
          createAgent: () => new https.Agent(),
        },
      );

      expect(result.redirectCount).toBe(2);
      expect(result.statusCode).toBe(204);
      expect(calls[0]).toContain("start.example");
      expect(calls[2]).toContain("hooks.partner.com");
    });

    it("blocks redirect downgrades to http", async () => {
      const requestImpl = async (): Promise<OnceResult> => ({
        statusCode: 302,
        headers: { location: "http://public.example/unsafe" },
        body: Buffer.from(""),
      });

      await expect(
        deliverWebhookJson("https://public.example/start", {}, basePolicy, {
          requestImpl,
          createAgent: () => new https.Agent(),
        }),
      ).rejects.toMatchObject({ code: "INVALID_SCHEME" });
    });

    it("enforces maxRedirects", async () => {
      const requestImpl = async (): Promise<OnceResult> => ({
        statusCode: 302,
        headers: { location: "https://public.example/next" },
        body: Buffer.from(""),
      });

      await expect(
        deliverWebhookJson(
          "https://public.example/start",
          {},
          { ...basePolicy, maxRedirects: 0 },
          {
            requestImpl,
            createAgent: () => new https.Agent(),
          },
        ),
      ).rejects.toMatchObject({ code: "TOO_MANY_REDIRECTS" });
    });

    it("returns final response metadata", async () => {
      const requestImpl = async (): Promise<OnceResult> => ({
        statusCode: 200,
        headers: {},
        body: Buffer.from("ack"),
      });

      const result = await deliverWebhookJson(
        "https://public.example/hook",
        { a: 1 },
        basePolicy,
        {
          requestImpl,
          createAgent: () => new https.Agent(),
        },
      );
      expect(result.responseBodyBytes).toBe(3);
      expect(result.redirectCount).toBe(0);
    });
  });

  describe("createWebhookSecureLookup", () => {
    it("invokes callback with error when dns lookup fails", async () => {
      const lookupFn = jest.fn((_h: string, _o: unknown, cb: Function) => {
        process.nextTick(() => cb(new Error("ENOTFOUND")));
      });
      const secure = createWebhookSecureLookup(lookupFn as unknown as DnsLookupFn);
      await new Promise<void>((resolve, reject) => {
        secure("nope.example", {}, (err, _addr, _fam) => {
          try {
            expect(err).toBeInstanceOf(Error);
            resolve();
          } catch (e) {
            reject(e);
          }
        });
      });
    });

    it("blocks when dns returns an empty answer set", async () => {
      const lookupFn = jest.fn((_h: string, _o: unknown, cb: Function) => {
        process.nextTick(() => cb(null, []));
      });
      const secure = createWebhookSecureLookup(lookupFn as unknown as DnsLookupFn);
      await new Promise<void>((resolve, reject) => {
        secure("x.example", {}, (err, _addr, _fam) => {
          try {
            expect(err).toBeDefined();
            expect((err as Error).message).toContain("WEBHOOK_DNS_EMPTY");
            resolve();
          } catch (e) {
            reject(e);
          }
        });
      });
    });

    it("blocks when any resolved address is non-public", async () => {
      const lookupFn = jest.fn((_h: string, _o: unknown, cb: Function) => {
        process.nextTick(() => cb(null, [{ address: "10.0.0.1", family: 4 }]));
      });
      const secure = createWebhookSecureLookup(lookupFn as unknown as DnsLookupFn);
      await new Promise<void>((resolve, reject) => {
        secure("internal.example", {}, (err, _addr, _fam) => {
          try {
            expect(err).toBeDefined();
            expect((err as Error).message).toContain("WEBHOOK_EGRESS_BLOCKED");
            resolve();
          } catch (e) {
            reject(e);
          }
        });
      });
    });

    it("returns the first address when all answers are public", async () => {
      const lookupFn = jest.fn((_h: string, _o: unknown, cb: Function) => {
        process.nextTick(() =>
          cb(null, [
            { address: "1.1.1.1", family: 4 },
            { address: "1.0.0.1", family: 4 },
          ]),
        );
      });
      const secure = createWebhookSecureLookup(lookupFn as unknown as DnsLookupFn);
      await new Promise<void>((resolve, reject) => {
        secure("cloudflare-dns.com", {}, (err, addr, fam) => {
          try {
            expect(err).toBeNull();
            expect(addr).toBe("1.1.1.1");
            expect(fam).toBe(4);
            resolve();
          } catch (e) {
            reject(e);
          }
        });
      });
    });

    it("supports IPv6 public answers", async () => {
      const lookupFn = jest.fn((_h: string, _o: unknown, cb: Function) => {
        process.nextTick(() =>
          cb(null, [{ address: "2001:4860:4860::8888", family: 6 }]),
        );
      });
      const secure = createWebhookSecureLookup(lookupFn as unknown as DnsLookupFn);
      await new Promise<void>((resolve, reject) => {
        secure("dns.google", {}, (err, addr, fam) => {
          try {
            expect(err).toBeNull();
            expect(addr).toContain("2001");
            expect(fam).toBe(6);
            resolve();
          } catch (e) {
            reject(e);
          }
        });
      });
    });
  });

  describe("readBodyWithByteLimit", () => {
    it("rejects streams that exceed the byte cap", async () => {
      const stream = Readable.from([Buffer.alloc(100), Buffer.alloc(100)]);
      const res = asIncomingMessage(stream);
      await expect(readBodyWithByteLimit(res, 150)).rejects.toBeInstanceOf(
        WebhookDeliveryError,
      );
    });

    it("concatenates chunks under the cap", async () => {
      const stream = Readable.from([Buffer.from("a"), Buffer.from("b")]);
      const buf = await readBodyWithByteLimit(asIncomingMessage(stream), 10);
      expect(buf.toString()).toBe("ab");
    });

    it("propagates stream errors", async () => {
      const stream = new Readable({
        read() {
          this.destroy(new Error("boom"));
        },
      });
      await expect(readBodyWithByteLimit(asIncomingMessage(stream), 100)).rejects.toThrow(
        "boom",
      );
    });
  });

});
