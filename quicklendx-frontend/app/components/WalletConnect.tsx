“use client”;

/**
 * WalletConnect — view component over useWallet().
 *
 * This component no longer owns wallet state. It reads from the context
 * provided by WalletProvider and delegates all actions to it.
 *
 * UI, copy, and telemetry calls are unchanged from the original design.
 * Spec: docs/ux/wallet-connect.md
 */

import React, { useCallback, useState } from "react";
import { useWallet } from "../lib/wallet/WalletProvider";

// —————————————————————————
// Types
// —————————————————————————

export interface WalletConnectProps {
  /** Called when the user successfully connects. Receives the public key. */
  onConnect?: (publicKey: string) => void;
  /** Called when the user disconnects. */
  onDisconnect?: () => void;
}

// —————————————————————————
// Helpers
// —————————————————————————

/**

* Truncates a Stellar address to “XXXXXX…XXXX” format.
* Spec: docs/ux/wallet-connect.md §4.1
    */
    export function truncateAddress(address: string): string {
    if (address.length <= 13) return address;
    return ${address.slice(0, 6)}\u2026${address.slice(-4)};
    }

function explorerUrl(address: string, network: “public” | “testnet”): string {
return https://stellar.expert/explorer/${network}/account/${address};
}

function explorerUrl(
  address: string,
  network: "public" | "testnet"
): string {
  return `https://stellar.expert/explorer/${network}/account/${address}`;
}
};
}, []);

const handleCopy = useCallback(
async (e: React.MouseEvent) => {
e.stopPropagation();
setErrorMessage(null);

const CopyButton: React.FC<{ value: string }> = ({ value }) => {
  const [copied, setCopied] = useState(false);
  const liveRef = React.useRef<HTMLSpanElement>(null);

  const handleCopy = useCallback(async () => {
    await navigator.clipboard.writeText(value);
    setCopied(true);
    if (liveRef.current) {
      liveRef.current.textContent = "Address copied.";
    }
    if (timeoutRef.current !== null) {
      clearTimeout(timeoutRef.current);
    }
    timeoutRef.current = setTimeout(() => {
      setCopied(false);
      if (liveRef.current) {
        liveRef.current.textContent = "";
      }
      timeoutRef.current = null;
    }, 2000);
  } catch {
    const message = "Couldn't copy, select the address manually";
    setErrorMessage(message);
    if (liveRef.current) {
      liveRef.current.textContent = message;
    }
    if (timeoutRef.current !== null) {
      clearTimeout(timeoutRef.current);
    }
    timeoutRef.current = setTimeout(() => {
      setErrorMessage(null);
      if (liveRef.current) {
        liveRef.current.textContent = "";
      }
      timeoutRef.current = null;
    }, 3000);
  }
},
[value]

  return (
    <>
      <button
        type="button"
        onClick={handleCopy}
        aria-label="Copy full address to clipboard"
        className="ml-1 text-neutral-500 hover:text-neutral-900 focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
      >
        {copied ? (
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path d="M3 8l4 4 6-6" stroke="#10B981" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        ) : (
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <rect x="5" y="5" width="8" height="8" rx="1" stroke="currentColor" strokeWidth="1.5" />
            <path d="M3 11V3h8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </svg>
        )}
      </button>
      <span ref={liveRef} aria-live="polite" className="sr-only" />
    </>
  );
};

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
      <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
        <path d="M7 3H3a1 1 0 00-1 1v9a1 1 0 001 1h9a1 1 0 001-1V9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        <path d="M10 2h4v4M14 2l-6 6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </a>
  </button>
);

return (
{copied ? (
) : (
)}
  <span
    ref={liveRef}
    aria-live="polite"
    aria-atomic="true"
    className="sr-only"
  />
  {errorMessage && (
    <div
      role="alert"
      className="absolute bottom-full left-1/2 z-50 mb-2 -translate-x-1/2 whitespace-nowrap rounded bg-red-900 px-2 py-1 text-xs text-white shadow-lg"
    >
      {errorMessage}
    </div>
  )}
</div>

);
};

// —————————————————————————
// Connected account
// —————————————————————————

export interface ConnectedAccountProps {
address: string;
network: “public” | “testnet”;
onDisconnect?: () => void;
disconnecting?: boolean;
}

export const WalletConnect: React.FC<WalletConnectProps> = ({
  onConnect,
  onDisconnect,
}) => {
  const {
    state,
    address,
    error,
    expectedNetwork,
    connect,
    disconnect,
  } = useWallet();

  const [modalOpen, setModalOpen] = useState(false);
  const [accountMenuOpen, setAccountMenuOpen] = useState(false);
  const modalRef = React.useRef<HTMLDivElement>(null);
  const firstFocusRef = React.useRef<HTMLButtonElement>(null);

  // Focus trap & Escape key for the connect modal
  React.useEffect(() => {
    if (!modalOpen) return;
    firstFocusRef.current?.focus();

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (state !== "CONNECTING") setModalOpen(false);
      }
      if (e.key === "Tab" && modalRef.current) {
        const focusable = modalRef.current.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
        );
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (e.shiftKey ? document.activeElement === first : document.activeElement === last) {
          e.preventDefault();
          (e.shiftKey ? last : first).focus();
        }
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [modalOpen, state]);

  // Notify parent when connection succeeds
  React.useEffect(() => {
    if (state === "CONNECTED" && address) {
      onConnect?.(address);
      setModalOpen(false);
    }
  }, [state, address, onConnect]);

  // Notify parent when disconnected
  React.useEffect(() => {
    if (state === "NOT_CONNECTED") {
      onDisconnect?.();
    }
  }, [state, onDisconnect]);

  const handleDisconnect = useCallback(async () => {
    setAccountMenuOpen(false);
    await disconnect();
  }, [disconnect]);

  const isConnecting = state === "CONNECTING";
  const isDisconnecting = state === "DISCONNECTING";

  // Map error code to user-facing copy (spec: docs/ux/wallet-connect.md §3.2–§3.6)
  const errorMessage = (() => {
    if (state !== "ERROR") return "";
    switch (error) {
      case "not_installed":
        return "Freighter is not installed.";
      case "rejected":
        return "Connection was declined. You can try again at any time.";
      case "timeout":
        return "The request timed out. Please try again.";
      default:
        return "Something went wrong. Please try again.";
    }
  })();

  const renderModalBody = () => {
    if (state === "ERROR") {
      return (
        <div className="flex flex-col gap-4">
          <p className="text-[#EF4444] flex items-start gap-2">
            <span aria-hidden="true">✕</span>
            {error === "not_installed" ? (
              <>
                Freighter is not installed.{" "}
                <a
                  href="https://freighter.app"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="underline text-[#2563EB]"
                >
                  Get Freighter
                </a>
              </>
            ) : (
              errorMessage
            )}
          </p>
          {error !== "not_installed" && (
            <button
              type="button"
              onClick={connect}
              className="self-start px-4 py-2 bg-[#2563EB] text-white rounded-md text-sm font-medium hover:brightness-110 focus:outline-none focus:ring-2 focus:ring-[#2563EB]"
            >
              Try Again
            </button>
          )}
        </div>
      );
    }

    return (
      <>
        <p className="text-[#64748B] text-base leading-relaxed">
          Connect your Stellar wallet to bid, fund, and settle invoices.
        </p>
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

  return (
    <>
      {/* Trigger / nav element */}
      {state === "CONNECTED" && address ? (
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

      {/* Connect Modal */}
      {modalOpen && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center sm:items-center"
          aria-modal="true"
          role="dialog"
          aria-labelledby="wc-modal-title"
        >
          <div
            className="absolute inset-0 bg-black/50"
            onClick={() => !isConnecting && setModalOpen(false)}
            aria-hidden="true"
          />
          <div
            ref={modalRef}
            className="relative w-full max-w-[480px] mx-4 bg-white rounded-lg shadow-[0_10px_15px_-3px_rgba(0,0,0,0.1)] sm:mx-auto"
          >
            <div className="flex items-center justify-between px-6 py-4 border-b border-[#E2E8F0]">
              <h2 id="wc-modal-title" className="text-2xl font-semibold text-[#0F172A]">
                Connect Wallet
              </h2>
              <button
                type="button"
                ref={firstFocusRef}
                onClick={() => setModalOpen(false)}
                aria-label="Close wallet connect modal"
                className="text-[#64748B] hover:text-[#0F172A] focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
              >
                <svg width="20" height="20" viewBox="0 0 20 20" fill="none" aria-hidden="true">
                  <path d="M5 5l10 10M15 5L5 15" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" />
                </svg>
              </button>
            </div>
            <div className="px-6 py-5">{renderModalBody()}</div>
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
                onClick={connect}
                disabled={isConnecting}
                className="flex items-center gap-2 px-4 py-2 bg-[#2563EB] text-white rounded-md text-sm font-medium hover:brightness-110 disabled:opacity-40 disabled:cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-[#2563EB]"
              >
                {isConnecting && (
                  <svg className="animate-spin h-4 w-4" viewBox="0 0 24 24" fill="none" role="status" aria-label="Connecting…">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v4a4 4 0 00-4 4H4z" />
                  </svg>
                )}
                {isConnecting ? "Connecting…" : "Connect"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Wrong-network inline banner */}
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
