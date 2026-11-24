import { isIP } from "node:net";

/**
 * Returns true when the resolved IP must not be used for outbound webhooks
 * (RFC1918, loopback, link-local, CGNAT, multicast, documentation ranges, etc.).
 */
export function isBlockedDestinationIP(ip: string): boolean {
  const kind = isIP(ip);
  if (kind === 4) return isBlockedIPv4(ip);
  if (kind === 6) return isBlockedIPv6(ip);
  return true;
}

/** True when every resolved address is a public unicast IP (non-empty input). */
export function areAllDnsResultsPublicForWebhook(
  addresses: { address: string }[],
): boolean {
  if (addresses.length === 0) return false;
  for (const a of addresses) {
    if (isBlockedDestinationIP(a.address)) return false;
  }
  return true;
}

function ipv4ToUint32(s: string): number | null {
  const parts = s.split(".");
  if (parts.length !== 4) return null;
  let n = 0;
  for (const p of parts) {
    if (!/^\d{1,3}$/.test(p)) return null;
    const o = Number(p);
    if (o > 255) return null;
    n = (n << 8) + o;
  }
  return n >>> 0;
}

function isBlockedIPv4(ip: string): boolean {
  const n = ipv4ToUint32(ip);
  if (n === null) return true;

  const first = n >>> 24;
  const second = (n >>> 16) & 0xff;
  const third = (n >>> 8) & 0xff;

  if (first === 0 || first === 127) return true;
  if (first === 10) return true;
  if (first === 100 && second >= 64 && second <= 127) return true;
  if (first === 169 && second === 254) return true;
  if (first === 172 && second >= 16 && second <= 31) return true;
  if (first === 192 && second === 168) return true;
  if (first === 192 && second === 0 && (third === 0 || third === 2)) return true;
  if (first === 192 && second === 88 && third === 99) return true;
  if (first === 198 && second === 18) return true;
  if (first === 198 && second === 51 && third === 100) return true;
  if (first === 203 && second === 0 && third === 113) return true;
  if (first >= 224 && first <= 239) return true;
  if (n === 0xffffffff) return true;

  return false;
}

/**
 * Parse an IPv6 literal into its eight 16-bit groups, or `null` if it cannot be
 * parsed. Handles `::` compression, an embedded dotted-quad tail
 * (`::ffff:127.0.0.1`), any letter case and a trailing `%zone`.
 *
 * Every notation of the same address has to land on the same eight groups,
 * because the checks below are on the numeric value. Matching the text instead
 * is how `::ffff:127.0.0.1` was caught but `::ffff:7f00:1` (the same address,
 * and the form `new URL()` rewrites it to) slipped through.
 */
function parseIPv6Groups(ip: string): number[] | null {
  let s = ip.toLowerCase();
  const zone = s.indexOf("%");
  if (zone !== -1) s = s.slice(0, zone);

  const lastColon = s.lastIndexOf(":");
  if (lastColon === -1) return null;
  const tail = s.slice(lastColon + 1);
  if (tail.includes(".")) {
    const v4 = ipv4ToUint32(tail);
    if (v4 === null) return null;
    s =
      s.slice(0, lastColon + 1) +
      ((v4 >>> 16) & 0xffff).toString(16) +
      ":" +
      (v4 & 0xffff).toString(16);
  }

  const halves = s.split("::");
  if (halves.length > 2) return null;
  const head = halves[0] ? halves[0].split(":") : [];
  let groups: string[];
  if (halves.length === 2) {
    const rest = halves[1] ? halves[1].split(":") : [];
    const missing = 8 - head.length - rest.length;
    if (missing < 1) return null;
    groups = [...head, ...new Array<string>(missing).fill("0"), ...rest];
  } else {
    groups = head;
  }
  if (groups.length !== 8) return null;

  const out: number[] = [];
  for (const g of groups) {
    if (!/^[0-9a-f]{1,4}$/.test(g)) return null;
    out.push(parseInt(g, 16));
  }
  return out;
}

function isBlockedIPv6(ip: string): boolean {
  const groups = parseIPv6Groups(ip);
  // `net.isIP` accepted it as IPv6 but we cannot read it: fail closed.
  if (groups === null) return true;

  // ::/96 covers the unspecified address (::, which connects to localhost on
  // Linux), loopback (::1) and the deprecated IPv4-compatible form (::a.b.c.d).
  if (groups.slice(0, 6).every((g) => g === 0)) return true;

  // ::ffff:0:0/96 IPv4-mapped: the address is only as public as the IPv4 it
  // carries, in whichever notation it was written (dotted, hex, expanded).
  if (groups.slice(0, 5).every((g) => g === 0) && groups[5] === 0xffff) {
    const v4 = `${groups[6] >> 8}.${groups[6] & 0xff}.${groups[7] >> 8}.${groups[7] & 0xff}`;
    return isBlockedIPv4(v4);
  }

  // Multicast. Kept as the original textual test, which also covers every
  // group starting "ff"; narrowing it to ff00::/8 would loosen the blocklist.
  if (ip.toLowerCase().startsWith("ff")) return true;

  const first = groups[0];
  if (first >= 0xfe80 && first <= 0xfebf) return true; // fe80::/10 link-local
  if (first >= 0xfc00 && first <= 0xfdff) return true; // fc00::/7 unique local

  return false;
}
