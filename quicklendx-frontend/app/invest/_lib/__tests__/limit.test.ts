import { pLimit } from "../limit";

describe("pLimit concurrency-limited promise pool", () => {
  it("never runs more than 4 tasks at once across 20 tasks, and yields results in submission order", async () => {
    const limit = pLimit(4);
    let activeCounter = 0;
    let maxCounter = 0;

    const tasks = Array.from({ length: 20 }, (_, index) => {
      return limit(async () => {
        activeCounter++;
        if (activeCounter > maxCounter) {
          maxCounter = activeCounter;
        }

        // simulate async delay
        await new Promise((resolve) => setTimeout(resolve, 20));

        activeCounter--;
        return `result_${index}`;
      });
    });

    const results = await Promise.all(tasks);

    // Assert max concurrency never exceeded 4
    expect(maxCounter).toBeLessThanOrEqual(4);
    expect(maxCounter).toBe(4);

    // Assert submission order preserved
    const expected = Array.from(
      { length: 20 },
      (_, index) => `result_${index}`
    );
    expect(results).toEqual(expected);
  });

  it("when a task rejects, only its own promise rejects while others run and resolve", async () => {
    const limit = pLimit(4);

    const tasks = Array.from({ length: 20 }, (_, index) => {
      return limit(async () => {
        await new Promise((resolve) => setTimeout(resolve, 10));
        if (index === 7) {
          throw new Error("Task 7 failed intentionally");
        }
        return `result_${index}`;
      });
    });

    const settled = await Promise.allSettled(tasks);

    expect(settled.length).toBe(20);

    settled.forEach((res, index) => {
      if (index === 7) {
        expect(res.status).toBe("rejected");
        if (res.status === "rejected") {
          expect(res.reason.message).toBe("Task 7 failed intentionally");
        }
      } else {
        expect(res.status).toBe("fulfilled");
        if (res.status === "fulfilled") {
          expect(res.value).toBe(`result_${index}`);
        }
      }
    });
  });

  it("throws error if concurrency < 1", () => {
    expect(() => pLimit(0)).toThrow("Concurrency limit must be at least 1");
  });
});
