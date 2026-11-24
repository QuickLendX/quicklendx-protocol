"use client";

import React, { useState } from "react";
import { useRouter } from "next/navigation";
import { useWallet } from "@/app/lib/wallet/WalletProvider";
import { createInvoice } from "@/app/lib/api-client";
import { AppError, ErrorCategory } from "@/app/lib/errors";
import {
  createInvoiceBodySchema,
  INVOICE_CATEGORIES,
  LIMITS,
} from "../_lib/schema";
import { toBaseUnits } from "../_lib/math";

type Errors = Record<string, string>;

const inputCls =
  "w-full rounded-md border border-[#E2E8F0] px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-[#2563EB]";

function mapServerDetails(details: unknown): Errors {
  const out: Errors = {};
  if (!Array.isArray(details)) return out;
  for (const d of details) {
    const issue = d as { path?: unknown; message?: unknown };
    if (typeof issue?.message !== "string") continue;
    const segs = Array.isArray(issue.path)
      ? issue.path.map(String)
      : typeof issue.path === "string"
        ? issue.path.split(".")
        : [];
    const key = segs[0] === "metadata" ? segs[1] : segs[0];
    if (key) out[key] ??= issue.message;
  }
  return out;
}

function Field(props: {
  id: string;
  label: string;
  error?: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label htmlFor={props.id} className="block text-sm font-medium text-[#0F172A]">
        {props.label}
      </label>
      <div className="mt-1">{props.children}</div>
      {props.error && (
        <p id={`${props.id}-error`} role="alert" className="mt-1 text-xs text-rose-700">
          {props.error}
        </p>
      )}
    </div>
  );
}

export default function NewInvoiceForm() {
  const wallet = useWallet();
  const router = useRouter();
  const business = wallet.address?.trim() || "";
  const isConnected = wallet.state === "CONNECTED" && Boolean(business);

  const [amount, setAmount] = useState("");
  const [currency, setCurrency] = useState("");
  const [dueDate, setDueDate] = useState("");
  const [category, setCategory] = useState<string>(INVOICE_CATEGORIES[0]);
  const [description, setDescription] = useState("");
  const [customerName, setCustomerName] = useState("");
  const [customerAddress, setCustomerAddress] = useState("");

  const [errors, setErrors] = useState<Errors>({});
  const [formError, setFormError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setFormError(null);

    const fieldErrors: Errors = {};

    const baseAmount = toBaseUnits(amount);
    if (!baseAmount || baseAmount === "0") {
      fieldErrors.amount = "Enter an amount greater than 0 (max 7 decimals).";
    }

    const dueMs = Date.parse(dueDate);
    if (Number.isNaN(dueMs)) {
      fieldErrors.due_date = "Choose a due date.";
    } else if (dueMs <= Date.now()) {
      fieldErrors.due_date = "Due date must be in the future.";
    }

    const candidate = {
      business,
      amount: baseAmount ?? "",
      currency: currency.trim(),
      due_date: Math.floor(dueMs / 1000),
      description: description.trim(),
      category,
      metadata: {
        customer_name: customerName.trim(),
        customer_address: customerAddress.trim(),
      },
    };

    const parsed = createInvoiceBodySchema.safeParse(candidate);
    if (!parsed.success) {
      for (const issue of parsed.error.issues) {
        const key = String(
          issue.path[0] === "metadata" ? issue.path[1] : issue.path[0]
        );
        fieldErrors[key] ??= issue.message;
      }
    }

    if (Object.keys(fieldErrors).length > 0) {
      setErrors(fieldErrors);
      return;
    }
    setErrors({});

    setSubmitting(true);
    try {
      await createInvoice(parsed.data!);
      router.push("/business/invoices");
    } catch (err: unknown) {
      if (err instanceof AppError && err.code === "KYC_NOT_VERIFIED") {
        setFormError(
          "KYC verification is required before you can submit invoices."
        );
      } else if (
        err instanceof AppError &&
        err.category === ErrorCategory.VALIDATION
      ) {
        const serverErrors = mapServerDetails(
          err.context.additionalData?.details
        );
        if (Object.keys(serverErrors).length > 0) setErrors(serverErrors);
        setFormError(err.message);
      } else {
        setFormError(
          err instanceof Error ? err.message : "Failed to create invoice."
        );
      }
    } finally {
      setSubmitting(false);
    }
  }

  if (!isConnected) {
    return (
      <div className="max-w-xl mx-auto p-8 text-center">
        <h1 className="text-xl font-semibold">Connect your wallet</h1>
        <p className="mt-2 text-sm text-[#64748B]">
          Connect your Stellar business wallet to create an invoice.
        </p>
      </div>
    );
  }

  const aria = (id: string) => ({
    "aria-invalid": errors[id] ? true : undefined,
    "aria-describedby": errors[id] ? `${id}-error` : undefined,
  });

  return (
    <div className="min-h-screen bg-[#F8FAFC] py-8 px-4">
      <form
        onSubmit={handleSubmit}
        noValidate
        className="max-w-xl mx-auto space-y-4 bg-white border border-[#E2E8F0] rounded-lg p-6"
      >
        <h1 className="text-2xl font-bold text-[#0F172A]">New Invoice</h1>

        {formError && (
          <div role="alert" className="rounded-md border border-rose-200 bg-rose-50 p-3 text-sm text-rose-800">
            {formError}
          </div>
        )}

        <Field id="amount" label="Amount" error={errors.amount}>
          <input id="amount" inputMode="decimal" className={inputCls}
            value={amount} onChange={(e) => setAmount(e.target.value)} {...aria("amount")} />
        </Field>

        <Field id="currency" label="Currency" error={errors.currency}>
          <input id="currency" className={inputCls}
            value={currency} onChange={(e) => setCurrency(e.target.value)} {...aria("currency")} />
        </Field>

        <Field id="due_date" label="Due date" error={errors.due_date}>
          <input id="due_date" type="date" className={inputCls}
            value={dueDate} onChange={(e) => setDueDate(e.target.value)} {...aria("due_date")} />
        </Field>

        <Field id="category" label="Category" error={errors.category}>
          <select id="category" className={inputCls}
            value={category} onChange={(e) => setCategory(e.target.value)} {...aria("category")}>
            {INVOICE_CATEGORIES.map((c) => (
              <option key={c} value={c}>{c}</option>
            ))}
          </select>
        </Field>

        <Field id="description" label="Description" error={errors.description}>
          <textarea id="description" rows={3} className={inputCls}
            maxLength={LIMITS.description}
            value={description} onChange={(e) => setDescription(e.target.value)} {...aria("description")} />
        </Field>

        <Field id="customer_name" label="Customer name" error={errors.customer_name}>
          <input id="customer_name" className={inputCls}
            maxLength={LIMITS.customerName}
            value={customerName} onChange={(e) => setCustomerName(e.target.value)} {...aria("customer_name")} />
        </Field>

        <Field id="customer_address" label="Customer address" error={errors.customer_address}>
          <input id="customer_address" className={inputCls}
            maxLength={LIMITS.customerAddress}
            value={customerAddress} onChange={(e) => setCustomerAddress(e.target.value)} {...aria("customer_address")} />
        </Field>

        <button
          type="submit"
          disabled={submitting}
          className="w-full rounded-md bg-[#2563EB] px-4 py-2 text-sm font-medium text-white disabled:opacity-40 focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-1"
        >
          {submitting ? "Submitting…" : "Submit invoice"}
        </button>
      </form>
    </div>
  );
}
