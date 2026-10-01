// Deterministic failure-boundary coverage for `isPromise`.
//
// `isPromise` is the branch selector inside `withSpan`: it decides whether a
// wrapped function returned a value to await or a value to return synchronously.
// Because it runs on every traced call, it must be total (never throw), free of
// side effects (never invoke `then`), and stable for a given input.
import { isPromise } from "./tracing";

describe("isPromise deterministic failure boundaries", () => {
  it.each([undefined, null, false, 0, -0, Number.NaN, 0n, "", true, 1, 3.14, "then"])(
    "treats falsy and primitive values as non-promises: %p",
    (value) => {
      expect(isPromise(value)).toBe(false);
      expect(isPromise(value)).toBe(false);
    },
  );

  it.each([{}, { a: 1 }, [], [1, 2, 3], () => {}, function named() {}, new Date(0), /x/])(
    "treats objects, arrays and functions without a callable then as non-promises: %p",
    (value) => {
      expect(isPromise(value)).toBe(false);
    },
  );

  it.each([
    { then: undefined },
    { then: null },
    { then: 0 },
    { then: "" },
    { then: "then" },
    { then: {} },
    { then: [] },
    { then: 1n },
  ])(
    "treats an object whose then property is not callable as a non-promise: %p",
    (value) => {
      expect(isPromise(value)).toBe(false);
      expect(isPromise(value)).toBe(false);
    },
  );

  it("treats a callable then as a promise-like even without Promise internals", () => {
    expect(isPromise({ then: () => {} })).toBe(true);
    expect(isPromise({ then(resolve: () => void) { resolve(); } })).toBe(true);
    expect(
      isPromise(Object.assign(function thenable() {}, { then: () => {} })),
    ).toBe(true);
  });

  it("treats real promises, pending promises and async results as promises", async () => {
    expect(isPromise(Promise.resolve(1))).toBe(true);
    expect(isPromise(Promise.reject(new Error("x")).catch(() => undefined))).toBe(
      true,
    );
    expect(isPromise(new Promise(() => {}))).toBe(true);

    const asyncValue = (async () => 1)();
    expect(isPromise(asyncValue)).toBe(true);
    await asyncValue;
  });

  it("detects a then method inherited from a prototype", () => {
    class Thenable {
      then(): void {}
    }
    expect(isPromise(new Thenable())).toBe(true);
  });

  it("does not invoke then, so probing has no side effects", () => {
    const then = jest.fn();
    const thenable = { then };

    expect(isPromise(thenable)).toBe(true);
    expect(then).not.toHaveBeenCalled();
  });

  it("returns false instead of throwing when a hostile then accessor throws", () => {
    const poisoned = {
      get then(): never {
        throw new Error("hostile then accessor");
      },
    };

    expect(() => isPromise(poisoned)).not.toThrow();
    expect(isPromise(poisoned)).toBe(false);
    expect(isPromise(poisoned)).toBe(false);
  });

  it("is deterministic across repeated probes for the same inputs", () => {
    const cases: unknown[] = [
      undefined,
      null,
      0,
      "x",
      {},
      { then: 1 },
      { then: () => {} },
    ];

    for (let attempt = 0; attempt < 3; attempt++) {
      expect(cases.map((value) => isPromise(value as never))).toEqual([
        false,
        false,
        false,
        false,
        false,
        false,
        true,
      ]);
    }
  });
});
