import nextConfig, { securityHeaders } from "../../next.config";

describe("Next.js security configuration", () => {
  it("disables the X-Powered-By header", () => {
    expect(nextConfig.poweredByHeader).toBe(false);
  });

  it("includes the expected security header values", () => {
    expect(securityHeaders).toEqual(
      expect.arrayContaining([
        {
          key: "Permissions-Policy",
          value: "camera=(), microphone=(), geolocation=()",
        },
        {
          key: "Strict-Transport-Security",
          value: "max-age=63072000; includeSubDomains",
        },
      ])
    );
  });

  it("applies the security headers to every route", async () => {
    const headerRules = await nextConfig.headers?.();

    expect(headerRules).toEqual([
      {
        source: "/:path*",
        headers: securityHeaders,
      },
    ]);
  });
});
