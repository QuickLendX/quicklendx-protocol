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

function parseIPv4TailFromIPv6(ip: string): string | null {
  const lower = ip.toLowerCase();
  const idx = lower.lastIndexOf(":");
  if (idx === -1) return null;
  const tail = ip.slice(idx + 1);
  if (!tail || tail.includes(":")) return null;
  if (isIP(tail) !== 4) return null;
  return tail;
}

function isBlockedIPv6(ip: string): boolean {
  const lower = ip.toLowerCase();

  if (lower === "::1") return true;

  if (lower.startsWith("ff")) return true;

  const first16 = parseIPv6FirstHextet(lower);
  if (first16 !== null && first16 >= 0xfe80 && first16 <= 0xfebf) return true;
  if (first16 !== null && first16 >= 0xfc00 && first16 <= 0xfdff) return true;

  if (lower.startsWith("::ffff:")) {
    const mapped = parseIPv4TailFromIPv6(lower);
    if (mapped) return isBlockedIPv4(mapped);
  }

  return false;
}

function parseIPv6FirstHextet(ip: string): number | null {
  const head = ip.split(":")[0];
  if (!head) return null;
  if (!/^[0-9a-fA-F]{1,4}$/.test(head)) return null;
  return parseInt(head, 16);
}
