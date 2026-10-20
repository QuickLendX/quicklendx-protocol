import { Response } from "express";
import {
  apiKeyAuth,
  loadApiKeys,
  resetApiKeys,
  getKeyMapSize,
  AuthenticatedRequest,
} from "../middleware/apiKeyAuth";

describe("apiKeyAuth middleware & resetApiKeys failure boundaries", () => {
  const next = jest.fn();

  const buildRes = () => {
    const res: Partial<Response> = {};
    res.status = jest.fn().mockReturnValue(res as Response);
    res.json = jest.fn().mockReturnValue(res as Response);
    return res as Response;
  };

  beforeEach(() => {
    next.mockReset();
    resetApiKeys();
    delete process.env.ADMIN_API_KEYS;
    delete process.env.SKIP_API_KEY_AUTH;
    delete process.env.TEST_ACTOR;
  });

  afterAll(() => {
    resetApiKeys();
  });

  describe("resetApiKeys and loadApiKeys deterministic lifecycle", () => {
    it("clears cached keys on resetApiKeys()", () => {
      process.env.ADMIN_API_KEYS = "secret-key-1:admin-actor,secret-key-2:ops-actor";
      loadApiKeys();
      expect(getKeyMapSize()).toBe(2);

      resetApiKeys();
      expect(getKeyMapSize()).toBe(0);
    });

    it("allows reloading updated keys after resetApiKeys() without stale keys", () => {
      process.env.ADMIN_API_KEYS = "key-v1:actor-v1";
      loadApiKeys();
      expect(getKeyMapSize()).toBe(1);

      resetApiKeys();
      process.env.ADMIN_API_KEYS = "key-v2:actor-v2,key-v3:actor-v3";
      loadApiKeys();
      expect(getKeyMapSize()).toBe(2);
    });

    it("handles malformed, empty or whitespace entries in ADMIN_API_KEYS gracefully", () => {
      process.env.ADMIN_API_KEYS = " , valid-key:actor-1, malformed-no-colon , :no-key, no-actor: , ";
      loadApiKeys();
      expect(getKeyMapSize()).toBe(1);
    });
  });

  describe("apiKeyAuth authentication & error boundaries", () => {
    it("returns 401 UNAUTHORIZED when X-API-Key header is missing", () => {
      process.env.ADMIN_API_KEYS = "valid-key:admin";
      const req = { header: jest.fn().mockReturnValue(undefined) } as unknown as AuthenticatedRequest;
      const res = buildRes();

      apiKeyAuth(req, res, next);

      expect(res.status).toHaveBeenCalledWith(401);
      expect(res.json).toHaveBeenCalledWith({
        error: {
          message: "Missing X-API-Key header",
          code: "UNAUTHORIZED",
        },
      });
      expect(next).not.toHaveBeenCalled();
    });

    it("returns 401 UNAUTHORIZED when X-API-Key does not match any configured key", () => {
      process.env.ADMIN_API_KEYS = "valid-key:admin";
      const req = { header: jest.fn().mockReturnValue("invalid-key") } as unknown as AuthenticatedRequest;
      const res = buildRes();

      apiKeyAuth(req, res, next);

      expect(res.status).toHaveBeenCalledWith(401);
      expect(res.json).toHaveBeenCalledWith({
        error: {
          message: "Invalid API key",
          code: "UNAUTHORIZED",
        },
      });
      expect(next).not.toHaveBeenCalled();
    });

    it("authenticates and attaches actor to request for valid key", () => {
      process.env.ADMIN_API_KEYS = "valid-key:admin-user";
      const req = { header: jest.fn().mockReturnValue("valid-key") } as unknown as AuthenticatedRequest;
      const res = buildRes();

      apiKeyAuth(req, res, next);

      expect(req.actor).toBe("admin-user");
      expect(next).toHaveBeenCalledTimes(1);
      expect(res.status).not.toHaveBeenCalled();
    });

    it("bypasses authentication when SKIP_API_KEY_AUTH is true", () => {
      process.env.SKIP_API_KEY_AUTH = "true";
      process.env.TEST_ACTOR = "custom-test-actor";
      const req = { header: jest.fn() } as unknown as AuthenticatedRequest;
      const res = buildRes();

      apiKeyAuth(req, res, next);

      expect(req.actor).toBe("custom-test-actor");
      expect(next).toHaveBeenCalledTimes(1);
      expect(req.header).not.toHaveBeenCalled();
    });

    it("falls back to default 'test-actor' when SKIP_API_KEY_AUTH is true and TEST_ACTOR unset", () => {
      process.env.SKIP_API_KEY_AUTH = "true";
      const req = { header: jest.fn() } as unknown as AuthenticatedRequest;
      const res = buildRes();

      apiKeyAuth(req, res, next);

      expect(req.actor).toBe("test-actor");
      expect(next).toHaveBeenCalledTimes(1);
    });

    it("catches runtime exceptions and returns clean 500 error boundary", () => {
      const faultyReq = {
        header: jest.fn().mockImplementation(() => {
          throw new Error("Simulated unexpected request inspection fault");
        }),
      } as unknown as AuthenticatedRequest;
      const res = buildRes();

      apiKeyAuth(faultyReq, res, next);

      expect(res.status).toHaveBeenCalledWith(500);
      expect(res.json).toHaveBeenCalledWith({
        error: {
          message: "Internal authentication error",
          code: "INTERNAL_ERROR",
        },
      });
      expect(next).not.toHaveBeenCalled();
    });
  });
});
