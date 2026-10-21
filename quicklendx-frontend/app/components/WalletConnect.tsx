“use client”;

import React, { useCallback, useEffect, useRef, useState } from “react”;
import {
trackWalletConnectAttempt,
trackWalletConnected,
trackWalletConnectFailed,
trackWalletDisconnected,
} from “../lib/wallet-telemetry”;

// —————————————————————————
// Types
// —————————————————————————

export type WalletConnectionState =
| “NOT_CONNECTED”
| “CONNECTING”
| “CONNECTED”
| “WRONG_NETWORK”
| “ERROR”
| “DISCONNECTING”;

export interface WalletConnectProps {
/** Expected network: “public” (Mainnet) or “testnet”. /
expectedNetwork?: “public” | “testnet”;
/* Called when the user successfully connects. Receives the public key. /
onConnect?: (publicKey: string) => void;
/* Called when the user disconnects. */
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

// —————————————————————————
// Copy button
// —————————————————————————

/** Copy-to-clipboard button with feedback and error handling. */
export const CopyButton: React.FC<{ value: string }> = ({ value }) => {
const [copied, setCopied] = useState(false);
const [errorMessage, setErrorMessage] = useState<string | null>(null);
const liveRef = useRef(null);
const timeoutRef = useRef<ReturnType | null>(null);

useEffect(() => {
return () => {
if (timeoutRef.current !== null) {
clearTimeout(timeoutRef.current);
}
};
}, []);

const handleCopy = useCallback(
async (e: React.MouseEvent) => {
e.stopPropagation();
setErrorMessage(null);

  try {
    if (!navigator.clipboard?.writeText) {
      throw new Error("Clipboard API not available");
    }
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

/**

* Displays the connected wallet as a group of sibling controls.
* The account trigger opens a keyboard-accessible menu.
    */
    export const ConnectedAccount: React.FC = ({
    address,
    network,
    onDisconnect,
    disconnecting = false,
    }) => {
    const [accountMenuOpen, setAccountMenuOpen] = useState(false);

const rootRef = useRef(null);
const triggerRef = useRef(null);
const explorerRef = useRef(null);
const disconnectRef = useRef(null);

const closeMenu = useCallback((restoreFocus = false) => {
setAccountMenuOpen(false);

if (restoreFocus) {
  requestAnimationFrame(() => {
    triggerRef.current?.focus();
  });
}

}, []);

// Close the menu when the user clicks or taps outside the entire component.
useEffect(() => {
if (!accountMenuOpen) return;

const handleOutsidePointerDown = (event: PointerEvent) => {
  const target = event.target;
  if (
    target instanceof Node &&
    rootRef.current &&
    !rootRef.current.contains(target)
  ) {
    setAccountMenuOpen(false);
  }
};
const handleDocumentKeyDown = (event: KeyboardEvent) => {
  if (event.key === "Escape") {
    event.preventDefault();
    closeMenu(true);
  }
};
document.addEventListener("pointerdown", handleOutsidePointerDown);
document.addEventListener("keydown", handleDocumentKeyDown);
return () => {
  document.removeEventListener("pointerdown", handleOutsidePointerDown);
  document.removeEventListener("keydown", handleDocumentKeyDown);
};

}, [accountMenuOpen, closeMenu]);

// Focus the first menu item whenever the menu opens.
useEffect(() => {
if (accountMenuOpen) {
disconnectRef.current?.focus();
}
}, [accountMenuOpen]);

const handleTriggerKeyDown = (event: React.KeyboardEvent) => {
if (
event.key === “ArrowDown” &&
!accountMenuOpen
) {
event.preventDefault();
setAccountMenuOpen(true);
}
};

const handleMenuKeyDown = (event: React.KeyboardEvent) => {
if (event.key === “Tab”) {
closeMenu(false);
}

if (event.key === "ArrowDown" || event.key === "ArrowUp") {
  event.preventDefault();
  const items = [disconnectRef.current, explorerRef.current].filter(
    (item): item is HTMLButtonElement | HTMLAnchorElement => item !== null
  );
  const currentIndex = items.indexOf(
    document.activeElement as HTMLButtonElement | HTMLAnchorElement
  );
  const direction = event.key === "ArrowDown" ? 1 : -1;
  const nextIndex =
    currentIndex === -1
      ? 0
      : (currentIndex + direction + items.length) % items.length;
  items[nextIndex]?.focus();
}
if (event.key === "Home") {
  event.preventDefault();
  disconnectRef.current?.focus();
}
if (event.key === "End") {
  event.preventDefault();
  explorerRef.current?.focus();
}

};

return (
  {/* Account menu trigger */}
  <button
    ref={triggerRef}
    type="button"
    aria-haspopup="menu"
    aria-expanded={accountMenuOpen}
    aria-controls="connected-account-menu"
    onClick={() => setAccountMenuOpen((open) => !open)}
    onKeyDown={handleTriggerKeyDown}
    className="flex items-center gap-1 rounded hover:text-neutral-900 focus:outline-none focus:ring-2 focus:ring-[#2563EB]"
  >
    <span>{truncateAddress(address)}</span>
    <svg
      width="12"
      height="12"
      viewBox="0 0 16 16"
      fill="none"
      aria-hidden="true"
      className={`transition-transform duration-200 ${
        accountMenuOpen ? "rotate-180" : ""
      }`}
    >
      <path
        d="M4 6l4 4 4-4"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  </button>
  {/* Copy is a sibling control, not nested inside the trigger. */}
  <CopyButton value={address} />
  {/* Explorer link is also a sibling control. */}
  <a
    href={explorerUrl(address, network)}
    target="_blank"
    rel="noopener noreferrer"
    aria-label="View wallet on Stellar Explorer (opens in a new tab)"
    className="ml-1 rounded px-1 text-xs text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB]"
  >
    Explorer ↗
  </a>
  {accountMenuOpen && (
    <div
      id="connected-account-menu"
      role="menu"
      aria-label="Wallet account actions"
      onKeyDown={handleMenuKeyDown}
      className="absolute right-0 top-full z-50 mt-2 w-48 rounded-lg border border-[#E2E8F0] bg-white py-1 font-sans text-xs shadow-lg"
    >
      <button
        ref={disconnectRef}
        role="menuitem"
        type="button"
        disabled={disconnecting}
        onClick={() => {
          closeMenu(false);
          onDisconnect?.();
        }}
        className="w-full px-4 py-2 text-left text-red-600 transition-colors hover:bg-red-50 disabled:cursor-not-allowed disabled:opacity-50"
      >
        {disconnecting ? "Disconnecting…" : "Disconnect"}
      </button>
    </div>
  )}
</div>

);
};
