import { scValToNative } from "@stellar/stellar-sdk";
import { buildStoreInvoiceArgs } from "../invoice-encoder";
import { InvoiceCategory } from "../../validation";

describe("Encoding unit test: buildStoreInvoiceArgs", () => {
  it("produces 8 ScVals and round-trips correctly", () => {
    const business = "GCXZ7YOZVQ43YJY4PRJBHV7465X6VGD47DQOVNN6NUIVHIHMNBNI4J5L";
    const currency = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM";
    const amount = BigInt("1234567890123456789");
    const dueDate = new Date("2026-06-15T12:00:00Z");
    const description = "Test invoice with special chars: café & tea 🌟";
    const category: InvoiceCategory = "Services";
    const tags = ["software", "development"];
    const nonce = Buffer.alloc(32, 0x42);

    const scVals = buildStoreInvoiceArgs({
      business,
      currency,
      amount,
      dueDate,
      description,
      category,
      tags,
      nonce,
    });

    // Produces 8 ScVals
    expect(scVals).toHaveLength(8);

    // 1. Amount equals exact BigInt
    const decodedAmount = scValToNative(scVals[1]);
    expect(decodedAmount).toBe(amount);

    // 2. Category gives ["Services"] (unit-enum encoding)
    const decodedCategory = scValToNative(scVals[5]);
    expect(decodedCategory).toEqual(["Services"]);

    // 3. The nonce is 32 bytes
    const decodedNonce = scValToNative(scVals[7]);
    expect(
      Buffer.isBuffer(decodedNonce) || decodedNonce instanceof Uint8Array
    ).toBe(true);
    expect(decodedNonce).toHaveLength(32);
    expect(Buffer.from(decodedNonce)).toEqual(nonce);

    // 4. Description bytes equal TextEncoder output
    const decodedDescription = scValToNative(scVals[4]);
    const expectedDescriptionBytes = Buffer.from(
      new TextEncoder().encode(description)
    );
    expect(Buffer.from(decodedDescription)).toEqual(expectedDescriptionBytes);
  });
});
