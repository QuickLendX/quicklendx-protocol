import { newId } from "../id";

const originalCrypto = globalThis.crypto;

function setCrypto(value: Crypto | undefined): void {
  Object.defineProperty(globalThis, "crypto", {
    configurable: true,
    value,
  });
}

afterEach(() => {
  setCrypto(originalCrypto);
});

describe("newId", () => {
  it("uses randomUUID when available and applies the optional prefix", () => {
    setCrypto({ randomUUID: () => "u" } as Crypto);

    expect(newId("req")).toBe("req_u");
  });

  it("creates unique RFC 4122 v4 IDs with getRandomValues", () => {
    let sequence = 0;
    setCrypto({
      getRandomValues<T extends ArrayBufferView>(array: T): T {
        const bytes = array as unknown as Uint8Array;
        sequence += 1;
        bytes[0] = sequence & 0xff;
        bytes[1] = (sequence >>> 8) & 0xff;
        return array;
      },
    } as Crypto);

    const ids = Array.from({ length: 1000 }, () => newId());
    const pattern =
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

    expect(ids.every((id) => pattern.test(id))).toBe(true);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("fails clearly when no secure random source is available", () => {
    setCrypto(undefined);

    expect(() => newId()).toThrow("secure random source unavailable");
  });
});
