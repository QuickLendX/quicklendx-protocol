import { load, envBoolean } from "../config";

const baseEnv = { NODE_ENV: "development" } as NodeJS.ProcessEnv;

describe("envBoolean", () => {
  it.each([
    ["true", true],
    ["1", true],
    ["false", false],
    ["0", false],
  ])("parses %s as %s", (input, expected) => {
    expect(envBoolean.parse(input)).toBe(expected);
  });

  it.each(["yes", "no", "", "TRUE", "on"])("rejects %p", (input) => {
    expect(envBoolean.safeParse(input).success).toBe(false);
  });
});

describe("ARCHIVE_ENABLED via load()", () => {
  it("defaults to false when unset", () => {
    expect(load(baseEnv).ARCHIVE_ENABLED).toBe(false);
  });

  it("yields false for ARCHIVE_ENABLED=false", () => {
    expect(load({ ...baseEnv, ARCHIVE_ENABLED: "false" }).ARCHIVE_ENABLED).toBe(false);
  });

  it("yields false for ARCHIVE_ENABLED=0", () => {
    expect(load({ ...baseEnv, ARCHIVE_ENABLED: "0" }).ARCHIVE_ENABLED).toBe(false);
  });

  it("yields true for ARCHIVE_ENABLED=true", () => {
    expect(load({ ...baseEnv, ARCHIVE_ENABLED: "true" }).ARCHIVE_ENABLED).toBe(true);
  });

  it("yields true for ARCHIVE_ENABLED=1", () => {
    expect(load({ ...baseEnv, ARCHIVE_ENABLED: "1" }).ARCHIVE_ENABLED).toBe(true);
  });

  it("rejects ARCHIVE_ENABLED=yes and names only the field", () => {
    expect(() => load({ ...baseEnv, ARCHIVE_ENABLED: "yes" })).toThrow(
      "Invalid configuration: ARCHIVE_ENABLED",
    );
    try {
      load({ ...baseEnv, ARCHIVE_ENABLED: "yes" });
    } catch (err) {
      expect((err as Error).message).not.toContain("yes");
    }
  });
});
