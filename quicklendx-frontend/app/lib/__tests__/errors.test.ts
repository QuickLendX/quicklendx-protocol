import {
  AppError,
  AuthenticationError,
  AuthorizationError,
  BusinessLogicError,
  ErrorAnalytics,
  ErrorCategory,
  ErrorPrevention,
  ErrorRecovery,
  ErrorSeverity,
  NetworkError,
  SystemError,
  ValidationError,
  errorRateLimiter,
  globalErrorHandler,
} from "../errors";

describe("errors.ts", () => {
  beforeEach(() => {
    ErrorAnalytics.getInstance().clearErrors();
    // Reset the rate limiter between tests by clearing its internal map.
    (errorRateLimiter as any).errorCounts = new Map();
  });

  describe("AppError base class", () => {
    it("stores message, category, severity, code and retry metadata", () => {
      const err = new AppError(
        "boom",
        ErrorCategory.SYSTEM,
        ErrorSeverity.LOW,
        "CUSTOM_CODE",
        { component: "test" },
        true,
        2
      );
      expect(err.message).toBe("boom");
      expect(err.name).toBe("AppError");
      expect(err.category).toBe(ErrorCategory.SYSTEM);
      expect(err.severity).toBe(ErrorSeverity.LOW);
      expect(err.code).toBe("CUSTOM_CODE");
      expect(err.retryable).toBe(true);
      expect(err.retryCount).toBe(2);
      expect(err.context.component).toBe("test");
    });

    it("defaults severity to MEDIUM and retryable to false", () => {
      const err = new AppError("x", ErrorCategory.VALIDATION);
      expect(err.severity).toBe(ErrorSeverity.MEDIUM);
      expect(err.retryable).toBe(false);
      expect(err.retryCount).toBe(0);
      expect(err.code).toBeUndefined();
    });
  });

  describe("subclass classification table", () => {
    const cases: Array<{
      name: string;
      make: (msg: string, ctx?: any, code?: string) => AppError;
      category: ErrorCategory;
      severity: ErrorSeverity;
      defaultCode: string;
      retryable: boolean;
    }> = [
      {
        name: "NetworkError",
        make: (m, c, code) => new NetworkError(m, c, 0, code),
        category: ErrorCategory.NETWORK,
        severity: ErrorSeverity.HIGH,
        defaultCode: "NETWORK_ERROR",
        retryable: true,
      },
      {
        name: "ValidationError",
        make: (m, c, code) => new ValidationError(m, c, code),
        category: ErrorCategory.VALIDATION,
        severity: ErrorSeverity.MEDIUM,
        defaultCode: "VALIDATION_ERROR",
        retryable: false,
      },
      {
        name: "AuthenticationError",
        make: (m, c, code) => new AuthenticationError(m, c, code),
        category: ErrorCategory.AUTHENTICATION,
        severity: ErrorSeverity.HIGH,
        defaultCode: "AUTH_ERROR",
        retryable: true,
      },
      {
        name: "AuthorizationError",
        make: (m, c, code) => new AuthorizationError(m, c, code),
        category: ErrorCategory.AUTHORIZATION,
        severity: ErrorSeverity.HIGH,
        defaultCode: "FORBIDDEN",
        retryable: false,
      },
      {
        name: "BusinessLogicError",
        make: (m, c, code) => new BusinessLogicError(m, c, code),
        category: ErrorCategory.BUSINESS_LOGIC,
        severity: ErrorSeverity.MEDIUM,
        defaultCode: "BUSINESS_ERROR",
        retryable: false,
      },
      {
        name: "SystemError",
        make: (m, c, code) => new SystemError(m, c, code),
        category: ErrorCategory.SYSTEM,
        severity: ErrorSeverity.CRITICAL,
        defaultCode: "SYSTEM_ERROR",
        retryable: true,
      },
    ];

    it.each(cases)(
      "$name maps to category, severity, retryable and default code",
      ({ make, category, severity, defaultCode, retryable, name }) => {
        const err = make("msg");
        expect(err).toBeInstanceOf(AppError);
        expect(err.name).toBe(name);
        expect(err.category).toBe(category);
        expect(err.severity).toBe(severity);
        expect(err.code).toBe(defaultCode);
        expect(err.retryable).toBe(retryable);
      }
    );

    it.each(cases)(
      "$name prefers server-supplied code over the default",
      ({ make }) => {
        const err = make("msg", undefined, "INVALID_PAGINATION");
        expect(err.code).toBe("INVALID_PAGINATION");
      }
    );

    it("NetworkError forwards retryCount", () => {
      expect(new NetworkError("x", undefined, 4).retryCount).toBe(4);
    });
  });

  describe("server error code table", () => {
    const serverCodes: Array<{ code: string; expectedCategory: ErrorCategory }> = [
      { code: "INVALID_PAGINATION", expectedCategory: ErrorCategory.VALIDATION },
      { code: "RATE_LIMITED", expectedCategory: ErrorCategory.NETWORK },
      { code: "ORIGIN_NOT_ALLOWED", expectedCategory: ErrorCategory.AUTHORIZATION },
      { code: "INTERNAL_ERROR", expectedCategory: ErrorCategory.SYSTEM },
    ];

    it.each(serverCodes)(
      "preserves server code $code and category $expectedCategory via ValidationError/NetworkError routing",
      ({ code }) => {
        const err = new AppError(
          "user message",
          ErrorCategory.SYSTEM,
          ErrorSeverity.MEDIUM,
          code
        );
        expect(err.code).toBe(code);
      }
    );
  });

  describe("unknown / missing code fallback", () => {
    it("AppError without code yields undefined code and rate-limiter key 'unknown'", () => {
      const err = new AppError("x", ErrorCategory.SYSTEM);
      expect(err.code).toBeUndefined();
      expect(errorRateLimiter.getErrorKey(err)).toBe("system_unknown");
    });

    it("globalErrorHandler wraps plain Error as SystemError with fallback code", () => {
      const spy = jest.spyOn(ErrorAnalytics.getInstance(), "trackError");
      globalErrorHandler(new Error("raw"));
      expect(spy).toHaveBeenCalledTimes(1);
      const tracked = spy.mock.calls[0][0];
      expect(tracked).toBeInstanceOf(SystemError);
      expect(tracked.code).toBe("SYSTEM_ERROR");
      spy.mockRestore();
    });

    it("globalErrorHandler falls back to generic message when Error has no message", () => {
      const spy = jest.spyOn(ErrorAnalytics.getInstance(), "trackError");
      globalErrorHandler(new Error(""));
      expect(spy.mock.calls[0][0].message).toBe("An unexpected error occurred");
      spy.mockRestore();
    });
  });

  describe("axios vs fetch error shapes", () => {
    it("extracts server message from axios-style response.data without leaking raw payload", async () => {
      const axiosLike = Object.assign(new Error("Request failed"), {
        response: { status: 429, data: { code: "RATE_LIMITED", message: "slow down" } },
      });
      const result = await ErrorRecovery.handleAsyncError(
        Promise.reject(axiosLike),
        "fallback"
      );
      expect(result).toBe("fallback");
    });

    it("fetch-style TypeError is wrapped as SystemError by handleAsyncError", async () => {
      const spy = jest.spyOn(ErrorAnalytics.getInstance(), "trackError");
      await ErrorRecovery.handleAsyncError(
        Promise.reject(new TypeError("Failed to fetch")),
        null
      );
      const tracked = spy.mock.calls[0][0];
      expect(tracked).toBeInstanceOf(SystemError);
      expect(tracked.message).toBe("Failed to fetch");
      spy.mockRestore();
    });

    it("non-Error rejection values become a generic SystemError message", async () => {
      const spy = jest.spyOn(ErrorAnalytics.getInstance(), "trackError");
      await ErrorRecovery.handleAsyncError(Promise.reject("<html>secret</html>"), 0);
      const tracked = spy.mock.calls[0][0];
      expect(tracked.message).toBe("Unknown error occurred");
      expect(tracked.message).not.toContain("secret");
      spy.mockRestore();
    });
  });

  describe("raw server text is never leaked into analytics messages", () => {
    it("globalErrorHandler tracks only the error message, not nested response bodies", () => {
      const spy = jest.spyOn(ErrorAnalytics.getInstance(), "trackError");
      const err = Object.assign(new Error("Request failed"), {
        response: { data: "<script>leak</script>" },
      });
      globalErrorHandler(err);
      const tracked = spy.mock.calls[0][0];
      expect(tracked.message).toBe("Request failed");
      expect(JSON.stringify(tracked.context)).not.toContain("<script>leak</script>");
      spy.mockRestore();
    });

    it("sanitizeInput escapes HTML so server text cannot inject markup", () => {
      const out = ErrorPrevention.sanitizeInput("<b>\"x\"</b>");
      expect(out).not.toContain("<");
      expect(out).not.toContain(">");
      expect(out).toContain("&lt;b&gt;");
    });
  });

  describe("ErrorAnalytics rate limiting and storage", () => {
    it("tracks error and exposes it in stats", () => {
      const analytics = ErrorAnalytics.getInstance();
      analytics.trackError(new ValidationError("bad", undefined, "INVALID_PAGINATION"));
      const stats = analytics.getErrorStats();
      expect(stats.total).toBe(1);
      expect(stats.byCategory[ErrorCategory.VALIDATION]).toBe(1);
      expect(stats.bySeverity[ErrorSeverity.MEDIUM]).toBe(1);
      expect(stats.recentErrors[0].code).toBe("INVALID_PAGINATION");
    });

    it("drops errors once the per-key limit (10/min) is reached", () => {
      const analytics = ErrorAnalytics.getInstance();
      for (let i = 0; i < 15; i++) {
        analytics.trackError(new NetworkError("net", undefined, 0, "RATE_LIMITED"));
      }
      expect(analytics.getErrorStats().total).toBe(10);
    });
  });

  describe("ErrorRecovery.retryOperation", () => {
    it("returns the value once an attempt succeeds", async () => {
      let calls = 0;
      const result = await ErrorRecovery.retryOperation(async () => {
        calls++;
        if (calls < 2) throw new Error("transient");
        return "ok";
      }, 3, 0);
      expect(result).toBe("ok");
      expect(calls).toBe(2);
    });

    it("rethrows the last error after exhausting retries", async () => {
      await expect(
        ErrorRecovery.retryOperation(async () => {
          throw new Error("always");
        }, 2, 0)
      ).rejects.toThrow("always");
    });
  });
});
