import {
  computeImpliedReturn,
  formatAtomicAmount,
  isValidHexInvoiceId,
} from "../math";

describe("formatAtomicAmount", () => {
  it.each([
    ["zero at the default scale", "0", undefined, "0.0000000"],
    ["negative zero at scale zero", "-000", 0, "0"],
    ["negative sub-unit amount", "-12", 7, "-0.0000012"],
    ["leading zeros", "00012345678", 7, "1.2345678"],
    [
      "integer above Number.MAX_SAFE_INTEGER",
      "9007199254740993",
      0,
      "9007199254740993",
    ],
    [
      "scaled integer above Number.MAX_SAFE_INTEGER",
      "9007199254740993",
      7,
      "900719925.4740993",
    ],
    ["non-integer input", " 12.5 ", 7, "12.5"],
    ["non-numeric input", " invalid ", 7, "invalid"],
  ])("formats %s exactly", (_label, amount, decimals, expected) => {
    expect(formatAtomicAmount(amount, decimals)).toBe(expected);
  });
});

describe("computeImpliedReturn", () => {
  it.each([
    ["zero bid", "0", "100", "—"],
    ["negative bid", "-1", "100", "—"],
    ["invalid return", "100", "invalid", "—"],
    ["return equals bid", "100", "100", "0.00%"],
    ["positive return", "100", "105", "5.00%"],
    ["return below bid", "100", "95", "-5.00%"],
    ["fractional basis point", "3", "4", "33.33%"],
    ["large exact ratio", "9007199254740993", "18014398509481986", "100.00%"],
    [
      "large ratio just below 100%",
      "9007199254740993",
      "18014398509481985",
      "99.99%",
    ],
  ])("handles %s", (_label, bid, expectedReturn, expected) => {
    expect(computeImpliedReturn(bid, expectedReturn)).toBe(expected);
  });
});

describe("isValidHexInvoiceId", () => {
  it.each([
    ["0x0", true],
    ["0xaB09", true],
    ["0x", false],
    ["ab09", false],
    ["0xg", false],
    [" 0xa", false],
  ])("validates %j", (id, expected) => {
    expect(isValidHexInvoiceId(id)).toBe(expected);
  });
});
