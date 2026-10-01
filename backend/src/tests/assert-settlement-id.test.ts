import { assertSettlementId } from "../lib/entityId";

const ULID = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const ID = `stl_${ULID}`;
const OVERFLOW_INITIALS = "89ABCDEFGHJKMNPQRSTVWXYZ";

function rejection(
  value: unknown,
): Error & { status: number; statusCode: number; code: string } {
  try {
    assertSettlementId(value);
  } catch (error) {
    return error as Error & {
      status: number;
      statusCode: number;
      code: string;
    };
  }
  throw new Error("Expected settlement ID rejection");
}

describe("assertSettlementId failure boundaries", () => {
  it.each([
    ID,
    `stl_${ULID.toLowerCase()}`,
    `stl_${ULID.slice(0, 10)}${ULID.slice(10).toLowerCase()}`,
    `stl_${"0".repeat(26)}`,
    `stl_7${"Z".repeat(25)}`,
    ` \t${ID}\r\n`,
  ])("accepts valid settlement ID %s without rewriting it", (value) => {
    const original = value;
    expect(assertSettlementId(value)).toBeUndefined();
    expect(value).toBe(original);
    // The assertion signature still narrows unknown for existing TS callers.
    const checked: unknown = value;
    assertSettlementId(checked);
    const narrowed: string = checked;
    expect(narrowed).toBe(value);
  });

  it.each([
    undefined,
    null,
    true,
    false,
    0,
    42,
    NaN,
    Infinity,
    1n,
    Symbol("private-symbol"),
    [],
    [ID],
    {},
    new String(ID),
    "",
    " \n\t",
    "stl_",
    `STL_${ULID}`,
    `inv_${ULID}`,
    `bid_${ULID}`,
    `exp_${ULID}`,
    `0x${ULID}`,
    "00000000-0000-0000-0000-000000000000",
    `stl_${ULID.slice(1)}`,
    `stl_${ULID}0`,
    `stl_${"0".repeat(10000)}`,
    `stl_${ULID.slice(0, 12)} ${ULID.slice(13)}`,
    `stl_${ULID.slice(0, 12)}\n${ULID.slice(13)}`,
    `stl_${ULID}\0`,
    `${ID}%00`,
    `stl_' OR secret='private'`,
    `stl_０${ULID.slice(1)}`,
    `stl_${ULID.slice(0, 25)}💰`,
  ])("rejects malformed input %# with a stable safe error", (value) => {
    const error = rejection(value);
    expect(error).toBeInstanceOf(Error);
    expect(error).toMatchObject({
      name: "BadRequestError",
      status: 400,
      statusCode: 400,
      code: "INVALID_ENTITY_ID",
      message: "Invalid entity ID",
    });
    expect(error.message).not.toContain("private");
  });

  it.each(OVERFLOW_INITIALS.split(""))(
    "rejects 128-bit overflow beginning with %s",
    (first) => {
      expect(rejection(`stl_${first}${"0".repeat(25)}`).code).toBe(
        "INVALID_ENTITY_ID",
      );
      expect(
        rejection(`stl_${first.toLowerCase()}${"z".repeat(25)}`).status,
      ).toBe(400);
    },
  );

  it.each(["I", "L", "O", "U", "i", "l", "o", "u", "-", "_", "/", "\0"])(
    "rejects forbidden character %s at every ULID position",
    (character) => {
      for (let index = 0; index < 26; index += 1) {
        const value = `stl_${ULID.slice(0, index)}${character}${ULID.slice(index + 1)}`;
        expect(rejection(value).code).toBe("INVALID_ENTITY_ID");
      }
    },
  );

  it("does not coerce objects or invoke user-controlled getters", () => {
    const toString = jest.fn(() => {
      throw new Error("private coercion failure");
    });
    const value = Object.freeze({ toString });
    expect(rejection(value).code).toBe("INVALID_ENTITY_ID");
    expect(toString).not.toHaveBeenCalled();
    const getter = jest.fn(() => {
      throw new Error("private getter failure");
    });
    const hostile = Object.defineProperty({}, "length", { get: getter });
    expect(rejection(hostile).code).toBe("INVALID_ENTITY_ID");
    expect(getter).not.toHaveBeenCalled();
  });

  it("remains stateless across repeated valid and rejected attempts", () => {
    const snapshot = Object.freeze({
      id: ID,
      status: "Pending",
      amount: "100",
    });
    for (let attempt = 0; attempt < 100; attempt += 1) {
      expect(() => assertSettlementId(snapshot.id)).not.toThrow();
      expect(rejection("private-invalid-id").code).toBe("INVALID_ENTITY_ID");
    }
    expect(snapshot).toEqual({ id: ID, status: "Pending", amount: "100" });
    expect(rejection("bad")).not.toBe(rejection("bad"));
  });

  it("isolates concurrent successes and rejections without leaking validation state", async () => {
    const outcomes = await Promise.allSettled(
      Array.from({ length: 40 }, (_, index) =>
        Promise.resolve().then(() =>
          assertSettlementId(index % 2 ? "bad" : ID),
        ),
      ),
    );
    expect(
      outcomes.filter((outcome) => outcome.status === "fulfilled"),
    ).toHaveLength(20);
    const rejected = outcomes.filter(
      (outcome): outcome is PromiseRejectedResult =>
        outcome.status === "rejected",
    );
    expect(rejected).toHaveLength(20);
    for (const outcome of rejected)
      expect(outcome.reason.code).toBe("INVALID_ENTITY_ID");
    expect(() => assertSettlementId(ID)).not.toThrow();
  });
});
