/**
 * Tests for the placeBid endpoint helper (#3176).
 *
 * Run with: npx jest app/lib/__tests__/place-bid-endpoint.test.ts
 */

jest.mock("../api-client", () => ({
  apiClient: { post: jest.fn() },
}));

import { apiClient } from "../api-client";
import { placeBid } from "../api/endpoints";
import { AuthenticationError, SystemError } from "../errors";

const post = apiClient.post as jest.Mock;

const payload = {
  invoice_id: "0xabc123",
  bid_amount: "1000",
  expected_return: "1200",
  expiration_timestamp: 1_770_086_400,
};

const serverBid = {
  bid_id: "0xnew",
  invoice_id: "0xabc123",
  investor: "GINVESTOR",
  bid_amount: "1000",
  expected_return: "1200",
  timestamp: 1_770_000_000,
  status: "Placed",
  expiration_timestamp: 1_770_086_400,
};

describe("placeBid", () => {
  beforeEach(() => post.mockReset());

  it("POSTs the payload, untouched, to /bids through the shared client", async () => {
    post.mockResolvedValue({ data: serverBid });
    await placeBid(payload);

    expect(post).toHaveBeenCalledTimes(1);
    expect(post.mock.calls[0][0]).toBe("/bids");
    expect(post.mock.calls[0][1]).toEqual(payload);
    // The same object, not a rebuilt copy: what was validated is what is sent.
    expect(post.mock.calls[0][1]).toBe(payload);
  });

  it("asks the client not to retry: a bid is sent once", async () => {
    post.mockResolvedValue({ data: serverBid });
    await placeBid(payload);
    expect(post.mock.calls[0][2]).toEqual({ retry: false });
  });

  it("returns the created bid from the 201 body", async () => {
    post.mockResolvedValue({ data: serverBid });
    await expect(placeBid(payload)).resolves.toEqual(serverBid);
  });

  it("lets the client's typed errors through unchanged", async () => {
    const error = new AuthenticationError(
      "Authentication required",
      {},
      "UNAUTHORIZED"
    );
    post.mockRejectedValue(error);
    await expect(placeBid(payload)).rejects.toBe(error);
  });

  describe("a success reply that cannot be read", () => {
    it.each([
      ["no data field", {}],
      [
        "a bid with a missing field",
        { data: { ...serverBid, bid_id: undefined } },
      ],
      ["an unknown status", { data: { ...serverBid, status: "Mystery" } }],
      ["null", null],
      ["a string", "created"],
    ])(
      "is reported as BID_RESPONSE_INVALID, not as a plain failure (%s)",
      async (_name, reply) => {
        post.mockResolvedValue(reply);

        const failure = await placeBid(payload).catch((e) => e);

        expect(failure).toBeInstanceOf(SystemError);
        expect(failure.code).toBe("BID_RESPONSE_INVALID");
        // The server answered 2xx, so the message must not suggest it failed.
        expect(failure.message).toMatch(/may have been placed/i);
      }
    );
  });
});
