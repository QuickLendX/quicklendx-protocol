import express from "express";
import supertest from "supertest";
import {
  apiKeyAuth,
  AuthenticatedRequest,
  resetApiKeys,
} from "./apiKeyAuth";

describe("apiKeyAuth reset boundary", () => {
  const originalKeys = process.env.ADMIN_API_KEYS;
  const originalSkip = process.env.SKIP_API_KEY_AUTH;

  const app = express();
  app.get("/protected", apiKeyAuth, (req, res) => {
    res.json({ actor: (req as AuthenticatedRequest).actor });
  });

  beforeEach(() => {
    delete process.env.SKIP_API_KEY_AUTH;
    resetApiKeys();
  });

  afterAll(() => {
    if (originalKeys === undefined) delete process.env.ADMIN_API_KEYS;
    else process.env.ADMIN_API_KEYS = originalKeys;
    if (originalSkip === undefined) delete process.env.SKIP_API_KEY_AUTH;
    else process.env.SKIP_API_KEY_AUTH = originalSkip;
    resetApiKeys();
  });

  it("discards cached credentials and loads the current configuration", async () => {
    process.env.ADMIN_API_KEYS = "old-key:old-actor";
    const oldKeyResponse = await supertest(app)
      .get("/protected")
      .set("X-API-Key", "old-key");
    expect(oldKeyResponse.status).toBe(200);
    expect(oldKeyResponse.body).toEqual({ actor: "old-actor" });

    process.env.ADMIN_API_KEYS = "new-key:new-actor";
    resetApiKeys();

    const staleKeyResponse = await supertest(app)
      .get("/protected")
      .set("X-API-Key", "old-key");
    expect(staleKeyResponse.status).toBe(401);
    expect(staleKeyResponse.body.error.code).toBe("UNAUTHORIZED");

    const refreshedKeyResponse = await supertest(app)
      .get("/protected")
      .set("X-API-Key", "new-key");
    expect(refreshedKeyResponse.status).toBe(200);
    expect(refreshedKeyResponse.body).toEqual({ actor: "new-actor" });
  });

  it("rejects missing and invalid keys after reset without calling the handler", async () => {
    process.env.ADMIN_API_KEYS = "valid-key:actor";
    resetApiKeys();

    const missing = await supertest(app).get("/protected");
    expect(missing.status).toBe(401);
    expect(missing.body.error.message).toBe("Missing X-API-Key header");

    const invalid = await supertest(app)
      .get("/protected")
      .set("X-API-Key", "invalid-key");
    expect(invalid.status).toBe(401);
    expect(invalid.body.error.message).toBe("Invalid API key");
  });

  it("keeps an empty configuration empty across reset and rejects all keys", async () => {
    process.env.ADMIN_API_KEYS = "";
    resetApiKeys();

    for (const key of ["", "unconfigured"]) {
      const request = supertest(app).get("/protected");
      const response = key ? await request.set("X-API-Key", key) : await request;
      expect(response.status).toBe(401);
    }
  });
});
