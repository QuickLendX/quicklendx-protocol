import * as dns from "node:dns";
import type { RequestOptions } from "node:https";
import { areAllDnsResultsPublicForWebhook } from "./blockedAddress";

export type DnsLookupFn = typeof dns.lookup;

/**
 * Custom `lookup` for `https.Agent` that rejects hostnames unless every
 * resolved address is a public unicast IP (mitigates DNS rebinding).
 */
export function createWebhookSecureLookup(
  lookupFn: DnsLookupFn = dns.lookup,
): NonNullable<RequestOptions["lookup"]> {
  return (hostname, _opts, callback) => {
    lookupFn(hostname, { all: true, verbatim: true }, (err, addresses) => {
      if (err) {
        callback(err, "", 0);
        return;
      }
      if (!Array.isArray(addresses) || addresses.length === 0) {
        callback(
          Object.assign(new Error("WEBHOOK_DNS_EMPTY"), {
            code: "ENOTFOUND" as const,
          }),
          "",
          0,
        );
        return;
      }
      if (!areAllDnsResultsPublicForWebhook(addresses)) {
        callback(
          Object.assign(new Error("WEBHOOK_EGRESS_BLOCKED"), {
            code: "EADDRNOTAVAIL" as const,
          }),
          "",
          0,
        );
        return;
      }
      const first = addresses[0];
      callback(null, first.address, first.family === 6 ? 6 : 4);
    });
  };
}
