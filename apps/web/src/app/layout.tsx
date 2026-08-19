import type { Metadata } from "next";
import { IdentityProvider } from "@/lib/identity";
import { Header } from "@/components/Header";
import "./globals.css";

export const metadata: Metadata = {
  title: "LumenLP — Stellar LP auto-rebalance",
  description:
    "Auto-rebalance Stellar LP positions with strategy rules, previews, and RPC-first pool analytics.",
  icons: {
    icon: "/icon.svg",
    shortcut: "/icon.svg",
  },
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <head>
        <link rel="preconnect" href="https://fonts.googleapis.com" />
        <link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="" />
        <link
          href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap"
          rel="stylesheet"
        />
      </head>
      <body>
        <IdentityProvider>
          <div className="shell">
            <Header />
            {children}
          </div>
        </IdentityProvider>
      </body>
    </html>
  );
}
