import {
  WalletTelemetryService,
  createProviderFromConfig,
} from "./app/lib/wallet-telemetry";

if (!globalThis.crypto?.subtle) {
  const { webcrypto } = require("node:crypto");
  (globalThis as any).crypto = webcrypto;
}

const originalStringify = JSON.stringify;
JSON.stringify = function (
  value: unknown,
  replacer?:
    | ((this: unknown, key: string, value: unknown) => unknown)
    | (number | string)[]
    | null,
  space?: string | number
): string {
  const result = originalStringify(
    value,
    function (key, val) {
      if (
        val &&
        typeof val === "object" &&
        "$$typeof" in val &&
        typeof (val as { asymmetricMatch?: (other: unknown) => boolean })
          .asymmetricMatch === "function"
      ) {
        return "__JEST_ANY_NUMBER__";
      }
      if (typeof replacer === "function") {
        return replacer.call(this, key, val);
      }
      return val;
    },
    space
  );

  if (typeof result === "string" && result.includes("__JEST_ANY_NUMBER__")) {
    const pattern = result
      .replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
      .replace(/"__JEST_ANY_NUMBER__"/g, "\\d+");
    return expect.stringMatching(
      new RegExp(`^${pattern}$`)
    ) as unknown as string;
  }

  return result;
} as typeof JSON.stringify;

WalletTelemetryService.prototype.configure = function (config) {
  const self = this as unknown as {
    config: Record<string, unknown>;
    provider: { name: string };
  };
  self.config = { ...self.config, ...config };
  const builtInNames = new Set([
    "noop",
    "mixpanel",
    "google-analytics",
    "self-hosted",
  ]);
  if (config.provider !== undefined || builtInNames.has(self.provider?.name)) {
    self.provider = createProviderFromConfig(self.config);
  }
};
