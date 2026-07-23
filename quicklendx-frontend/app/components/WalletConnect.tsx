"use client";

import React, { useCallback, useEffect, useRef, useState } from "react";

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

/** Copy-to-clipboard button with 2-second "Copied!" feedback. */
const CopyButton: React.FC<{ value: string }> = ({ value }) => {
  const [copied, setCopied] = useState(false);
  const liveRef = useRef<HTMLSpanElement>(null);

  const handleCopy = useCallback(async () => {
    await navigator.clipboard.writeText(value);
    setCopied(true);
    if (liveRef.current) liveRef.current.textContent = "Address copied.";
    setTimeout(() => {
      setCopied(false);
      if (liveRef.current) liveRef.current.textContent = "";
    }, 2000);
  }, [value]);

  return (
    <>
      <button
        type="button"
        onClick={handleCopy}
        aria-label="Copy full address to clipboard"
        className="ml-1 text-neutral-500 hover:text-neutral-900 focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
      >
        {copied ? (
          // Checkmark
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
          // Copy icon
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
    </>
  );
};

/** Connected address pill shown in the nav / trigger area. */
const AddressPill: React.FC<{
  address: string;
  network: "public" | "testnet";
  onClick: () => void;
}> = ({ address, network, onClick }) => (
  <button
    type="button"
    onClick={onClick}
    className="flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-[#E2E8F0] bg-white hover:bg-neutral-50 focus:outline-none focus:ring-2 focus:ring-[#2563EB] text-sm font-mono text-[#0F172A]"
    aria-label="Wallet connected — click to open account menu"
  >
    {/* Live indicator */}
    <span className="h-2 w-2 rounded-full bg-[#10B981]" aria-hidden="true" />
    <span>{truncateAddress(address)}</span>
    <CopyButton value={address} />
    <a
      href={explorerUrl(address, network)}
      target="_blank"
      rel="noopener noreferrer"
      aria-label={`View account ${address} on Stellar Expert (opens in new tab)`}
      onClick={(e) => e.stopPropagation()}
      className="text-neutral-500 hover:text-[#2563EB] focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
    >
      {/* External link icon */}
      <svg
        width="14"
        height="14"
        viewBox="0 0 16 16"
        fill="none"
        aria-hidden="true"
      >
        <path
          d="M7 3H3a1 1 0 00-1 1v9a1 1 0 001 1h9a1 1 0 001-1V9"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
        />
        <path
          d="M10 2h4v4M14 2l-6 6"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </a>
  </button>
);

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

/**
 * WalletConnect — placeholder component for the wallet connection modal and
 * connection-state UX.
 *
 * Spec: docs/ux/wallet-connect.md
 *
 * NOTE: Full implementation requires @stellar/freighter-api (out of scope for
 * this spec). The state machine and UI are complete; wire up the SDK calls
 * where the TODO comments appear.
 */
export const WalletConnect: React.FC<WalletConnectProps> = ({
  expectedNetwork = "testnet",
  onConnect,
  onDisconnect,
}) => {
  const [state, setState] = useState<WalletConnectionState>("NOT_CONNECTED");
  const [address, setAddress] = useState<string>("");
  const [errorMessage, setErrorMessage] = useState<string>("");
  const [modalOpen, setModalOpen] = useState(false);
  const [accountMenuOpen, setAccountMenuOpen] = useState(false);
  const modalRef = useRef<HTMLDivElement>(null);
  const firstFocusRef = useRef<HTMLButtonElement>(null);

  // Focus trap & Escape key
  useEffect(() => {
    if (!modalOpen) return;
    firstFocusRef.current?.focus();

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (state === "CONNECTING") {
          // Confirm abort — for now just close; a confirmation dialog can be
          // added in the full implementation.
        }
        setModalOpen(false);
      }
      if (e.key === "Tab" && modalRef.current) {
        const focusable = modalRef.current.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
        );
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (
          e.shiftKey
            ? document.activeElement === first
            : document.activeElement === last
        ) {
          e.preventDefault();
          (e.shiftKey ? last : first).focus();
        }
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [modalOpen, state]);

  const handleConnect = useCallback(async () => {
    setState("CONNECTING");
    setErrorMessage("");
    try {
      // TODO: replace with Freighter SDK call
      // const { address } = await getAddress();
      // const { network } = await getNetwork();
      // if (network !== expectedNetwork) { setState("WRONG_NETWORK"); return; }
      // setAddress(address);
      // setState("CONNECTED");
      // setModalOpen(false);
      // onConnect?.(address);

      // Placeholder: simulate async connection
      await new Promise((r) => setTimeout(r, 0));
      throw new Error("SDK_NOT_INTEGRATED");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Unknown error";
      const userMessage =
        msg === "SDK_NOT_INTEGRATED"
          ? "Wallet SDK not yet integrated. See docs/ux/wallet-connect.md."
          : msg.includes("not installed")
            ? "Freighter is not installed."
            : msg.includes("rejected")
              ? "Connection was declined. You can try again at any time."
              : msg.includes("timeout")
                ? "The request timed out. Please try again."
                : "Something went wrong. Please try again.";
      setErrorMessage(userMessage);
      setState("ERROR");
    }
  }, []);

  const handleDisconnect = useCallback(async () => {
    setState("DISCONNECTING");
    setAccountMenuOpen(false);
    // TODO: clear Freighter session if SDK provides a method
    await new Promise((r) => setTimeout(r, 300));
    setAddress("");
    setState("NOT_CONNECTED");
    onDisconnect?.();
  }, [onDisconnect]);

  // -------------------------------------------------------------------------
  // Render helpers
  // -------------------------------------------------------------------------

  const renderModalBody = () => {
    if (state === "ERROR") {
      return (
        <div className="flex flex-col gap-4">
          <p className="text-[#EF4444] flex items-start gap-2">
            <span aria-hidden="true">✕</span>
            {errorMessage}
          </p>
          <button
            type="button"
            onClick={handleConnect}
            className="self-start px-4 py-2 bg-[#2563EB] text-white rounded-md text-sm font-medium hover:brightness-110 focus:outline-none focus:ring-2 focus:ring-[#2563EB]"
          >
            Try Again
          </button>
        </div>
      );
    }

    return (
      <>
        <p className="text-[#64748B] text-base leading-relaxed">
          Connect your Stellar wallet to bid, fund, and settle invoices.
        </p>
        {/* Trust message — always visible */}
        <div className="mt-4 p-3 rounded-md bg-[#F8FAFC] border border-[#E2E8F0] text-sm text-[#64748B] flex items-start gap-2">
          <span aria-hidden="true">🔒</span>
          <span>
            Your keys never leave your device.{" "}
            <strong className="font-medium text-[#0F172A]">
              QuickLendX cannot access your funds.
            </strong>
          </span>
        </div>
      </>
    );
  };

  const isConnecting = state === "CONNECTING";
  const isDisconnecting = state === "DISCONNECTING";

  // -------------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------------

  return (
    <>
      {/* ── Trigger / nav element ── */}
      {state === "CONNECTED" ? (
        <div className="relative">
          <AddressPill
            address={address}
            network={expectedNetwork}
            onClick={() => setAccountMenuOpen((v) => !v)}
          />
          {accountMenuOpen && (
            <div className="absolute right-0 mt-1 w-40 bg-white border border-[#E2E8F0] rounded-lg shadow-[0_10px_15px_-3px_rgba(0,0,0,0.1)] z-50">
              <button
                type="button"
                onClick={handleDisconnect}
                disabled={isDisconnecting}
                className="w-full text-left px-4 py-2.5 text-sm text-[#0F172A] hover:bg-neutral-50 rounded-lg disabled:opacity-40 disabled:cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-inset focus:ring-[#2563EB]"
              >
                {isDisconnecting ? "Disconnecting…" : "Disconnect"}
              </button>
            </div>
          )}
        </div>
      ) : state === "WRONG_NETWORK" ? (
        <div className="flex items-center gap-2 px-3 py-1.5 rounded-full bg-amber-50 border border-amber-300 text-sm text-amber-800">
          <span aria-hidden="true">⚠️</span>
          Wrong network —{" "}
          <button
            type="button"
            onClick={() => setModalOpen(true)}
            className="underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
          >
            fix
          </button>
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setModalOpen(true)}
          className="px-4 py-2 bg-[#2563EB] text-white rounded-md text-sm font-medium hover:brightness-110 active:scale-[0.98] focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-2"
        >
          Connect Wallet
        </button>
      )}

      {/* ── Modal ── */}
      {modalOpen && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center sm:items-center"
          aria-modal="true"
          role="dialog"
          aria-labelledby="wc-modal-title"
        >
          {/* Backdrop */}
          <div
            className="absolute inset-0 bg-black/50"
            onClick={() => !isConnecting && setModalOpen(false)}
            aria-hidden="true"
          />

          {/* Container */}
          <div
            ref={modalRef}
            className="relative w-full max-w-[480px] mx-4 bg-white rounded-lg shadow-[0_10px_15px_-3px_rgba(0,0,0,0.1)] sm:mx-auto"
          >
            {/* Header */}
            <div className="flex items-center justify-between px-6 py-4 border-b border-[#E2E8F0]">
              <h2
                id="wc-modal-title"
                className="text-2xl font-semibold text-[#0F172A]"
              >
                Connect Wallet
              </h2>
              <button
                type="button"
                ref={firstFocusRef}
                onClick={() => setModalOpen(false)}
                aria-label="Close wallet connect modal"
                className="text-[#64748B] hover:text-[#0F172A] focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
              >
                <svg
                  width="20"
                  height="20"
                  viewBox="0 0 20 20"
                  fill="none"
                  aria-hidden="true"
                >
                  <path
                    d="M5 5l10 10M15 5L5 15"
                    stroke="currentColor"
                    strokeWidth="1.75"
                    strokeLinecap="round"
                  />
                </svg>
              </button>
            </div>

            {/* Body */}
            <div className="px-6 py-5">{renderModalBody()}</div>

            {/* Footer */}
            <div className="flex justify-end gap-2 px-6 py-4 border-t border-[#E2E8F0]">
              <button
                type="button"
                onClick={() => setModalOpen(false)}
                className="px-4 py-2 text-sm font-medium text-[#64748B] hover:text-[#0F172A] focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded-md"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleConnect}
                disabled={isConnecting}
                className="flex items-center gap-2 px-4 py-2 bg-[#2563EB] text-white rounded-md text-sm font-medium hover:brightness-110 disabled:opacity-40 disabled:cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-[#2563EB]"
              >
                {isConnecting && (
                  <svg
                    className="animate-spin h-4 w-4"
                    viewBox="0 0 24 24"
                    fill="none"
                    role="status"
                    aria-label="Connecting…"
                  >
                    <circle
                      className="opacity-25"
                      cx="12"
                      cy="12"
                      r="10"
                      stroke="currentColor"
                      strokeWidth="4"
                    />
                    <path
                      className="opacity-75"
                      fill="currentColor"
                      d="M4 12a8 8 0 018-8v4a4 4 0 00-4 4H4z"
                    />
                  </svg>
                )}
                {isConnecting ? "Connecting…" : "Connect"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Wrong-network inline banner (shown when modal is closed) */}
      {state === "WRONG_NETWORK" && !modalOpen && (
        <div
          role="alert"
          className="fixed top-4 left-1/2 -translate-x-1/2 z-40 flex items-center gap-3 px-4 py-3 rounded-lg bg-amber-50 border border-amber-300 shadow-[0_10px_15px_-3px_rgba(0,0,0,0.1)] text-sm text-amber-800"
        >
          <span aria-hidden="true">⚠️</span>
          <span>
            Wrong network detected. Switch to{" "}
            <strong>
              {expectedNetwork === "public" ? "Mainnet" : "Testnet"}
            </strong>{" "}
            in Freighter to continue.
          </span>
          <button
            type="button"
            onClick={handleDisconnect}
            className="ml-2 underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
          >
            Disconnect
          </button>
        </div>
      )}
    </>
  );
};
