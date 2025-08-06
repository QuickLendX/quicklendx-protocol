import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "QuickLendX - Invoice Financing Platform",
  description: "A decentralized invoice financing platform built on blockchain technology",
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
