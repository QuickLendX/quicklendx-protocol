"use client";

/**
 * AppHeader — global site header rendered on every route.
 *
 * Contains the home logo, primary section navigation, and <WalletConnect />
 * for wallet connection UI.
 *
 * Must be wrapped in <WalletProvider> (done in app/layout.tsx).
 */

import React from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { WalletConnect } from "./WalletConnect";

export function AppHeader() {
  const pathname = usePathname();
  const [isMenuOpen, setIsMenuOpen] = React.useState(false);
  const links = [
    { href: "/invest", label: "Invest" },
    { href: "/business/invoices", label: "Business invoices" },
    { href: "/business/settlements", label: "Settlements" },
  ];

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

        <nav aria-label="Primary" className="relative ml-auto mr-4">
          <button
            type="button"
            className="rounded px-3 py-2 text-sm font-medium text-[#0F172A] focus:outline-none focus:ring-2 focus:ring-[#2563EB] md:hidden"
            aria-expanded={isMenuOpen}
            aria-controls="primary-navigation"
            onClick={() => setIsMenuOpen((open) => !open)}
          >
            Menu
          </button>
          <div
            id="primary-navigation"
            className={`${isMenuOpen ? "flex" : "hidden"} absolute right-0 top-full z-40 min-w-48 flex-col gap-1 rounded-md border border-[#E2E8F0] bg-white p-2 shadow-lg md:static md:flex md:min-w-0 md:flex-row md:items-center md:gap-6 md:border-0 md:bg-transparent md:p-0 md:shadow-none`}
          >
            {links.map(({ href, label }) => {
              const isActive =
                pathname === href || pathname?.startsWith(`${href}/`);

              return (
                <Link
                  key={href}
                  href={href}
                  aria-current={isActive ? "page" : undefined}
                  onClick={() => setIsMenuOpen(false)}
                  className={`rounded px-3 py-2 text-sm font-medium focus:outline-none focus:ring-2 focus:ring-[#2563EB] ${
                    isActive
                      ? "text-[#2563EB]"
                      : "text-[#475569] hover:text-[#0F172A]"
                  }`}
                >
                  {label}
                </Link>
              );
            })}
          </div>
        </nav>

        {/* Wallet connect */}
        <WalletConnect />
      </div>
    </header>
  );
}

export default AppHeader;
