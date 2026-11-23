/**
 * Load-shedding concurrency-cap test.
 *
 * Asserts that under saturation (active requests == CONCURRENCY_CAP),
 * new requests receive 503 with a Retry-After header.
 */

import express, { Request, Response } from "express";
import request from "supertest";
import {
  loadSheddingMiddleware,
  CONCURRENCY_CAP,
  RETRY_AFTER_SECONDS,
  resetActiveRequests,
} from "../middleware/load-shedding";

// Build a minimal express app with a slow handler to hold concurrency slots open.
function makeApp(handlerDelayMs = 0) {
  const app = express();
  app.use(loadSheddingMiddleware);

  app.get("/slow", (_req: Request, res: Response) => {
    setTimeout(() => res.json({ ok: true }), handlerDelayMs);
  });

  app.get("/fast", (_req: Request, res: Response) => {
    res.json({ ok: true });
  });

  return app;
}

afterEach(() => {
  resetActiveRequests();
});

describe("load-shedding concurrency cap", () => {
  it("serves requests normally when under the cap", async () => {
    const app = makeApp(0);
    const res = await request(app).get("/fast");
    expect(res.status).toBe(200);
    expect(res.body.ok).toBe(true);
  });

  it("returns 503 with Retry-After when the concurrency cap is saturated", async () => {
    const app = makeApp(300); // hold slots open for 300 ms

    // Fire CONCURRENCY_CAP slow requests to saturate the cap.
    const saturating = Array.from({ length: CONCURRENCY_CAP }, () =>
      request(app).get("/slow")
    );

    // Give the slow requests a moment to enter the middleware before firing the overflow.
    await new Promise((r) => setTimeout(r, 20));

    const overflow = await request(app).get("/fast");

    expect(overflow.status).toBe(503);
    expect(overflow.headers["retry-after"]).toBe(String(RETRY_AFTER_SECONDS));
    expect(overflow.body.code).toBe("CONCURRENCY_CAP");

    // Wait for slow requests to finish so port/fd cleanup is clean.
    await Promise.allSettled(saturating);
  });

  it("recovers and serves new requests after the cap drains", async () => {
    const app = makeApp(50);

    // Saturate briefly.
    const saturating = Array.from({ length: CONCURRENCY_CAP }, () =>
      request(app).get("/slow")
    );

    await new Promise((r) => setTimeout(r, 20));

    const overflow = await request(app).get("/fast");
    expect(overflow.status).toBe(503);

    // Wait for slots to drain.
    await Promise.allSettled(saturating);
    await new Promise((r) => setTimeout(r, 10));

    // Now the cap is drained — should succeed.
    const recovered = await request(app).get("/fast");
    expect(recovered.status).toBe(200);
  });
});
