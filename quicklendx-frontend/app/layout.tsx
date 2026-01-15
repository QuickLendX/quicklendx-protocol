import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "QuickLendX - Decentralized Invoice Financing Platform",
  description: "A decentralized invoice financing platform built on Stellar's Soroban smart contract platform. Connect businesses with investors through transparent, secure invoice financing.",
  keywords: ["invoice financing", "decentralized finance", "DeFi", "Stellar", "Soroban", "blockchain", "invoice factoring"],
  authors: [{ name: "QuickLendX Team" }],
  openGraph: {
    title: "QuickLendX - Decentralized Invoice Financing Platform",
    description: "Connect businesses with investors through transparent, secure invoice financing on Stellar's Soroban platform",
    type: "website",
  },
  twitter: {
    card: "summary_large_image",
    title: "QuickLendX - Decentralized Invoice Financing Platform",
    description: "Connect businesses with investors through transparent, secure invoice financing",
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
        {children}
      </body>
    </html>
  );
}
