import type { Metadata } from "next";
import "./globals.css";
import { WalletProvider } from "./lib/wallet/WalletProvider";
import { AppHeader } from "./components/AppHeader";

export const metadata: Metadata = {
  title: "QuickLendX - Decentralized Invoice Financing Platform",
  description:
    "A decentralized invoice financing platform built on Stellar's Soroban smart contract platform. Connect businesses with investors through transparent, secure invoice financing.",
  keywords: [
    "invoice financing",
    "decentralized finance",
    "DeFi",
    "Stellar",
    "Soroban",
    "blockchain",
    "invoice factoring",
  ],
  authors: [{ name: "QuickLendX Team" }],
  openGraph: {
    title: "QuickLendX - Decentralized Invoice Financing Platform",
    description:
      "Connect businesses with investors through transparent, secure invoice financing on Stellar's Soroban platform",
    type: "website",
  },
  twitter: {
    card: "summary_large_image",
    title: "QuickLendX - Decentralized Invoice Financing Platform",
    description:
      "Connect businesses with investors through transparent, secure invoice financing",
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body>
        {/*
          WalletProvider is a client component that wraps the entire app.
          AppHeader is also client-side, but page-level content remains
          server-rendered because Next.js per-page Server Components are
          not affected by a client boundary in the layout.
        */}
        <WalletProvider>
          <AppHeader />
          <main>{children}</main>
        </WalletProvider>
      </body>
    </html>
  );
}
