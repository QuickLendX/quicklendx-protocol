import request from "supertest";

describe("CORS and CSRF hardening", () => {
  const allowedOrigin = "https://app.quicklendx.test";
  const disallowedOrigin = "https://evil.example";

  const loadV1App = async () => {
    jest.resetModules();
    process.env.ALLOWED_ORIGINS = allowedOrigin;
    const appModule = await import("../src/app");
    return appModule.default;
  };

  const loadStatusApp = async () => {
    jest.resetModules();
    process.env.ALLOWED_ORIGINS = allowedOrigin;
    const appModule = await import("../src/index");
    return appModule.default;
  };

  describe("CORS policy", () => {
    it("allows configured browser origins", async () => {
      const app = await loadV1App();
      const res = await request(app)
        .get("/api/v1/health")
        .set("Origin", allowedOrigin);

      expect(res.status).toBe(200);
      expect(res.headers["access-control-allow-origin"]).toBe(allowedOrigin);
    });

    it("denies untrusted browser origins", async () => {
      const app = await loadV1App();
      const res = await request(app)
        .get("/api/v1/health")
        .set("Origin", disallowedOrigin);

      expect(res.status).toBe(200);
      expect(res.headers["access-control-allow-origin"]).toBeUndefined();
    });

    it("supports safe preflight for allowed browser origins", async () => {
      const app = await loadV1App();
      const res = await request(app)
        .options("/api/v1/health")
        .set("Origin", allowedOrigin)
        .set("Access-Control-Request-Method", "GET");

      expect(res.status).toBe(204);
      expect(res.headers["access-control-allow-origin"]).toBe(allowedOrigin);
      expect(res.headers["access-control-allow-methods"]).toContain("GET");
    });

    it("rejects preflight for disallowed origins", async () => {
      const app = await loadV1App();
      const res = await request(app)
        .options("/api/v1/health")
        .set("Origin", disallowedOrigin)
        .set("Access-Control-Request-Method", "GET");

      expect(res.status).toBe(200);
      expect(res.headers["access-control-allow-origin"]).toBeUndefined();
    });

    it("allows requests without Origin header", async () => {
      const app = await loadV1App();
      const res = await request(app).get("/api/v1/health");

      expect(res.status).toBe(200);
    });
  });

  describe("CSRF protections", () => {
    it("blocks state-changing requests that are not JSON", async () => {
      const statusApp = await loadStatusApp();
      const res = await request(statusApp)
        .post("/api/admin/maintenance")
        .set("Origin", allowedOrigin)
        .set("Content-Type", "text/plain")
        .send("enabled=true");

      expect(res.status).toBe(415);
      expect(res.body.error.code).toBe("INVALID_CONTENT_TYPE");
    });

    it("blocks state-changing requests without a content type", async () => {
      const statusApp = await loadStatusApp();
      const res = await request(statusApp)
        .post("/api/admin/maintenance")
        .set("Origin", allowedOrigin);

      expect(res.status).toBe(415);
      expect(res.body.error.code).toBe("INVALID_CONTENT_TYPE");
    });

    it("allows JSON state-changing requests from trusted origins", async () => {
      const statusApp = await loadStatusApp();
      const res = await request(statusApp)
        .post("/api/admin/maintenance")
        .set("Origin", allowedOrigin)
        .send({ enabled: true });

      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.maintenance).toBe(true);
    });

    it("blocks state-changing requests from untrusted origins", async () => {
      const statusApp = await loadStatusApp();
      const res = await request(statusApp)
        .post("/api/admin/maintenance")
        .set("Origin", disallowedOrigin)
        .send({ enabled: true });

      expect(res.status).toBe(403);
      expect(res.body.error.code).toBe("ORIGIN_NOT_ALLOWED");
    });

    it("allows trusted JSON requests without Origin header", async () => {
      const statusApp = await loadStatusApp();
      const res = await request(statusApp)
        .post("/api/admin/maintenance")
        .send({ enabled: false });

      expect(res.status).toBe(200);
      expect(res.body.success).toBe(true);
      expect(res.body.maintenance).toBe(false);
    });
  });

  describe("Webhook route separation", () => {
    it("accepts webhook callbacks separately from browser API routes", async () => {
      const app = await loadV1App();
      const webhookRes = await request(app)
        .post("/api/webhooks/callbacks")
        .set("Origin", disallowedOrigin)
        .send({ event: "invoice.updated" });

      expect(webhookRes.status).toBe(202);
      expect(webhookRes.body.accepted).toBe(true);
    });

    it("restricts webhook callback endpoint to POST", async () => {
      const app = await loadV1App();
      const res = await request(app).get("/api/webhooks/callbacks");

      expect(res.status).toBe(405);
      expect(res.body.error.code).toBe("METHOD_NOT_ALLOWED");
    });

    it("does not expose webhooks under v1 browser routes", async () => {
      const app = await loadV1App();
      const res = await request(app).post("/api/v1/webhooks/callbacks").send({});

      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe("NOT_FOUND");
    });
  });
});
