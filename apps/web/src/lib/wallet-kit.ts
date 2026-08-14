"use client";

import type { StellarWalletsKit as KitType } from "@creit.tech/stellar-wallets-kit/sdk";

let initPromise: Promise<typeof KitType> | null = null;

/** Lazily init Stellar Wallets Kit once (browser only). */
export function ensureWalletKit(): Promise<typeof KitType> {
  if (typeof window === "undefined") {
    return Promise.reject(new Error("Wallet kit is browser-only"));
  }
  if (!initPromise) {
    initPromise = (async () => {
      const { StellarWalletsKit } = await import("@creit.tech/stellar-wallets-kit/sdk");
      const { defaultModules } = await import(
        "@creit.tech/stellar-wallets-kit/modules/utils"
      );
      const { SwkAppDarkTheme, Networks } = await import(
        "@creit.tech/stellar-wallets-kit/types"
      );
      StellarWalletsKit.init({
        modules: defaultModules(),
        network: Networks.PUBLIC,
        theme: SwkAppDarkTheme,
      });
      return StellarWalletsKit;
    })();
  }
  return initPromise;
}
