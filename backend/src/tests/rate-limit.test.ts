describe("rate-limit configuration", () => {
  const originalNodeEnv = process.env.NODE_ENV;

  afterEach(() => {
    jest.resetModules();
    process.env.NODE_ENV = originalNodeEnv;
  });

  it("uses test-specific point budget in test env", async () => {
    process.env.NODE_ENV = "test";
    const { rateLimiter } = await import("../middleware/rate-limit");
    expect((rateLimiter as any)._points).toBe(1000);
  });

  it("uses production/default point budget outside test env", async () => {
    process.env.NODE_ENV = "production";
    const { rateLimiter } = await import("../middleware/rate-limit");
    expect((rateLimiter as any)._points).toBe(100);
  });
});
