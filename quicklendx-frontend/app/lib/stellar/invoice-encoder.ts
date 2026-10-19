import { Address, nativeToScVal, xdr } from "@stellar/stellar-sdk";
import { InvoiceCategory } from "../validation";

export interface StoreInvoiceFormValues {
  business: string; // G... public key
  amount: bigint; // exact smallest unit i128
  currency: string; // C... token address
  dueDate: Date | number; // unix timestamp or Date
  description: string;
  category: InvoiceCategory;
  tags: string[];
  nonce: Uint8Array | Buffer; // 32 bytes
}

/**
 * Builds the 8 ScVal arguments required by store_invoice:
 * 1. business (Address)
 * 2. amount (i128)
 * 3. currency (Address)
 * 4. due_date (u64 unix seconds)
 * 5. description (Bytes UTF-8)
 * 6. category (InvoiceCategory enum as Vec([Symbol]))
 * 7. tags (Vec<Bytes>)
 * 8. nonce (BytesN<32>)
 */
export function buildStoreInvoiceArgs(
  form: StoreInvoiceFormValues
): xdr.ScVal[] {
  // 1. business: Address
  const businessScVal = new Address(form.business).toScVal();

  // 2. amount: i128
  const amountScVal = nativeToScVal(form.amount, { type: "i128" });

  // 3. currency: Address
  const currencyScVal = new Address(form.currency).toScVal();

  // 4. due_date: u64 (unix seconds)
  const unixSeconds =
    typeof form.dueDate === "number"
      ? BigInt(Math.floor(form.dueDate))
      : BigInt(Math.floor(form.dueDate.getTime() / 1000));
  const dueDateScVal = nativeToScVal(unixSeconds, { type: "u64" });

  // 5. description: Bytes UTF-8
  const descriptionBytes = Buffer.from(
    new TextEncoder().encode(form.description)
  );
  const descriptionScVal = xdr.ScVal.scvBytes(descriptionBytes);

  // 6. category: unit-enum encoding Vec([Symbol(category)])
  const categoryScVal = xdr.ScVal.scvVec([xdr.ScVal.scvSymbol(form.category)]);

  // 7. tags: Vec<Bytes>
  const tagBytesArray = form.tags.map((tag) =>
    xdr.ScVal.scvBytes(Buffer.from(new TextEncoder().encode(tag)))
  );
  const tagsScVal = xdr.ScVal.scvVec(tagBytesArray);

  // 8. nonce: BytesN<32>
  if (form.nonce.length !== 32) {
    throw new Error(`Nonce must be 32 bytes, got ${form.nonce.length}`);
  }
  const nonceScVal = xdr.ScVal.scvBytes(Buffer.from(form.nonce));

  return [
    businessScVal,
    amountScVal,
    currencyScVal,
    dueDateScVal,
    descriptionScVal,
    categoryScVal,
    tagsScVal,
    nonceScVal,
  ];
}
