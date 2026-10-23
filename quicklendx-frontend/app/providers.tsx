"use client";

import React from "react";
import { Toaster } from "react-hot-toast";

/**
 * Client-side providers mounted once by the root layout.
 *
 * `react-hot-toast` only renders toasts while a `<Toaster />` is mounted, so
 * every `toast.*` call in the app — ConfirmAction's "Bid accepted. Funds are
 * in escrow." toast and every `ErrorToastManager.showError/showSuccess/
 * showWarning/showInfo` call — was dropped without a trace before this
 * component existed.
 *
 * Exactly one `<Toaster />` is mounted for the whole tree; mounting a second
 * one would duplicate every toast.
 */
export const Providers: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <>
    {children}
    <Toaster position="top-right" />
  </>
);

export default Providers;
