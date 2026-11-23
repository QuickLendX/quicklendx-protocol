import { isHexId } from "../ids";

describe("isHexId", () => {
  it("accepts hex identifiers", () => {
    expect(isHexId("0x1a")).toBe(true);
    expect(isHexId("0xABCDEF0123456789")).toBe(true);
    expect(isHexId("0x0")).toBe(true);
  });

  it("rejects identifiers that are not hex strings", () => {
    expect(isHexId("REC-1")).toBe(false);
    expect(isHexId("0x")).toBe(false);
    expect(isHexId("0xZZ")).toBe(false);
  });

  it("rejects values that are not strings", () => {
    expect(isHexId(undefined)).toBe(false);
    expect(isHexId(null)).toBe(false);
    expect(isHexId(42)).toBe(false);
    expect(isHexId(["0x1a"])).toBe(false);
  });

  it("requires the 0x prefix", () => {
    expect(isHexId("1a")).toBe(false);
    expect(isHexId("abc123")).toBe(false);
  });
});
