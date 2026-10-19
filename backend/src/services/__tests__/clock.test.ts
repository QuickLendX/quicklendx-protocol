import { describe, it, expect } from "vitest";
import { FrozenClock, SystemClock } from "../clock";

describe("FrozenClock", () => {
  it("returns the configured start time and advances on delay", () => {
    const clock = new FrozenClock(1_000);
    expect(clock.now()).toBe(1_000);
    // delay() records the requested backoff and advances virtual time.
    void clock.delay(100);
    expect(clock.now()).toBe(1_100);
    expect(clock.scheduledDelays).toEqual([100]);
  });

  it("accumulates scheduled backoffs in order", async () => {
    const clock = new FrozenClock(0);
    await clock.delay(50);
    await clock.delay(150);
    await clock.delay(300);
    expect(clock.scheduledDelays).toEqual([50, 150, 300]);
    expect(clock.now()).toBe(500);
  });

  it("rejects negative delays and rejects advancing backwards", () => {
    const clock = new FrozenClock(1000);
    expect(() => clock.delay(-1)).toThrow(RangeError);
    expect(() => clock.advance(-10)).toThrow(RangeError);
  });

  it("advance() moves the virtual clock forward in time", () => {
    const clock = new FrozenClock(2_000);
    clock.advance(500);
    expect(clock.now()).toBe(2_500);
    clock.advance(300);
    expect(clock.now()).toBe(2_800);
  });

  it("reset restores the start time and clears the schedule", () => {
    const clock = new FrozenClock(1_000);
    void clock.delay(200);
    clock.reset(5_000);
    expect(clock.now()).toBe(5_000);
    expect(clock.scheduledDelays).toHaveLength(0);
  });

  it("delay resolves asynchronously (yields to the microtask queue)", async () => {
    const clock = new FrozenClock();
    let stepped = false;
    const p = clock.delay(0).then(() => {
      stepped = true;
    });
    // Before awaiting, the .then callback has not run yet.
    expect(stepped).toBe(false);
    await p;
    expect(stepped).toBe(true);
  });
});

describe("SystemClock", () => {
  it("now() approximates real epoch time and delay resolves", async () => {
    const clock = new SystemClock();
    const before = Date.now();
    expect(clock.now()).toBeGreaterThanOrEqual(before);
    await expect(clock.delay(0)).resolves.toBeUndefined();
  });
});
