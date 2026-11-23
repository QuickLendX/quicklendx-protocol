"use client";

/**
 * AppHeader — global site header rendered on every route.
 *
 * Contains:
 * - Logo linking to "/"
 * - <WalletConnect /> for wallet connection UI
 *
 * Must be wrapped in <WalletProvider> (done in app/layout.tsx).
 */

import React from "react";
import Link from "next/link";
import { WalletConnect } from "./WalletConnect";

export function AppHeader() {
  return (
    <header className="sticky top-0 z-30 w-full border-b border-[#E2E8F0] bg-white/90 backdrop-blur-sm">
      <div className="mx-auto flex h-14 max-w-7xl items-center justify-between px-4 sm:px-6 lg:px-8">
        {/* Logo */}
        <Link
          href="/"
          className="flex items-center gap-2 text-[#0F172A] font-semibold text-lg focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
          aria-label="QuickLendX — home"
        >
          {/* Wordmark */}
          <span className="text-[#2563EB] font-bold tracking-tight">
            Quick<span className="text-[#0F172A]">LendX</span>
          </span>
        </Link>

        {/* Wallet connect */}
        <WalletConnect />
      </div>
    </header>
  );
}

export default AppHeader;
