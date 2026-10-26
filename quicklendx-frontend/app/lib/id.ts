/** Create a browser-safe RFC 4122 version 4 identifier. */
export function newId(prefix?: string): string {
  const cryptoApi = globalThis.crypto;
  if (!cryptoApi) {
    throw new Error("secure random source unavailable");
  }

  let id: string;
  if (typeof cryptoApi.randomUUID === "function") {
    id = cryptoApi.randomUUID();
  } else if (typeof cryptoApi.getRandomValues === "function") {
    const bytes = cryptoApi.getRandomValues(new Uint8Array(16));
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = Array.from(bytes, (byte) =>
      byte.toString(16).padStart(2, "0")
    ).join("");
    id = `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  } else {
    throw new Error("secure random source unavailable");
  }

  return prefix ? `${prefix}_${id}` : id;
}
