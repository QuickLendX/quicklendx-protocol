import { Request, Response } from "express";

jest.mock("../services/api-key-service", () => ({ apiKeyService: {} }));
jest.mock("../services/audit-log", () => ({ auditLogService: {} }));
jest.mock("../services/api-key-errors", () => {
  class ApiKeyError extends Error {
    code = "API_KEY_ERROR";
  }
  class ApiKeyNotFoundError extends ApiKeyError {}
  class ApiKeyRevokedError extends ApiKeyError {}
  class ApiKeyRotationConflictError extends ApiKeyError {}
  return { ApiKeyError, ApiKeyNotFoundError, ApiKeyRevokedError, ApiKeyRotationConflictError };
}, { virtual: true });
jest.mock("../services/api-key-error-codes", () => ({
  ApiKeyErrorCode: { INTERNAL: "INTERNAL", NOT_FOUND: "NOT_FOUND", REVOKED: "REVOKED", ROTATION_CONFLICT: "ROTATION_CONFLICT" },
}), { virtual: true });

import { getScopes } from "../controllers/v1/api-keys";

describe("getScopes", () => {
  it("returns the registered scopes and count", async () => {
    const json = jest.fn();
    const response = { json } as unknown as Response;

    await getScopes({} as Request, response);

    expect(json).toHaveBeenCalledWith(
      expect.objectContaining({
        data: expect.any(Array),
        count: expect.any(Number),
      }),
    );
  });

  it("converts a response serialization failure into a stable 500 error", async () => {
    const errorJson = jest.fn();
    const status = jest.fn().mockReturnValue({ json: errorJson });
    const json = jest.fn().mockImplementationOnce(() => {
      throw new Error("serialization failed");
    });
    const response = { json, status } as unknown as Response;
    const errorSpy = jest.spyOn(console, "error").mockImplementation(() => undefined);

    await getScopes({} as Request, response);

    expect(status).toHaveBeenCalledWith(500);
    expect(errorJson).toHaveBeenCalledWith({
      error: {
        message: "Failed to get scopes",
        code: "GET_SCOPES_ERROR",
      },
    });
    expect(errorSpy).toHaveBeenCalled();
    errorSpy.mockRestore();
  });
});
