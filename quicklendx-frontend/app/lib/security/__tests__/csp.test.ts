import nextConfig from "../../../../next.config";
import {
  buildCsp,
  buildCspHeader,
  getApiOrigin,
  CSP_HEADER,
  CSP_REPORT_ONLY_HEADER,
} from "../csp";

const productionEnv = {
  NODE_ENV: "production",
  NEXT_PUBLIC_API_URL: "https://api.example.com/api/v1",
};

describe("buildCsp", () => {
  it("builds the baseline policy from the API origin", () => {
    const policy = buildCsp(productionEnv);

    expect(policy).toContain("default-src 'self'");
    expect(policy).toContain(
      "connect-src 'self' https://api.example.com https://*.stellar.org"
    );
    expect(policy).toContain("img-src 'self' data:");
    expect(policy).toContain("frame-ancestors 'self'");
    expect(policy).toContain("object-src 'none'");
  });

  it("extracts the origin from an API URL with a path and query", () => {
    expect(getApiOrigin("https://api.example.com:8443/api/v1?x=1")).toBe(
      "https://api.example.com:8443"
    );
  });

  it("uses the localhost API origin when NEXT_PUBLIC_API_URL is unset", () => {
    expect(getApiOrigin(undefined)).toBe("http://localhost:3001");
    expect(buildCsp({ NODE_ENV: "production" })).toContain(
      "connect-src 'self' http://localhost:3001 https://*.stellar.org"
    );
  });

  it("omits an invalid API origin instead of injecting the raw value", () => {
    expect(getApiOrigin("not a url")).toBeNull();
    expect(
      buildCsp({ ...productionEnv, NEXT_PUBLIC_API_URL: "not a url" })
    ).toContain("connect-src 'self' https://*.stellar.org");
  });

  it("adds unsafe-eval only in development", () => {
    const developmentPolicy = buildCsp({
      ...productionEnv,
      NODE_ENV: "development",
    });
    const productionPolicy = buildCsp(productionEnv);

    expect(developmentPolicy).toContain("script-src 'self' 'unsafe-eval'");
    expect(productionPolicy).not.toContain("unsafe-eval");
    expect(developmentPolicy.replace(" 'unsafe-eval'", "")).toBe(
      productionPolicy
    );
  });

  it("defaults to report-only and switches to enforce mode via CSP_ENFORCE", () => {
    expect(buildCspHeader(productionEnv)).toEqual({
      key: CSP_REPORT_ONLY_HEADER,
      value: buildCsp(productionEnv),
    });
    expect(buildCspHeader({ ...productionEnv, CSP_ENFORCE: "true" }).key).toBe(
      CSP_HEADER
    );
    expect(buildCspHeader({ ...productionEnv, CSP_ENFORCE: "1" }).key).toBe(
      CSP_HEADER
    );
  });
});

describe("next.config security headers", () => {
  it("sends a CSP header on every route", async () => {
    const rules = await nextConfig.headers?.();
    const headers = rules?.[0]?.headers ?? [];
    const cspHeader = headers.find(
      (header) =>
        header.key === CSP_REPORT_ONLY_HEADER || header.key === CSP_HEADER
    );

    expect(cspHeader).toBeDefined();
    expect(cspHeader?.value).toBe(buildCsp(process.env));
  });

  it("honours CSP_ENFORCE when building headers", async () => {
    const originalEnforce = process.env.CSP_ENFORCE;
    process.env.CSP_ENFORCE = "true";

    try {
      const rules = await nextConfig.headers?.();
      const headers = rules?.[0]?.headers ?? [];

      expect(headers).toEqual(
        expect.arrayContaining([
          { key: CSP_HEADER, value: buildCsp(process.env) },
        ])
      );
    } finally {
      if (originalEnforce === undefined) {
        delete process.env.CSP_ENFORCE;
      } else {
        process.env.CSP_ENFORCE = originalEnforce;
      }
    }
  });
});
