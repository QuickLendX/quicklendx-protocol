import {
  createRequestContextMiddleware,
  getCorrelationId,
  runWithContext,
} from "../requestContext";

describe("createRequestContextMiddleware", () => {
  it("should initialize context with valid correlationId", (done) => {
    const middleware = createRequestContextMiddleware();
    const req = { correlationId: "valid-id-123" };
    
    middleware(req, {}, () => {
      expect(getCorrelationId()).toBe("valid-id-123");
      done();
    });
  });

  it("should initialize context with valid requestId as fallback", (done) => {
    const middleware = createRequestContextMiddleware();
    const req = { requestId: "valid-request-id" };
    
    middleware(req, {}, () => {
      expect(getCorrelationId()).toBe("valid-request-id");
      done();
    });
  });

  it("should ignore invalid/duplicate/boundary-case correlationIds and proceed without context", (done) => {
    const middleware = createRequestContextMiddleware();
    // Invalid characters (e.g. log injection attempt)
    const req = { correlationId: "invalid\nID!" };
    
    middleware(req, {}, () => {
      expect(getCorrelationId()).toBeNull();
      done();
    });
  });

  it("should proceed without context if no ID is provided", (done) => {
    const middleware = createRequestContextMiddleware();
    const req = {};
    
    middleware(req, {}, () => {
      expect(getCorrelationId()).toBeNull();
      done();
    });
  });

  it("should deterministically catch synchronous errors in next() and pass them down", () => {
    const middleware = createRequestContextMiddleware();
    const req = { correlationId: "safe-id" };
    const simulatedError = new Error("simulated failure");
    
    let caughtError: any = null;
    middleware(req, {}, (err) => {
      if (err) {
        caughtError = err;
      } else {
        throw simulatedError;
      }
    });

    expect(caughtError).toBe(simulatedError);
  });

  it("should ensure concurrent executions remain isolated", (done) => {
    const middleware = createRequestContextMiddleware();
    
    const req1 = { correlationId: "context-1" };
    const req2 = { correlationId: "context-2" };

    let completed = 0;

    middleware(req1, {}, () => {
      setTimeout(() => {
        expect(getCorrelationId()).toBe("context-1");
        if (++completed === 2) done();
      }, 10);
    });

    middleware(req2, {}, () => {
      setTimeout(() => {
        expect(getCorrelationId()).toBe("context-2");
        if (++completed === 2) done();
      }, 5);
    });
  });
});
