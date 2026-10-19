"use client";

import React, { useCallback, useEffect, useRef, useState } from "react";
import {
  trackWalletConnectAttempt,
  trackWalletConnected,
  trackWalletConnectFailed,
  trackWalletDisconnected,
} from "../lib/wallet-telemetry";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type WalletConnectionState =
  | "NOT_CONNECTED"
  | "CONNECTING"
  | "CONNECTED"
  | "WRONG_NETWORK"
  | "ERROR"
  | "DISCONNECTING";

export interface WalletConnectProps {
  /** Expected network: "public" (Mainnet) or "testnet". */
  expectedNetwork?: "public" | "testnet";
  /** Called when the user successfully connects. Receives the public key. */
  onConnect?: (publicKey: string) => void;
  /** Called when the user disconnects. */
  onDisconnect?: () => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Truncates a Stellar address to "XXXXXX…XXXX" format.
 * Spec: docs/ux/wallet-connect.md §4.1
 */
export function truncateAddress(address: string): string {
  if (address.length <= 13) return address;
  return `${address.slice(0, 6)}\u2026${address.slice(-4)}`;
}

function explorerUrl(address: string, network: "public" | "testnet"): string {
  return `https://stellar.expert/explorer/${network}/account/${address}`;
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

/** Copy-to-clipboard button with feedback and error hardening. */
export const CopyButton: React.FC<{ value: string }> = ({ value }) => {
  const [copied, setCopied] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const liveRef = useRef<HTMLSpanElement>(null);
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);

  useEffect(() => {
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, []);

  const handleCopy = useCallback(
    async (e: React.MouseEvent | React.KeyboardEvent) => {
      e.stopPropagation();
      setErrorMessage(null);
      try {
        if (!navigator?.clipboard?.writeText) {
          throw new Error("Clipboard API not available");
        }
        await navigator.clipboard.writeText(value);
        setCopied(true);
        if (liveRef.current) liveRef.current.textContent = "Address copied.";
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => {
          setCopied(false);
          if (liveRef.current) liveRef.current.textContent = "";
        }, 2000);
      } catch {
        const errText = "Couldn't copy, select the address manually";
        setErrorMessage(errText);
        if (liveRef.current) liveRef.current.textContent = errText;
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => {
          setErrorMessage(null);
          if (liveRef.current) liveRef.current.textContent = "";
        }, 3000);
      }
    },
    [value]
  );

  return (
    <div className="relative inline-flex items-center">
      <button
        type="button"
        onClick={handleCopy}
        aria-label="Copy full address to clipboard"
        className="ml-1 text-neutral-500 hover:text-neutral-900 focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
      >
        {copied ? (
          <svg
            width="14"
            height="14"
            viewBox="0 0 16 16"
            fill="none"
            aria-hidden="true"
          >
            <path
              d="M3 8l4 4 6-6"
              stroke="#10B981"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        ) : (
          <svg
            width="14"
            height="14"
            viewBox="0 0 16 16"
            fill="none"
            aria-hidden="true"
          >
            <rect
              x="5"
              y="5"
              width="8"
              height="8"
              rx="1"
              stroke="currentColor"
              strokeWidth="1.5"
            />
            <path
              d="M3 11V3h8"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
            />
          </svg>
        )}
      </button>
      {/* aria-live region for screen readers */}
      <span ref={liveRef} aria-live="polite" className="sr-only" />
      {errorMessage && (
        <div
          role="alert"
          className="absolute bottom-full mb-2 left-1/2 -translate-x-1/2 whitespace-nowrap bg-red-900 text-white text-xs px-2 py-1 rounded shadow-lg z-50"
        >
          {errorMessage}
        </div>
      )}
    </div>
  );
};

export interface ConnectedAccountProps {
  address: string;
  network: "public" | "testnet";
  onDisconnect?: () => void;
  disconnecting?: boolean;
}

/** Connected account pill restructured as a role="group" with sibling controls. */
export const ConnectedAccount: React.FC<ConnectedAccountProps> = ({
  address,
  network,
  onDisconnect,
  disconnecting = false,
}) => {
  const [accountMenuOpen, setAccountMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const disconnectRef = useRef<HTMLButtonElement>(null);

  // Handle outside pointerdown & Escape key
  useEffect(() => {
    const handleOutsidePointerDown = (event: PointerEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setAccountMenuOpen(false);
      }
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && accountMenuOpen) {
        setAccountMenuOpen(false);
        triggerRef.current?.focus();
      }
    };

    if (accountMenuOpen) {
      document.addEventListener("pointerdown", handleOutsidePointerDown);
      document.addEventListener("keydown", handleKeyDown);
    }
    return () => {
      document.removeEventListener("pointerdown", handleOutsidePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [accountMenuOpen]);

  // Move focus to first menu item when opened
  useEffect(() => {
    if (accountMenuOpen) {
      disconnectRef.current?.focus();
    }
  }, [accountMenuOpen]);

  const handleTriggerKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
      e.preventDefault();
      setAccountMenuOpen(true);
    }
  };

  return (
    <div
      ref={menuRef}
      role="group"
      aria-label="Connected wallet"
      className="relative inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-[#E2E8F0] bg-white text-sm font-mono text-[#0F172A]"
    >
      {/* Live indicator */}
      <span className="h-2 w-2 rounded-full bg-[#10B981]" aria-hidden="true" />

      {/* Account menu trigger button */}
      <button
        ref={triggerRef}
        type="button"
        aria-haspopup="menu"
        aria-expanded={accountMenuOpen}
        onClick={() => setAccountMenuOpen((v) => !v)}
        onKeyDown={handleTriggerKeyDown}
        className="flex items-center gap-1 hover:text-neutral-900 focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
      >
        <span>{truncateAddress(address)}</span>
        <svg
          width="12"
          height="12"
          viewBox="0 0 16 16"
          fill="none"
          aria-hidden="true"
          className={`transition-
