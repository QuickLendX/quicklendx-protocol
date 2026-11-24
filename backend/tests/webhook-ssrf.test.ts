/**
 * Webhook SSRF defenses (#3201).
 *
 * Covers `src/services/webhook/`: `blockedAddress.ts`, `urlValidation.ts`,
 * `secureLookup.ts`, `egressPolicy.ts` and the delivery pipeline in
 * `delivery.ts`.
 *
 * Every blocked address range is its own table row, with the first and last
 * address of the range, and each blocked range is paired with rows just outside
 * it so an off-by-one in a boundary shows up as a failing row.
 *
 * Run with: npx jest tests/webhook-ssrf.test.ts
 */

import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  jest,
} from "@jest/globals";
import dns from "node:dns";
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

// ---------------------------------------------------------------------------
// Address tables
// ---------------------------------------------------------------------------

/** [range, note, address]: every one of these must be refused. */
const BLOCKED_IPV4: [string, string, string][] = [
  ["0.0.0.0/8", "first", "0.0.0.0"],
  ["0.0.0.0/8", "last", "0.255.255.255"],
  ["10.0.0.0/8", "first", "10.0.0.0"],
  ["10.0.0.0/8", "typical", "10.0.0.1"],
  ["10.0.0.0/8", "last", "10.255.255.255"],
  ["100.64.0.0/10", "first (CGNAT)", "100.64.0.0"],
  ["100.64.0.0/10", "last (CGNAT)", "100.127.255.255"],
  ["127.0.0.0/8", "loopback", "127.0.0.1"],
  ["127.0.0.0/8", "last", "127.255.255.255"],
  ["169.254.0.0/16", "first (link-local)", "169.254.0.0"],
  ["169.254.0.0/16", "cloud metadata endpoint", "169.254.169.254"],
  ["169.254.0.0/16", "last", "169.254.255.255"],
  ["172.16.0.0/12", "first", "172.16.0.0"],
  ["172.16.0.0/12", "typical", "172.20.1.1"],
  ["172.16.0.0/12", "last", "172.31.255.255"],
  ["192.0.0.0/24", "IETF protocol assignments", "192.0.0.1"],
  ["192.0.2.0/24", "TEST-NET-1", "192.0.2.1"],
  ["192.88.99.0/24", "6to4 relay anycast", "192.88.99.1"],
  ["192.168.0.0/16", "first", "192.168.0.0"],
  ["192.168.0.0/16", "typical", "192.168.4.4"],
  ["192.168.0.0/16", "last", "192.168.255.255"],
  ["198.18.0.0/16", "benchmarking", "198.18.0.1"],
  ["198.51.100.0/24", "TEST-NET-2", "198.51.100.2"],
  ["203.0.113.0/24", "TEST-NET-3", "203.0.113.9"],
  ["224.0.0.0/4", "multicast first", "224.0.0.1"],
  ["224.0.0.0/4", "multicast last", "239.255.255.255"],
  ["255.255.255.255/32", "limited broadcast", "255.255.255.255"],
];

/** Addresses just outside each blocked IPv4 range, which must stay reachable. */
const PUBLIC_IPV4: [string, string][] = [
  ["Google DNS", "8.8.8.8"],
  ["Cloudflare DNS", "1.1.1.1"],
  ["just below 10/8", "9.255.255.255"],
  ["just above 10/8", "11.0.0.0"],
  ["just below 100.64/10", "100.63.255.255"],
  ["just above 100.64/10", "100.128.0.0"],
  ["just below 127/8", "126.255.255.255"],
  ["just above 127/8", "128.0.0.1"],
  ["just below 169.254/16", "169.253.255.255"],
  ["just above 169.254/16", "169.255.0.0"],
  ["just below 172.16/12", "172.15.255.255"],
  ["just above 172.16/12", "172.32.0.0"],
  ["between 192.0.0/24 and 192.0.2/24", "192.0.1.1"],
  ["just above 192.0.2/24", "192.0.3.1"],
  ["just below 192.88.99/24", "192.88.98.255"],
  ["just above 192.88.99/24", "192.88.100.0"],
  ["just below 192.168/16", "192.167.255.255"],
  ["just above 192.168/16", "192.169.0.0"],
  ["just below 198.18/16", "198.17.255.255"],
  ["just below 198.51.100/24", "198.51.99.255"],
  ["just above 198.51.100/24", "198.51.101.0"],
  ["just below 203.0.113/24", "203.0.112.255"],
  ["just above 203.0.113/24", "203.0.114.0"],
  ["just below multicast", "223.255.255.255"],
];

const BLOCKED_IPV6: [string, string, string][] = [
  ["::/96", "unspecified (connects to localhost on Linux)", "::"],
  ["::1/128", "loopback", "::1"],
  ["::1/128", "loopback, expanded", "0:0:0:0:0:0:0:1"],
  ["::/96", "IPv4-compatible loopback (dotted)", "::127.0.0.1"],
  ["::/96", "IPv4-compatible loopback (hex)", "::7f00:1"],
  ["fe80::/10", "first", "fe80::1"],
  ["fe80::/10", "last", "febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff"],
  ["fc00::/7", "ULA first", "fc00::1"],
  ["fc00::/7", "ULA typical", "fd00::1"],
  ["fc00::/7", "ULA last", "fdff::1"],
  ["fc00::/7", "AWS IPv6 metadata endpoint", "fd00:ec2::254"],
  ["ff00::/8", "multicast link-local", "ff02::1"],
  ["ff00::/8", "multicast first", "ff00::1"],
  ["ff00::/8", "multicast last", "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"],
];

/** Routable global unicast IPv6, plus addresses just outside the blocked ranges. */
const PUBLIC_IPV6: [string, string][] = [
  ["Google DNS", "2001:4860:4860::8888"],
  ["Cloudflare DNS", "2606:4700:4700::1111"],
  ["Google", "2a00:1450:4001:81b::200e"],
  ["just below fc00::/7", "fbff::1"],
  ["just above fdff::", "fe00::1"],
  ["just below fe80::/10", "fe7f::1"],
];

/**
 * IPv4 addresses reached through an IPv4-mapped IPv6 address (`::ffff:a.b.c.d`).
 * Each one has to be refused in every notation, because `new URL()` rewrites the
 * dotted form to the hex form, so a check that only reads the dotted form is
 * bypassed by an ordinary URL.
 */
const MAPPED_TARGETS: [string, string, string][] = [
  ["0.0.0.0", "::ffff:0:0", "0:0:0:0:0:ffff:0:0"],
  ["10.0.0.1", "::ffff:a00:1", "0:0:0:0:0:ffff:a00:1"],
  ["100.64.0.1", "::ffff:6440:1", "0:0:0:0:0:ffff:6440:1"],
  ["127.0.0.1", "::ffff:7f00:1", "0:0:0:0:0:ffff:7f00:1"],
  ["169.254.169.254", "::ffff:a9fe:a9fe", "0:0:0:0:0:ffff:a9fe:a9fe"],
  ["172.16.0.1", "::ffff:ac10:1", "0:0:0:0:0:ffff:ac10:1"],
  ["192.168.1.1", "::ffff:c0a8:101", "0:0:0:0:0:ffff:c0a8:101"],
];

/** Rows for `it.each`: [label, address] for every notation of every mapped target. */
const BLOCKED_MAPPED_ROWS: [string, string][] = MAPPED_TARGETS.flatMap(
  ([v4, hex, expanded]) => [
    [`${v4} mapped, dotted`, `::ffff:${v4}`] as [string, string],
    [`${v4} mapped, hex`, hex] as [string, string],
    [`${v4} mapped, hex, upper case`, hex.toUpperCase()] as [string, string],
    [`${v4} mapped, expanded`, expanded] as [string, string],
    [`${v4} mapped, expanded with dotted tail`, `0:0:0:0:0:ffff:${v4}`] as [
      string,
      string,
    ],
  ],
);

const PUBLIC_MAPPED_ROWS: [string, string][] = [
  ["8.8.8.8 mapped, dotted", "::ffff:8.8.8.8"],
  ["8.8.8.8 mapped, hex", "::ffff:808:808"],
  ["8.8.8.8 mapped, expanded", "0:0:0:0:0:ffff:808:808"],
  ["1.1.1.1 mapped, hex", "::ffff:101:101"],
];

const MALFORMED_ADDRESSES: [string, string][] = [
  ["not an address", "not-an-ip"],
  ["empty string", ""],
  ["three octets", "1.2.3"],
  ["five octets", "1.2.3.4.5"],
  ["octet out of range", "256.1.1.1"],
  ["trailing space", "127.0.0.1 "],
  ["bracketed IPv6", "[::1]"],
  ["bad IPv6 group", "::g"],
  ["too few IPv6 groups", "1:2:3"],
  ["double compression", "1::2::3"],
  ["hex IPv4", "0x7f.0.0.1"],
  ["decimal IPv4", "2130706433"],
];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("webhook SSRF defenses", () => {
  describe("isBlockedDestinationIP", () => {
    describe("blocked IPv4 ranges", () => {
      it.each(BLOCKED_IPV4)("%s blocks %s: %s", (_range, _note, ip) => {
        expect(isBlockedDestinationIP(ip)).toBe(true);
      });
    });

    describe("public IPv4 next to blocked ranges", () => {
      it.each(PUBLIC_IPV4)("allows %s: %s", (_note, ip) => {
        expect(isBlockedDestinationIP(ip)).toBe(false);
      });
    });

    describe("blocked IPv6 ranges", () => {
      it.each(BLOCKED_IPV6)("%s blocks %s: %s", (_range, _note, ip) => {
        expect(isBlockedDestinationIP(ip)).toBe(true);
      });
    });

    describe("public IPv6", () => {
      it.each(PUBLIC_IPV6)("allows %s: %s", (_note, ip) => {
        expect(isBlockedDestinationIP(ip)).toBe(false);
      });
    });

    describe("IPv4-mapped IPv6 judged by the embedded IPv4", () => {
      it.each(BLOCKED_MAPPED_ROWS)("blocks %s: %s", (_note, ip) => {
        expect(isBlockedDestinationIP(ip)).toBe(true);
      });

      it.each(PUBLIC_MAPPED_ROWS)("allows %s: %s", (_note, ip) => {
        expect(isBlockedDestinationIP(ip)).toBe(false);
      });
    });

    describe("malformed input fails closed", () => {
      it.each(MALFORMED_ADDRESSES)("blocks %s: %j", (_note, ip) => {
        expect(isBlockedDestinationIP(ip)).toBe(true);
      });
    });
  });

  describe("areAllDnsResultsPublicForWebhook", () => {
    it("rejects an empty answer set", () => {
      expect(areAllDnsResultsPublicForWebhook([])).toBe(false);
    });

    it("accepts a single public answer and several public answers", () => {
      expect(areAllDnsResultsPublicForWebhook([{ address: "1.1.1.1" }])).toBe(true);
      expect(
        areAllDnsResultsPublicForWebhook([
          { address: "1.1.1.1" },
          { address: "2001:4860:4860::8888" },
        ]),
      ).toBe(true);
    });

    it.each(BLOCKED_IPV4.map(([range, , ip]) => [range, ip]))(
      "rejects a single answer in %s: %s",
      (_range, ip) => {
        expect(areAllDnsResultsPublicForWebhook([{ address: ip }])).toBe(false);
      },
    );

    // DNS rebinding with a mixed answer set: one public record to look harmless,
    // one internal record to connect to. The position must not matter.
    it("rejects a public answer followed by a private one", () => {
      expect(
        areAllDnsResultsPublicForWebhook([
          { address: "1.1.1.1" },
          { address: "10.0.0.1" },
        ]),
      ).toBe(false);
    });

    it("rejects a private answer followed by a public one", () => {
      expect(
        areAllDnsResultsPublicForWebhook([
          { address: "10.0.0.1" },
          { address: "1.1.1.1" },
        ]),
      ).toBe(false);
    });

    it("rejects an internal address hidden as an IPv4-mapped AAAA answer", () => {
      expect(
        areAllDnsResultsPublicForWebhook([
          { address: "1.1.1.1" },
          { address: "::ffff:7f00:1" },
        ]),
      ).toBe(false);
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

    it("does not let a wildcard rule match a look-alike suffix", () => {
      expect(hostMatchesAllowRule("evilhooks.example.com", "*.hooks.example.com")).toBe(
        false,
      );
      expect(
        hostMatchesAllowRule("hooks.example.com.evil.com", "*.hooks.example.com"),
      ).toBe(false);
    });

    it.each([
      "localhost",
      "LOCALHOST",
      "sub.localhost",
      "metadata.google.internal",
      "metadata.google",
      "printer.local",
    ])("denies the built-in host %s", (host) => {
      expect(hostnameViolatesDenyPolicy(host, basePolicy)).toBe(true);
    });

    it("does not deny ordinary hosts", () => {
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
      expect(hostnameViolatesDenyPolicy("notblocked.test", p)).toBe(false);
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

    it("allows any host when no allowlist is configured", () => {
      expect(hostnameViolatesAllowPolicy("anything.example", basePolicy)).toBe(false);
    });
  });

  describe("validateWebhookUrl", () => {
    it("accepts a normal HTTPS URL", () => {
      const u = validateWebhookUrl("https://hooks.slack.com/services/ABC", basePolicy);
      expect(u.hostname).toBe("hooks.slack.com");
    });

    it("accepts literal public IPv4 and IPv6 in the URL", () => {
      expect(validateWebhookUrl("https://1.1.1.1/path", basePolicy).hostname).toBe(
        "1.1.1.1",
      );
      expect(
        validateWebhookUrl("https://[2001:4860:4860::8888]/", basePolicy).hostname,
      ).toBe("[2001:4860:4860::8888]");
    });

    it("accepts an IPv4-mapped IPv6 literal that carries a public address", () => {
      expect(() => validateWebhookUrl("https://[::ffff:8.8.8.8]/", basePolicy)).not.toThrow();
    });

    it.each([
      ["http", "http://example.com/hook"],
      ["ftp", "ftp://example.com/hook"],
      ["file", "file:///etc/passwd"],
      ["gopher", "gopher://example.com/"],
      ["ws", "ws://example.com/hook"],
      ["wss", "wss://example.com/hook"],
    ])("rejects the %s scheme", (_scheme, url) => {
      expect(() => validateWebhookUrl(url, basePolicy)).toThrow(
        expect.objectContaining({
          name: "WebhookUrlValidationError",
          code: "INVALID_SCHEME",
        }),
      );
    });

    it.each([
      "https://user:pass@example.com/hook",
      "https://user@example.com/hook",
      "https://:pass@example.com/hook",
    ])("rejects credentials in the URL: %s", (url) => {
      expect(() => validateWebhookUrl(url, basePolicy)).toThrow(
        expect.objectContaining({ code: "USERINFO_FORBIDDEN" }),
      );
    });

    it.each(["https://[broken", "not a url", ""])(
      "rejects the malformed URL %j",
      (url) => {
        expect(() => validateWebhookUrl(url, basePolicy)).toThrow(
          expect.objectContaining({ code: "INVALID_URL" }),
        );
      },
    );

    it("throws WebhookUrlValidationError", () => {
      expect(() => validateWebhookUrl("http://example.com/hook", basePolicy)).toThrow(
        WebhookUrlValidationError,
      );
    });

    it.each([
      "localhost",
      "LOCALHOST",
      "sub.localhost",
      "metadata.google.internal",
      "printer.local",
    ])("rejects the denied host %s before any DNS", (host) => {
      expect(() => validateWebhookUrl(`https://${host}/webhook`, basePolicy)).toThrow(
        expect.objectContaining({ code: "HOST_DENIED" }),
      );
    });

    it("rejects hosts that fail the allowlist", () => {
      const p: WebhookEgressPolicy = {
        ...basePolicy,
        hostAllowRules: ["only.example"],
      };
      expect(() => validateWebhookUrl("https://other.example/path", p)).toThrow(
        expect.objectContaining({ code: "HOST_NOT_ALLOWLISTED" }),
      );
    });

    // The URL parser normalises every one of these spellings, so the checks see
    // the canonical address. Each row is a way an attacker may write the same
    // internal target.
    describe("literal internal addresses, in every notation", () => {
      it.each([
        ["loopback", "https://127.0.0.1/"],
        ["loopback, decimal", "https://2130706433/"],
        ["loopback, hex", "https://0x7f000001/"],
        ["loopback, dotted hex", "https://0x7f.0.0.1/"],
        ["loopback, short form", "https://127.1/"],
        ["loopback, octal", "https://017700000001/"],
        ["RFC 1918 10/8", "https://10.0.0.1/"],
        ["RFC 1918 172.16/12", "https://172.16.0.1/"],
        ["RFC 1918 192.168/16", "https://192.168.1.1/webhook"],
        ["CGNAT", "https://100.64.0.1/"],
        ["cloud metadata", "https://169.254.169.254/latest/meta-data/"],
        ["cloud metadata, decimal", "https://2852039166/latest/meta-data/"],
        ["unspecified", "https://0.0.0.0/"],
        ["IPv6 loopback", "https://[::1]/"],
        ["IPv6 unspecified", "https://[::]/"],
        ["IPv6 unique local", "https://[fd00::1]/"],
        ["IPv6 AWS metadata", "https://[fd00:ec2::254]/"],
        ["IPv6 link-local", "https://[fe80::1]/"],
        ["IPv4-mapped loopback, dotted", "https://[::ffff:127.0.0.1]/"],
        ["IPv4-mapped loopback, hex", "https://[::ffff:7f00:1]/"],
        ["IPv4-mapped metadata, dotted", "https://[::ffff:169.254.169.254]/"],
        ["IPv4-mapped metadata, hex", "https://[::ffff:a9fe:a9fe]/"],
        ["IPv4-mapped private, hex", "https://[::ffff:c0a8:101]/"],
        ["IPv4-compatible loopback", "https://[::127.0.0.1]/"],
      ])("rejects %s: %s", (_note, url) => {
        expect(() => validateWebhookUrl(url, basePolicy)).toThrow(
          expect.objectContaining({ code: "BLOCKED_LITERAL_IP" }),
        );
      });
    });
  });

  describe("deliverWebhookJson with injected transport", () => {
    let httpsRequest: ReturnType<typeof jest.spyOn>;

    beforeEach(() => {
      jest.spyOn(console, "log").mockImplementation(() => {});
      // The injected transport must be the only thing that ever sends: a real
      // connection from a test would be an SSRF in the test suite itself.
      httpsRequest = jest.spyOn(https, "request").mockImplementation((() => {
        throw new Error("the network must not be reached");
      }) as never);
    });

    afterEach(() => {
      jest.restoreAllMocks();
    });

    function transport(responses: OnceResult[]) {
      const calls: string[] = [];
      const requestImpl = async (target: URL): Promise<OnceResult> => {
        calls.push(target.href);
        const next = responses[calls.length - 1];
        if (!next) throw new Error("unexpected extra request to " + target.href);
        return next;
      };
      return { calls, requestImpl };
    }

    const redirect = (location?: string): OnceResult => ({
      statusCode: 302,
      headers: location === undefined ? {} : { location },
      body: Buffer.from(""),
    });
    const ok = (status = 204): OnceResult => ({
      statusCode: status,
      headers: {},
      body: Buffer.from(""),
    });

    function deliver(
      url: string,
      t: ReturnType<typeof transport>,
      policy: WebhookEgressPolicy = basePolicy,
    ) {
      return deliverWebhookJson(url, { ok: true }, policy, {
        requestImpl: t.requestImpl,
        createAgent: () => new https.Agent(),
      });
    }

    it("wraps validation failures as WebhookDeliveryError", async () => {
      const t = transport([ok(200)]);
      const result = deliver("http://bad/http-only", t);
      await expect(result).rejects.toBeInstanceOf(WebhookDeliveryError);
      await expect(result).rejects.toMatchObject({ code: "INVALID_SCHEME" });
      expect(t.calls).toHaveLength(0);
    });

    it("refuses a literal internal address without making a request", async () => {
      const t = transport([ok(200)]);
      await expect(deliver("https://[::ffff:7f00:1]/hook", t)).rejects.toMatchObject({
        code: "BLOCKED_LITERAL_IP",
      });
      expect(t.calls).toHaveLength(0);
    });

    it("returns final response metadata", async () => {
      const t = transport([
        { statusCode: 200, headers: {}, body: Buffer.from("ack") },
      ]);
      const result = await deliver("https://public.example/hook", t);
      expect(result).toEqual({
        finalUrl: "https://public.example/hook",
        statusCode: 200,
        redirectCount: 0,
        responseBodyBytes: 3,
      });
    });

    describe("redirects", () => {
      // Current contract (delivery.ts): at most ONE redirect is followed, and
      // only if its target passes the same checks as the original URL. Anything
      // else is REDIRECT_NOT_ALLOWED and no further request is made.

      it("follows a single redirect after re-validating the target", async () => {
        const t = transport([redirect("/two"), ok(204)]);
        const result = await deliver("https://start.example/one", t);

        expect(t.calls).toEqual(["https://start.example/one", "https://start.example/two"]);
        expect(result.redirectCount).toBe(1);
        expect(result.statusCode).toBe(204);
        expect(result.finalUrl).toBe("https://start.example/two");
      });

      it("follows a redirect to another allow-listed host", async () => {
        const t = transport([redirect("https://hooks.partner.com/final"), ok(204)]);
        const policy: WebhookEgressPolicy = {
          ...basePolicy,
          hostAllowRules: ["start.example", "*.partner.com"],
        };
        const result = await deliver("https://start.example/one", t, policy);

        expect(result.redirectCount).toBe(1);
        expect(t.calls[1]).toBe("https://hooks.partner.com/final");
      });

      it("refuses a second redirect, so a chain cannot be used to wander", async () => {
        const t = transport([redirect("/two"), redirect("/three"), ok(204)]);
        await expect(deliver("https://start.example/one", t)).rejects.toMatchObject({
          code: "REDIRECT_NOT_ALLOWED",
        });
        expect(t.calls).toHaveLength(2);
      });

      it("never follows more than one redirect, even when maxRedirects is higher", async () => {
        const t = transport([redirect("/two"), redirect("/three"), ok(204)]);
        await expect(
          deliver("https://start.example/one", t, { ...basePolicy, maxRedirects: 10 }),
        ).rejects.toMatchObject({ code: "REDIRECT_NOT_ALLOWED" });
        expect(t.calls).toHaveLength(2);
      });

      it("follows no redirect when maxRedirects is 0", async () => {
        const t = transport([redirect("https://public.example/next"), ok(204)]);
        await expect(
          deliver("https://public.example/start", t, { ...basePolicy, maxRedirects: 0 }),
        ).rejects.toMatchObject({ code: "REDIRECT_NOT_ALLOWED" });
        expect(t.calls).toEqual(["https://public.example/start"]);
      });

      it("blocks a redirect downgrade to http and sends nothing to it", async () => {
        const t = transport([redirect("http://public.example/unsafe"), ok(204)]);
        const result = deliver("https://public.example/start", t);

        await expect(result).rejects.toMatchObject({ code: "REDIRECT_NOT_ALLOWED" });
        await expect(result).rejects.toThrow(/https/i);
        expect(t.calls).toEqual(["https://public.example/start"]);
      });

      it.each([
        ["loopback", "https://127.0.0.1/admin"],
        ["cloud metadata", "https://169.254.169.254/latest/meta-data/"],
        ["private 10/8", "https://10.0.0.1/"],
        ["private 192.168/16", "https://192.168.1.1/"],
        ["IPv6 loopback", "https://[::1]/"],
        ["IPv4-mapped loopback", "https://[::ffff:7f00:1]/"],
        ["IPv4-mapped metadata", "https://[::ffff:a9fe:a9fe]/"],
        ["decimal loopback", "https://2130706433/"],
      ])("blocks a redirect to %s and sends nothing to it", async (_note, location) => {
        const t = transport([redirect(location), ok(204)]);
        await expect(deliver("https://public.example/start", t)).rejects.toMatchObject({
          code: "REDIRECT_NOT_ALLOWED",
        });
        expect(t.calls).toEqual(["https://public.example/start"]);
      });

      it.each([
        ["localhost", "https://localhost/hook"],
        ["a .local host", "https://printer.local/hook"],
        ["the GCP metadata host", "https://metadata.google.internal/"],
      ])("blocks a redirect to the denied host %s", async (_note, location) => {
        const t = transport([redirect(location), ok(204)]);
        await expect(deliver("https://public.example/start", t)).rejects.toMatchObject({
          code: "REDIRECT_NOT_ALLOWED",
        });
        expect(t.calls).toHaveLength(1);
      });

      it("blocks a redirect that leaves the allowlist", async () => {
        const t = transport([redirect("https://elsewhere.example/hook"), ok(204)]);
        const policy: WebhookEgressPolicy = {
          ...basePolicy,
          hostAllowRules: ["start.example"],
        };
        await expect(deliver("https://start.example/one", t, policy)).rejects.toMatchObject({
          code: "REDIRECT_NOT_ALLOWED",
        });
        expect(t.calls).toHaveLength(1);
      });

      it("rejects a redirect with no Location header", async () => {
        const t = transport([redirect(undefined), ok(204)]);
        await expect(deliver("https://public.example/start", t)).rejects.toMatchObject({
          code: "REDIRECT_NOT_ALLOWED",
        });
        expect(t.calls).toHaveLength(1);
      });

      it("rejects a redirect whose Location cannot be parsed", async () => {
        const t = transport([redirect("https://[broken"), ok(204)]);
        await expect(deliver("https://public.example/start", t)).rejects.toMatchObject({
          code: "REDIRECT_NOT_ALLOWED",
        });
        expect(t.calls).toHaveLength(1);
      });
    });

    // The real transport (no injected requestImpl) resolves the hostname itself
    // and refuses to connect if any answer is non-public. dns.lookup is stubbed,
    // and https.request is stubbed to throw, so a failure to block would show up
    // as a test failure instead of a real outbound connection.
    describe("DNS rebinding with the real transport", () => {
      type Answer = { address: string; family: number };

      function stubDns(answers: Answer[] | Error) {
        jest.spyOn(dns, "lookup").mockImplementation(((
          _host: string,
          _opts: unknown,
          cb: (err: Error | null, addresses?: Answer[]) => void,
        ) => {
          process.nextTick(() =>
            answers instanceof Error ? cb(answers) : cb(null, answers),
          );
        }) as never);
      }

      const v4 = (address: string): Answer => ({ address, family: 4 });
      const v6 = (address: string): Answer => ({ address, family: 6 });

      it.each([
        ["loopback", [v4("127.0.0.1")]],
        ["private 10/8", [v4("10.0.0.1")]],
        ["private 192.168/16", [v4("192.168.0.10")]],
        ["cloud metadata", [v4("169.254.169.254")]],
        ["CGNAT", [v4("100.64.0.1")]],
        ["IPv6 loopback", [v6("::1")]],
        ["IPv6 unique local", [v6("fd00::1")]],
        ["IPv6 link-local", [v6("fe80::1")]],
        ["IPv4-mapped loopback, dotted", [v6("::ffff:127.0.0.1")]],
        ["IPv4-mapped loopback, hex", [v6("::ffff:7f00:1")]],
        ["IPv4-mapped metadata, hex", [v6("::ffff:a9fe:a9fe")]],
        ["a public answer then a private one", [v4("1.1.1.1"), v4("10.0.0.1")]],
        ["a private answer then a public one", [v4("10.0.0.1"), v4("1.1.1.1")]],
        [
          "a public answer then a hidden mapped one",
          [v4("1.1.1.1"), v6("::ffff:7f00:1")],
        ],
      ])("refuses a hostname that resolves to %s", async (_note, answers) => {
        stubDns(answers as Answer[]);
        await expect(
          deliverWebhookJson("https://rebind.example/hook", {}, basePolicy),
        ).rejects.toMatchObject({ code: "EGRESS_BLOCKED" });
        expect(httpsRequest).not.toHaveBeenCalled();
      });

      it("refuses a hostname with no DNS answers", async () => {
        stubDns([]);
        await expect(
          deliverWebhookJson("https://empty.example/hook", {}, basePolicy),
        ).rejects.toMatchObject({ code: "EGRESS_BLOCKED" });
        expect(httpsRequest).not.toHaveBeenCalled();
      });

      it("reports a failed DNS lookup as a transport error", async () => {
        stubDns(new Error("ENOTFOUND"));
        await expect(
          deliverWebhookJson("https://nope.example/hook", {}, basePolicy),
        ).rejects.toMatchObject({ code: "TRANSPORT_ERROR" });
        expect(httpsRequest).not.toHaveBeenCalled();
      });

      it("looks the hostname up asking for every answer, so none is hidden", async () => {
        stubDns([v4("10.0.0.1")]);
        await expect(
          deliverWebhookJson("https://rebind.example/hook", {}, basePolicy),
        ).rejects.toBeInstanceOf(WebhookDeliveryError);
        expect(dns.lookup).toHaveBeenCalledWith(
          "rebind.example",
          expect.objectContaining({ all: true }),
          expect.any(Function),
        );
      });
    });
  });

  describe("createWebhookSecureLookup", () => {
    type LookupResult = { err: Error | null | undefined; addr: unknown; fam: unknown };

    function lookupWith(answers: unknown, error?: Error) {
      const lookupFn = jest.fn((_h: string, _o: unknown, cb: Function) => {
        process.nextTick(() => (error ? cb(error) : cb(null, answers)));
      });
      const secure = createWebhookSecureLookup(lookupFn as unknown as DnsLookupFn);
      const run = (host = "x.example") =>
        new Promise<LookupResult>((resolve) => {
          secure(host, {}, (err, addr, fam) =>
            resolve({ err: err as Error | null | undefined, addr, fam }),
          );
        });
      return { lookupFn, run };
    }

    it("invokes callback with error when dns lookup fails", async () => {
      const { run } = lookupWith(undefined, new Error("ENOTFOUND"));
      const { err } = await run("nope.example");
      expect(err).toBeInstanceOf(Error);
      expect((err as Error).message).toBe("ENOTFOUND");
    });

    it("blocks when dns returns an empty answer set", async () => {
      const { err } = await lookupWith([]).run();
      expect(err).toBeDefined();
      expect((err as Error).message).toContain("WEBHOOK_DNS_EMPTY");
    });

    it("treats a non-array answer as empty", async () => {
      const { err } = await lookupWith(undefined).run();
      expect((err as Error).message).toContain("WEBHOOK_DNS_EMPTY");
    });

    it.each(BLOCKED_IPV4.map(([range, , ip]) => [range, ip]))(
      "blocks an answer in %s: %s",
      async (_range, ip) => {
        const { err } = await lookupWith([{ address: ip, family: 4 }]).run();
        expect((err as Error).message).toContain("WEBHOOK_EGRESS_BLOCKED");
        expect((err as NodeJS.ErrnoException).code).toBe("EADDRNOTAVAIL");
      },
    );

    it.each(BLOCKED_IPV6.map(([range, , ip]) => [range, ip]))(
      "blocks an IPv6 answer in %s: %s",
      async (_range, ip) => {
        const { err } = await lookupWith([{ address: ip, family: 6 }]).run();
        expect((err as Error).message).toContain("WEBHOOK_EGRESS_BLOCKED");
      },
    );

    it.each(BLOCKED_MAPPED_ROWS)(
      "blocks an IPv4-mapped answer (%s): %s",
      async (_note, ip) => {
        const { err } = await lookupWith([{ address: ip, family: 6 }]).run();
        expect((err as Error).message).toContain("WEBHOOK_EGRESS_BLOCKED");
      },
    );

    it("blocks when any resolved address is non-public, whatever its position", async () => {
      for (const answers of [
        [{ address: "10.0.0.1", family: 4 }],
        [
          { address: "1.1.1.1", family: 4 },
          { address: "10.0.0.1", family: 4 },
        ],
        [
          { address: "10.0.0.1", family: 4 },
          { address: "1.1.1.1", family: 4 },
        ],
      ]) {
        const { err } = await lookupWith(answers).run("internal.example");
        expect((err as Error).message).toContain("WEBHOOK_EGRESS_BLOCKED");
      }
    });

    it("returns no address when it blocks", async () => {
      const { addr, fam } = await lookupWith([{ address: "10.0.0.1", family: 4 }]).run();
      expect(addr).toBe("");
      expect(fam).toBe(0);
    });

    it("returns the first address when all answers are public", async () => {
      const { err, addr, fam } = await lookupWith([
        { address: "1.1.1.1", family: 4 },
        { address: "1.0.0.1", family: 4 },
      ]).run("cloudflare-dns.com");
      expect(err).toBeNull();
      expect(addr).toBe("1.1.1.1");
      expect(fam).toBe(4);
    });

    it("supports IPv6 public answers", async () => {
      const { err, addr, fam } = await lookupWith([
        { address: "2001:4860:4860::8888", family: 6 },
      ]).run("dns.google");
      expect(err).toBeNull();
      expect(addr).toContain("2001");
      expect(fam).toBe(6);
    });

    it("asks the resolver for every answer, so none can be hidden", async () => {
      const { lookupFn, run } = lookupWith([{ address: "1.1.1.1", family: 4 }]);
      await run("host.example");
      expect(lookupFn).toHaveBeenCalledWith(
        "host.example",
        expect.objectContaining({ all: true, verbatim: true }),
        expect.any(Function),
      );
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

    it("reports RESPONSE_TOO_LARGE when the cap is exceeded", async () => {
      const stream = Readable.from([Buffer.alloc(100), Buffer.alloc(100)]);
      await expect(
        readBodyWithByteLimit(asIncomingMessage(stream), 150),
      ).rejects.toMatchObject({ code: "RESPONSE_TOO_LARGE" });
    });

    it("concatenates chunks under the cap", async () => {
      const stream = Readable.from([Buffer.from("a"), Buffer.from("b")]);
      const buf = await readBodyWithByteLimit(asIncomingMessage(stream), 10);
      expect(buf.toString()).toBe("ab");
    });

    it("accepts a body of exactly the cap", async () => {
      const stream = Readable.from([Buffer.alloc(10)]);
      const buf = await readBodyWithByteLimit(asIncomingMessage(stream), 10);
      expect(buf.length).toBe(10);
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
