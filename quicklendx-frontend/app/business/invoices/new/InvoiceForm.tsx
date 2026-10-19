"use client";

import React, { useState, useId, useRef, useCallback, useEffect } from "react";
import { useRouter } from "next/navigation";
import toast from "react-hot-toast";
import { useWallet } from "../../../lib/wallet/WalletProvider";
import {
  createInvoiceSchema,
  INVOICE_CATEGORIES,
  InvoiceCategory,
  parseTokenAmountToUnits,
} from "../../../lib/validation";
import { getConfiguredTokens } from "../../../lib/stellar/tokens";
import { buildStoreInvoiceArgs } from "../../../lib/stellar/invoice-encoder";
import { invokeContract } from "../../../lib/stellar/contract-client";
import { ContractError } from "../../../lib/stellar/contract-errors";

function generateNonce(): Uint8Array {
  const nonce = new Uint8Array(32);
  if (typeof crypto !== "undefined" && crypto.getRandomValues) {
    crypto.getRandomValues(nonce);
  } else {
    for (let i = 0; i < 32; i++) {
      nonce[i] = Math.floor(Math.random() * 256);
    }
  }
  return nonce;
}

export function InvoiceForm() {
  const router = useRouter();
  const {
    status: walletStatus,
    address,
    expectedNetwork,
    signTransaction,
  } = useWallet();

  const tokens = getConfiguredTokens();

  // Form states
  const [tokenAddress, setTokenAddress] = useState<string>(
    tokens[0]?.address || ""
  );
  const [amount, setAmount] = useState<string>("");
  const [dueDate, setDueDate] = useState<string>("");
  const [description, setDescription] = useState<string>("");
  const [category, setCategory] = useState<InvoiceCategory>("Services");
  const [tags, setTags] = useState<string[]>([]);
  const [tagInput, setTagInput] = useState<string>("");

  // Status & submission states
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
  const [submitPhase, setSubmitPhase] = useState<
    "idle" | "signing" | "submitting"
  >("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<Record<string, string>>({});

  // Idempotency: nonce stored across retries, regenerated on field edit or success
  const nonceRef = useRef<Uint8Array | null>(null);
  const isExecutingRef = useRef<boolean>(false);

  const resetNonce = useCallback(() => {
    nonceRef.current = null;
  }, []);

  // Description byte count
  const descriptionBytes = new TextEncoder().encode(description).length;

  // Add tag handler (case-insensitively deduplicated, max 10 tags)
  const handleAddTag = useCallback(() => {
    const trimmed = tagInput.trim();
    if (!trimmed) return;

    const lowerTrimmed = trimmed.toLowerCase();
    if (tags.some((t) => t.toLowerCase() === lowerTrimmed)) {
      setTagInput("");
      return;
    }

    if (tags.length >= 10) {
      setFieldErrors((prev) => ({
        ...prev,
        tags: "Cannot specify more than 10 tags",
      }));
      return;
    }

    const tagBytes = new TextEncoder().encode(trimmed).length;
    if (tagBytes > 50) {
      setFieldErrors((prev) => ({
        ...prev,
        tags: "Each tag must be at most 50 bytes",
      }));
      return;
    }

    setTags((prev) => [...prev, trimmed]);
    setTagInput("");
    resetNonce();
  }, [tagInput, tags, resetNonce]);

  const handleRemoveTag = useCallback(
    (indexToRemove: number) => {
      setTags((prev) => prev.filter((_, idx) => idx !== indexToRemove));
      resetNonce();
    },
    [resetNonce]
  );

  const isWalletConnected = walletStatus === "CONNECTED" && Boolean(address);
  const isWrongNetwork = walletStatus === "WRONG_NETWORK";

  const tokenSelectId = useId();
  const amountInputId = useId();
  const dueDateInputId = useId();
  const descriptionInputId = useId();
  const categorySelectId = useId();
  const tagInputId = useId();

  // Validate form synchronously
  const validateForm = () => {
    const selectedDate = dueDate ? new Date(`${dueDate}T00:00:00Z`) : undefined;
    const schema = createInvoiceSchema(new Date(), tokens);

    const result = schema.safeParse({
      currency: tokenAddress,
      amount,
      dueDate: selectedDate,
      description,
      category,
      tags,
    });

    if (!result.success) {
      const errors: Record<string, string> = {};
      for (const issue of result.error.issues) {
        const path = issue.path[0]?.toString() || "form";
        if (!errors[path]) {
          errors[path] = issue.message;
        }
      }
      setFieldErrors(errors);
      return false;
    }

    setFieldErrors({});
    return true;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (isExecutingRef.current || isSubmitting) {
      return;
    }

    setErrorMessage(null);

    if (!isWalletConnected) {
      setErrorMessage("Please connect your wallet first.");
      return;
    }

    if (!validateForm()) {
      return;
    }

    isExecutingRef.current = true;
    setIsSubmitting(true);
    setSubmitPhase("signing");

    try {
      // Create nonce if not already set (retained across retries)
      if (!nonceRef.current) {
        nonceRef.current = generateNonce();
      }

      const selectedToken =
        tokens.find((t) => t.address === tokenAddress) || tokens[0];
      const parsedAmountUnits = parseTokenAmountToUnits(
        amount,
        selectedToken.decimals
      );
      const parsedDueDate = new Date(`${dueDate}T00:00:00Z`);

      const args = buildStoreInvoiceArgs({
        business: address!,
        amount: parsedAmountUnits,
        currency: tokenAddress,
        dueDate: parsedDueDate,
        description,
        category,
        tags,
        nonce: nonceRef.current,
      });

      const customSigner = async (xdrString: string) => {
        setSubmitPhase("signing");
        try {
          const signed = await signTransaction(xdrString);
          setSubmitPhase("submitting");
          return signed;
        } catch (signErr) {
          throw signErr;
        }
      };

      const result = await invokeContract<string>({
        method: "store_invoice",
        args,
        source: address!,
        signTransaction: customSigner,
      });

      toast.success("Invoice created successfully!");
      resetNonce();

      const invoiceHex = result.result;
      router.push(`/business/invoices/${invoiceHex}`);
    } catch (err: any) {
      if (err instanceof ContractError) {
        setErrorMessage(err.message);
      } else if (err?.message === "Signature request declined.") {
        setErrorMessage("Signature request declined.");
      } else {
        setErrorMessage(err.message || "Failed to create invoice.");
      }
    } finally {
      setIsSubmitting(false);
      setSubmitPhase("idle");
      isExecutingRef.current = false;
    }
  };

  const getButtonText = () => {
    if (submitPhase === "signing") return "Waiting for signature…";
    if (submitPhase === "submitting") return "Submitting…";
    if (errorMessage) return "Retry";
    return "Create Invoice";
  };

  return (
    <form
      onSubmit={handleSubmit}
      className="space-y-6 max-w-2xl mx-auto p-6 bg-white rounded-xl shadow-md border border-neutral-100"
    >
      <h2 className="text-2xl font-bold text-neutral-900 mb-6">
        Create New Invoice
      </h2>

      {errorMessage && (
        <div
          role="alert"
          className="p-4 rounded-lg bg-red-50 border border-red-200 text-red-700 text-sm font-medium"
        >
          {errorMessage}
        </div>
      )}

      {isWrongNetwork && (
        <div
          role="alert"
          className="p-3 rounded-lg bg-amber-50 border border-amber-200 text-amber-800 text-sm"
        >
          Connected to the wrong network. Please switch to {expectedNetwork}.
        </div>
      )}

      {/* Currency / Token selection */}
      <div>
        <label
          htmlFor={tokenSelectId}
          className="block text-sm font-medium text-neutral-700 mb-1"
        >
          Currency
        </label>
        <select
          id={tokenSelectId}
          value={tokenAddress}
          onChange={(e) => {
            setTokenAddress(e.target.value);
            resetNonce();
          }}
          className="w-full px-4 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500 border-neutral-300"
        >
          {tokens.map((t) => (
            <option key={t.address} value={t.address}>
              {t.code} ({t.address.slice(0, 6)}…{t.address.slice(-4)})
            </option>
          ))}
        </select>
        {fieldErrors.currency && (
          <p className="mt-1 text-sm text-red-600">{fieldErrors.currency}</p>
        )}
      </div>

      {/* Amount input */}
      <div>
        <label
          htmlFor={amountInputId}
          className="block text-sm font-medium text-neutral-700 mb-1"
        >
          Amount
        </label>
        <input
          id={amountInputId}
          type="text"
          placeholder="0.00"
          value={amount}
          onChange={(e) => {
            setAmount(e.target.value);
            resetNonce();
          }}
          className="w-full px-4 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500 border-neutral-300"
        />
        {fieldErrors.amount && (
          <p className="mt-1 text-sm text-red-600">{fieldErrors.amount}</p>
        )}
      </div>

      {/* Due Date */}
      <div>
        <label
          htmlFor={dueDateInputId}
          className="block text-sm font-medium text-neutral-700 mb-1"
        >
          Due Date
        </label>
        <input
          id={dueDateInputId}
          type="date"
          value={dueDate}
          onChange={(e) => {
            setDueDate(e.target.value);
            resetNonce();
          }}
          className="w-full px-4 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500 border-neutral-300"
        />
        {fieldErrors.dueDate && (
          <p className="mt-1 text-sm text-red-600">{fieldErrors.dueDate}</p>
        )}
      </div>

      {/* Description with byte counter */}
      <div>
        <div className="flex justify-between items-center mb-1">
          <label
            htmlFor={descriptionInputId}
            className="block text-sm font-medium text-neutral-700"
          >
            Description
          </label>
          <span
            aria-label="Description byte count"
            className={`text-xs ${
              descriptionBytes > 1024
                ? "text-red-600 font-semibold"
                : "text-neutral-500"
            }`}
          >
            {descriptionBytes} / 1024 bytes
          </span>
        </div>
        <textarea
          id={descriptionInputId}
          rows={3}
          value={description}
          onChange={(e) => {
            setDescription(e.target.value);
            resetNonce();
          }}
          className="w-full px-4 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500 border-neutral-300"
        />
        {fieldErrors.description && (
          <p className="mt-1 text-sm text-red-600">{fieldErrors.description}</p>
        )}
      </div>

      {/* Category selection */}
      <div>
        <label
          htmlFor={categorySelectId}
          className="block text-sm font-medium text-neutral-700 mb-1"
        >
          Category
        </label>
        <select
          id={categorySelectId}
          value={category}
          onChange={(e) => {
            setCategory(e.target.value as InvoiceCategory);
            resetNonce();
          }}
          className="w-full px-4 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500 border-neutral-300"
        >
          {INVOICE_CATEGORIES.map((cat) => (
            <option key={cat} value={cat}>
              {cat}
            </option>
          ))}
        </select>
        {fieldErrors.category && (
          <p className="mt-1 text-sm text-red-600">{fieldErrors.category}</p>
        )}
      </div>

      {/* Tags chips */}
      <div>
        <label
          htmlFor={tagInputId}
          className="block text-sm font-medium text-neutral-700 mb-1"
        >
          Tags (max 10)
        </label>
        <div className="flex gap-2 mb-2">
          <input
            id={tagInputId}
            type="text"
            placeholder="Add tag"
            value={tagInput}
            onChange={(e) => setTagInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                handleAddTag();
              }
            }}
            className="flex-1 px-4 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500 border-neutral-300"
          />
          <button
            type="button"
            onClick={handleAddTag}
            className="px-4 py-2 bg-neutral-100 hover:bg-neutral-200 text-neutral-800 rounded-lg text-sm font-medium transition"
          >
            Add
          </button>
        </div>

        {tags.length > 0 && (
          <div className="flex flex-wrap gap-2 mt-2">
            {tags.map((tag, idx) => (
              <span
                key={idx}
                className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium bg-blue-50 text-blue-700 border border-blue-200"
              >
                {tag}
                <button
                  type="button"
                  onClick={() => handleRemoveTag(idx)}
                  className="text-blue-500 hover:text-blue-800 focus:outline-none"
                  aria-label={`Remove tag ${tag}`}
                >
                  &times;
                </button>
              </span>
            ))}
          </div>
        )}
        {fieldErrors.tags && (
          <p className="mt-1 text-sm text-red-600">{fieldErrors.tags}</p>
        )}
      </div>

      {/* Submit Button */}
      <div className="pt-4">
        <button
          type="submit"
          disabled={!isWalletConnected || isWrongNetwork || isSubmitting}
          className="w-full py-3 px-6 bg-blue-600 hover:bg-blue-700 disabled:bg-neutral-300 disabled:cursor-not-allowed text-white font-medium rounded-lg transition duration-150 ease-in-out shadow-sm"
        >
          {getButtonText()}
        </button>
        {isWrongNetwork && (
          <p className="text-xs text-center text-amber-700 mt-2">
            Wallet is connected to the wrong network. Switch to{" "}
            {expectedNetwork} to submit.
          </p>
        )}
        {!isWalletConnected && !isWrongNetwork && (
          <p className="text-xs text-center text-neutral-500 mt-2">
            Connect wallet to create invoices.
          </p>
        )}
      </div>
    </form>
  );
}
