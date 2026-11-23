import { describe, it, expect } from "vitest";
import * as api from "../index";

/** Smoke test for the package's public surface. Ensures the barrel exports are
 *  stable (no accidental removal/breakage of the public interface). */
describe("public API surface", () => {
  it("exports the controller and failure-boundary infrastructure", () => {
    expect(api.ApiKeysController).toBeInstanceOf(Function);
    expect(api.FailureBoundary).toBeInstanceOf(Function);
  });

  it("exports domain errors and types", () => {
    expect(api.ApiKeyError).toBeInstanceOf(Function);
    expect(api.isApiKeyError(new api.ApiKeyError("NOT_FOUND", "x", {}))).toBe(
      true
    );
    expect(
      api.apiKeyRetryPredicate(new api.ApiKeyError("TRANSIENT", "x", {}))
    ).toBe(true);
    expect(
      api.apiKeyRetryPredicate(new api.ApiKeyError("VALIDATION_ERROR", "x", {}))
    ).toBe(false);
  });

  it("exports services and the clock abstraction", () => {
    expect(api.InMemoryApiKeyService).toBeInstanceOf(Function);
    expect(api.FrozenClock).toBeInstanceOf(Function);
    expect(api.SystemClock).toBeInstanceOf(Function);
    expect(api.AuthorizationServiceFactory.ownerBased).toBeInstanceOf(Function);
  });
});
