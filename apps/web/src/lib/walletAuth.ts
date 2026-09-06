"use client";

import { postJson } from "./api";
import { ensureWalletKit } from "./wallet-kit";

type WalletAuthToken = {
  token: string;
  expires_at: number;
  address: string;
};

type WalletAuthChallenge = {
  challenge_id: string;
  message: string;
  expires_at: number;
};

const STORAGE_PREFIX = "lumenlp.walletAuth.v1.";
const inflight = new Map<string, Promise<string>>();

function storageKey(address: string) {
  return `${STORAGE_PREFIX}${address}`;
}

function cachedToken(address: string): string | null {
  if (typeof window === "undefined") return null;
  const raw = sessionStorage.getItem(storageKey(address));
  if (!raw) return null;
  try {
    const value = JSON.parse(raw) as WalletAuthToken;
    if (value.address === address && value.token && value.expires_at > Date.now() / 1000 + 30) {
      return value.token;
    }
  } catch {
    // Invalid local state is replaced by a fresh wallet challenge.
  }
  sessionStorage.removeItem(storageKey(address));
  return null;
}

async function authenticate(address: string): Promise<string> {
  const challenge = await postJson<WalletAuthChallenge>("/v1/auth/challenge", { address });
  const Kit = await ensureWalletKit();
  const { signedMessage, signerAddress } = await Kit.signMessage(challenge.message, {
    address,
    networkPassphrase: "Public Global Stellar Network ; September 2015",
  });
  if (signerAddress && signerAddress !== address) {
    throw new Error("The signing wallet does not match the connected follower account.");
  }
  const auth = await postJson<WalletAuthToken>("/v1/auth/verify", {
    challenge_id: challenge.challenge_id,
    address,
    signed_message: signedMessage,
  });
  sessionStorage.setItem(storageKey(address), JSON.stringify(auth));
  return auth.token;
}

export function walletAuthToken(address: string): Promise<string> {
  const cached = cachedToken(address);
  if (cached) return Promise.resolve(cached);
  const active = inflight.get(address);
  if (active) return active;
  const request = authenticate(address).finally(() => inflight.delete(address));
  inflight.set(address, request);
  return request;
}

export async function walletAuthHeaders(address: string): Promise<Record<string, string>> {
  return { Authorization: `Bearer ${await walletAuthToken(address)}` };
}
