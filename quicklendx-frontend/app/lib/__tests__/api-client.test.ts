/**
 * Unit tests for app/lib/api-client.ts
 *
 * Acceptance criteria verified:
 *  AC1 – baseURL uses /api/v1 when NEXT_PUBLIC_API_URL is unset
 *  AC2 – POST/PUT/PATCH/DELETE send X-CSRF-Token and Content-Type; GET does not
 *  AC3 – 400 body { error: { message, code } } → ValidationError with matching .message/.code
 *  AC4 – 422 body with details → ValidationError, details preserved in context.additionalData.details
 *  AC5 – 500 with non-JSON body → SystemError with generic message, no throw inside handler
 *  AC6 – ECONNABORTED → NetworkError with timeout message
 *  AC7 – upload() rejects immediately without sending a request
 */

// ---------------------------------------------------------------------------
// Mock axios BEFORE any imports so that when api-client.ts is imported below
// it receives the mock. The mock instance is defined here at module level so
// we can reference it later to extract installed interceptors.
// ---------------------------------------------------------------------------

const mockAxiosInstance = {
  interceptors: {
    request: { use: jest.fn() },
    response: { use: jest.fn() },
  },
  request: jest.fn(),
};

jest.mock("axios", () => {
  const actual = jest.requireActual("axios");
  return {
    ...actual,
    create: jest.fn(() => mockAxiosInstance),
    isAxiosError: actual.isAxiosError,
  };
});

// ---------------------------------------------------------------------------
// Import the module under test.
//
// This executes the module body including `new ApiClient()`, which calls
// axios.create (our mock) and then installs two interceptors via
// mockAxiosInstance.interceptors.{request,response}.use().
//
// The interceptor callbacks are captured once at this point.  We MUST NOT
// call jest.clearAllMocks() before reading them, because clearAllMocks resets
// .mock.calls on every mock function.
// ---------------------------------------------------------------------------

import { _resetCsrfToken, getCsrfToken, apiClient } from "../api-client";
import axios from "axios";

// Capture interceptor callbacks installed during module initialisation.
// These are stable for the lifetime of the test file.
const requestOnFulfilled: (cfg: any) => any =
  mockAxiosInstance.interceptors.request.use.mock.calls[0][0];

const responseOnRejected: (err: any) => Promise<never> =
  mockAxiosInstance.interceptors.response.use.mock.calls[0][1];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Builds a minimal AxiosError-like object. */
function makeAxiosError(opts: {
  code?: string;
  request?: any;
  response?: { status: number; data: any; headers?: Record<string, any> };
  config?: Record<string, any>;
}): any {
  return {
    isAxiosError: true,
    code: opts.code,
    config: opts.config ?? { method: "GET", url: "/test", timeout: 10000 },
    request: opts.request,
    response: opts.response,
    message: "axios error",
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("ApiClient", () => {
  beforeEach(() => {
    // Reset only what's needed between tests; DO NOT clear the interceptor mocks.
    _resetCsrfToken();
    mockAxiosInstance.request.mockClear();
  });

  // -------------------------------------------------------------------------
  // AC1 – baseURL defaults to http://localhost:3001/api/v1
  // -------------------------------------------------------------------------
  describe("AC1 – baseURL", () => {
    it("passes http://localhost:3001/api/v1 to axios.create when NEXT_PUBLIC_API_URL is unset", () => {
      const createMock = axios.create as jest.Mock;
      // The single axios.create call happened when the module was imported.
      // (NEXT_PUBLIC_API_URL is not set in the test environment.)
      expect(createMock).toHaveBeenCalledWith(
        expect.objectContaining({
          baseURL: "http://localhost:3001/api/v1",
        })
      );
    });
  });

  // -------------------------------------------------------------------------
  // AC2 – CSRF token + Content-Type on write methods; GET is clean
  // -------------------------------------------------------------------------
  describe("AC2 – CSRF token + Content-Type on write methods", () => {
    const writeMethods = ["post", "put", "patch", "delete"];

    it.each(writeMethods)(
      "%s adds X-CSRF-Token and Content-Type: application/json",
      (method) => {
        const cfg = {
          method,
          headers: {} as Record<string, string>,
          url: "/x",
        };
        const result = requestOnFulfilled(cfg);

        expect(result.headers["X-CSRF-Token"]).toBeTruthy();
        expect(result.headers["Content-Type"]).toBe("application/json");
      }
    );

    it("GET does NOT set X-CSRF-Token", () => {
      const cfg = { method: "get", headers: {}, url: "/invoices" };
      const result = requestOnFulfilled(cfg);
      expect(result.headers["X-CSRF-Token"]).toBeUndefined();
    });

    it("GET does NOT set Content-Type via the CSRF branch", () => {
      const cfg = { method: "get", headers: {}, url: "/invoices" };
      const result = requestOnFulfilled(cfg);
      expect(result.headers["Content-Type"]).toBeUndefined();
    });

    it("CSRF token is the same non-empty value across multiple write requests in the same session", () => {
      _resetCsrfToken();
      const r1 = requestOnFulfilled({ method: "post", headers: {}, url: "/a" });
      const r2 = requestOnFulfilled({ method: "put", headers: {}, url: "/b" });

      expect(r1.headers["X-CSRF-Token"]).toBeTruthy();
      expect(r1.headers["X-CSRF-Token"]).toBe(r2.headers["X-CSRF-Token"]);
    });

    it("getCsrfToken() returns a stable non-empty token across calls", () => {
      _resetCsrfToken();
      const t1 = getCsrfToken();
      const t2 = getCsrfToken();
      expect(t1).toBeTruthy();
      expect(t1).toBe(t2);
    });
  });

  // -------------------------------------------------------------------------
  // AC3 – 400 ErrorEnvelope → ValidationError with correct message + code
  // -------------------------------------------------------------------------
  describe("AC3 – 400 with ErrorEnvelope", () => {
    it("extracts message and code from { error: { message, code } }", async () => {
      const err = makeAxiosError({
        response: {
          status: 400,
          data: {
            error: { message: "Invalid cursor", code: "INVALID_PAGINATION" },
          },
          headers: {},
        },
      });

      let caught: any;
      try {
        await responseOnRejected(err);
      } catch (e) {
        caught = e;
      }

      expect(caught).toBeDefined();
      expect(caught.name).toBe("ValidationError");
      expect(caught.message).toBe("Invalid cursor");
      expect(caught.code).toBe("INVALID_PAGINATION");
    });
  });

  // -------------------------------------------------------------------------
  // AC4 – 422 with details → ValidationError, details in additionalData.details
  // -------------------------------------------------------------------------
  describe("AC4 – 422 with details", () => {
    it("preserves details in context.additionalData.details", async () => {
      const err = makeAxiosError({
        response: {
          status: 422,
          data: {
            error: {
              message: "Invalid amount",
              code: "VALIDATION_ERROR",
              details: { field: "amount" },
            },
          },
          headers: {},
        },
      });

      let caught: any;
      try {
        await responseOnRejected(err);
      } catch (e) {
        caught = e;
      }

      expect(caught).toBeDefined();
      expect(caught.name).toBe("ValidationError");
      expect(caught.message).toBe("Invalid amount");
      expect(caught.code).toBe("VALIDATION_ERROR");
      expect(caught.context.additionalData?.details).toEqual({
        field: "amount",
      });
    });
  });

  // -------------------------------------------------------------------------
  // AC5 – 500 with non-JSON body → SystemError with generic message
  // -------------------------------------------------------------------------
  describe("AC5 – 500 with non-JSON (HTML) body", () => {
    it("returns a SystemError with generic message for HTML body", async () => {
      const err = makeAxiosError({
        response: {
          status: 500,
          data: "<html><body>Internal Server Error</body></html>",
          headers: {},
        },
      });

      let caught: any;
      try {
        await responseOnRejected(err);
      } catch (e) {
        caught = e;
      }

      expect(caught).toBeDefined();
      expect(caught.name).toBe("SystemError");
      expect(caught.message).toBe("Server error. Please try again later.");
    });

    it("does not throw synchronously inside the handler for null body", async () => {
      const err = makeAxiosError({
        response: { status: 500, data: null, headers: {} },
      });

      let caught: any;
      try {
        await responseOnRejected(err);
      } catch (e) {
        caught = e;
      }

      expect(caught).toBeDefined();
      expect(caught.name).toBe("SystemError");
    });
  });

  // -------------------------------------------------------------------------
  // AC6 – ECONNABORTED → NetworkError with timeout message
  // -------------------------------------------------------------------------
  describe("AC6 – timeout (ECONNABORTED / ETIMEDOUT)", () => {
    it("ECONNABORTED maps to NetworkError with timeout message", async () => {
      // request is set — this is the crux: without the fix, the !response&&request
      // branch fires first and returns "Network connection failed" instead.
      const err = makeAxiosError({
        code: "ECONNABORTED",
        request: {},
        response: undefined,
      });

      let caught: any;
      try {
        await responseOnRejected(err);
      } catch (e) {
        caught = e;
      }

      expect(caught).toBeDefined();
      expect(caught.name).toBe("NetworkError");
      expect(caught.message).toBe("Request timed out. Please try again.");
    });

    it("ETIMEDOUT maps to NetworkError with timeout message", async () => {
      const err = makeAxiosError({
        code: "ETIMEDOUT",
        request: {},
        response: undefined,
      });

      let caught: any;
      try {
        await responseOnRejected(err);
      } catch (e) {
        caught = e;
      }

      expect(caught).toBeDefined();
      expect(caught.name).toBe("NetworkError");
      expect(caught.message).toBe("Request timed out. Please try again.");
    });

    it("ECONNABORTED does NOT produce the generic 'Network connection failed' message", async () => {
      const err = makeAxiosError({
        code: "ECONNABORTED",
        request: {},
        response: undefined,
      });

      let caught: any;
      try {
        await responseOnRejected(err);
      } catch (e) {
        caught = e;
      }

      expect(caught?.message).not.toContain("Network connection failed");
    });
  });

  // -------------------------------------------------------------------------
  // AC7 – upload() rejects locally, no HTTP request sent
  // -------------------------------------------------------------------------
  describe("AC7 – upload() rejected locally", () => {
    it("rejects with ValidationError before sending any request", async () => {
      const fakeFile = {} as File;

      let caught: any;
      try {
        await apiClient.upload("/upload", fakeFile);
      } catch (e) {
        caught = e;
      }

      expect(caught).toBeDefined();
      expect(caught.name).toBe("ValidationError");
      expect(caught.message).toBe("Uploads are not supported by the v1 API");

      // No underlying HTTP request must have been dispatched
      expect(mockAxiosInstance.request).not.toHaveBeenCalled();
    });
  });
});
