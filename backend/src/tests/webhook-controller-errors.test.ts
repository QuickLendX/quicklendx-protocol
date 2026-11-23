import type { Request, Response, NextFunction } from "express";
import * as controllers from "../controllers/v1/webhooks";
import { errorHandler } from "../middleware/error-handler";
import { webhookSecretService, WebhookSecretError } from "../services/webhookSecretService";

const operations = [
  ["registerSubscriber", "registerSubscriber", 409, "SUBSCRIBER_ALREADY_EXISTS"],
  ["getSubscriber", "getSubscriberView", 404, "SUBSCRIBER_NOT_FOUND"],
  ["initiateRotation", "initiateRotation", 409, "ROTATION_ALREADY_IN_PROGRESS"],
  ["finalizeRotation", "finalizeRotation", 500, "ROTATION_STATE_INCONSISTENT"],
  ["cancelRotation", "cancelRotation", 409, "NO_ROTATION_IN_PROGRESS"],
] as const;

function context(headersSent = false) {
  const req = { body: { subscriber_id: "subscriber" }, params: { subscriberId: "subscriber" } } as unknown as Request;
  const json = jest.fn().mockReturnThis();
  const status = jest.fn().mockReturnThis();
  const res = { headersSent, status, json } as unknown as Response;
  const next = jest.fn();
  return { req, res, next, status, json };
}

afterEach(() => jest.restoreAllMocks());

describe.each(operations)("%s error boundary", (controller, method, statusCode, code) => {
  it("responds exactly once with only the domain error's public fields", async () => {
    const error = Object.assign(new WebhookSecretError("Operation rejected", code, statusCode), {
      details: { primary_secret: "private-test-secret" },
    });
    jest.spyOn(webhookSecretService, method).mockImplementation(() => { throw error; });
    const { req, res, next, status, json } = context();

    await controllers[controller](req, res, next);

    expect(status).toHaveBeenCalledTimes(1);
    expect(status).toHaveBeenCalledWith(statusCode);
    expect(json).toHaveBeenCalledTimes(1);
    expect(json).toHaveBeenCalledWith({ error: { message: "Operation rejected", code } });
    expect(next).not.toHaveBeenCalled();
  });

  it("forwards unexpected errors unchanged without writing a response", async () => {
    const error = new Error("Storage unavailable");
    jest.spyOn(webhookSecretService, method).mockImplementation(() => { throw error; });
    const { req, res, next, status, json } = context();

    await controllers[controller](req, res, next);

    expect(next).toHaveBeenCalledTimes(1);
    expect(next).toHaveBeenCalledWith(error);
    expect(status).not.toHaveBeenCalled();
    expect(json).not.toHaveBeenCalled();
  });

  it("delegates a domain error after headers are sent instead of sending again", async () => {
    const error = new WebhookSecretError("Operation rejected", code, statusCode);
    jest.spyOn(webhookSecretService, method).mockImplementation(() => { throw error; });
    const { req, res, next, status, json } = context(true);

    await controllers[controller](req, res, next);

    expect(next).toHaveBeenCalledTimes(1);
    expect(next).toHaveBeenCalledWith(error);
    expect(status).not.toHaveBeenCalled();
    expect(json).not.toHaveBeenCalled();
  });
});

describe("Express error/control-flow boundary", () => {
  it.each([undefined, null, false, 0, NaN, "", "route", "router"])(
    "turns thrown %p into a diagnosable error rather than Express routing control",
    async (value) => {
      jest.spyOn(webhookSecretService, "getSubscriberView").mockImplementation(() => { throw value; });
      const { req, res, next, json } = context();

      await controllers.getSubscriber(req, res, next);

      expect(next).toHaveBeenCalledTimes(1);
      expect(next.mock.calls[0][0]).toBeInstanceOf(Error);
      expect(next.mock.calls[0][0]).toMatchObject({
        message: "Unexpected webhook controller failure",
        code: "WEBHOOK_CONTROLLER_ERROR",
      });
      expect(json).not.toHaveBeenCalled();
    }
  );

  it("does not mistake an error-shaped object for a trusted domain error", async () => {
    const error = { status: 403, code: "DENIED", message: "Rejected" };
    jest.spyOn(webhookSecretService, "getSubscriberView").mockImplementation(() => { throw error; });
    const { req, res, next, json } = context();

    await controllers.getSubscriber(req, res, next);

    expect(next).toHaveBeenCalledWith(error);
    expect(json).not.toHaveBeenCalled();
  });

  it("handles an ingest response failure through the same boundary", async () => {
    const { req, res, next, json } = context();
    json.mockImplementationOnce(() => { throw "route"; });

    await controllers.ingestWebhook(req, res, next);

    expect(next).toHaveBeenCalledTimes(1);
    expect(next.mock.calls[0][0]).toBeInstanceOf(Error);
    expect(next.mock.calls[0][0].code).toBe("WEBHOOK_CONTROLLER_ERROR");
    expect(json).toHaveBeenCalledTimes(1);
  });

  it("lets Express finish an already-started response in the global handler", () => {
    const { req, res, next, status, json } = context(true);
    const error = new Error("Connection failed");
    const log = jest.spyOn(console, "error").mockImplementation(() => {});

    errorHandler(error, req, res, next as NextFunction);

    expect(next).toHaveBeenCalledWith(error);
    expect(status).not.toHaveBeenCalled();
    expect(json).not.toHaveBeenCalled();
    expect(log).not.toHaveBeenCalled();
  });
});
