import { parseAlertRoutes } from "../config";

describe("parseAlertRoutes failure boundaries", () => {
  it("returns undefined routes when the setting is absent", () => {
    expect(parseAlertRoutes(undefined)).toEqual({ routes: undefined });
  });

  it("parses valid routes without changing their order", () => {
    expect(
      parseAlertRoutes(
        JSON.stringify({
          routes: [
            { severity: "HIGH", channels: ["slack", "pagerduty"] },
            { severity: "LOW", channels: ["email"] },
          ],
        })
      )
    ).toEqual({
      routes: [
        { severity: "HIGH", channels: ["slack", "pagerduty"] },
        { severity: "LOW", channels: ["email"] },
      ],
    });
  });

  it.each([
    ["malformed JSON", "{not-json"],
    ["a JSON primitive", "null"],
    ["a JSON array", "[]"],
    ["a non-array routes value", JSON.stringify({ routes: {} })],
    ["a route with no channels", JSON.stringify({ routes: [{ severity: "HIGH", channels: [] }] })],
    ["an unsupported severity", JSON.stringify({ routes: [{ severity: "CRITICAL", channels: ["email"] }] })],
    ["an unsupported channel", JSON.stringify({ routes: [{ severity: "HIGH", channels: ["sms"] }] })],
    ["duplicate severities", JSON.stringify({ routes: [
      { severity: "HIGH", channels: ["slack"] },
      { severity: "HIGH", channels: ["email"] },
    ] })],
    ["duplicate channels", JSON.stringify({ routes: [{ severity: "HIGH", channels: ["slack", "slack"] }] })],
  ])("rejects %s with a stable error", (_label, raw) => {
    expect(() => parseAlertRoutes(raw)).toThrow("Invalid ALERT_ROUTES_JSON configuration");
  });

  it("does not include the raw setting in the thrown error", () => {
    const raw = JSON.stringify({ routes: [{ severity: "SECRET_VALUE", channels: ["email"] }] });
    let thrown: unknown;
    try {
      parseAlertRoutes(raw);
    } catch (error) {
      thrown = error;
    }

    expect(thrown).toBeInstanceOf(Error);
    expect((thrown as Error).message).toBe("Invalid ALERT_ROUTES_JSON configuration");
    expect((thrown as Error).message).not.toContain("SECRET_VALUE");
  });
});
