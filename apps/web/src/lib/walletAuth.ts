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
const authEpochs = new Map<string, number>();

function authEpoch(address: string) {
  return authEpochs.get(address) ?? 0;
}

function invalidateAuth(address: string) {
  authEpochs.set(address, authEpoch(address) + 1);
  inflight.delete(address);
}

function storageKey(address: string) {
  return `${STORAGE_PREFIX}${address}`;
}

function storedToken(address: string): WalletAuthToken | null {
  if (typeof window === "undefined") return null;
  const raw = sessionStorage.getItem(storageKey(address));
  if (!raw) return null;
  try {
    const value = JSON.parse(raw) as WalletAuthToken;
    if (value.address === address && value.token) return value;
  } catch {
    // Invalid local state is replaced by a fresh wallet challenge.
  }
  sessionStorage.removeItem(storageKey(address));
  return null;
}

function cachedToken(address: string): string | null {
  const value = storedToken(address);
  if (value && value.expires_at > Date.now() / 1000 + 30) return value.token;
  if (typeof window !== "undefined") sessionStorage.removeItem(storageKey(address));
  return null;
}

async function authenticate(address: string, epoch: number): Promise<string> {
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
  if (authEpoch(address) !== epoch) {
    await postJson<{ revoked: boolean }>("/v1/auth/revoke", undefined, {
      Authorization: `Bearer ${auth.token}`,
    }).catch(() => undefined);
    throw new Error("Wallet authentication was cancelled.");
  }
  sessionStorage.setItem(storageKey(address), JSON.stringify(auth));
  return auth.token;
}

export function walletAuthToken(address: string): Promise<string> {
  const cached = cachedToken(address);
  if (cached) return Promise.resolve(cached);
  const active = inflight.get(address);
  if (active) return active;
  const request = authenticate(address, authEpoch(address));
  inflight.set(address, request);
  const cleanup = () => {
    if (inflight.get(address) === request) inflight.delete(address);
  };
  void request.then(cleanup, cleanup);
  return request;
}

export async function walletAuthHeaders(address: string): Promise<Record<string, string>> {
  return { Authorization: `Bearer ${await walletAuthToken(address)}` };
}

export function clearWalletAuth(address?: string): void {
  if (typeof window === "undefined") return;
  if (address) {
    invalidateAuth(address);
    sessionStorage.removeItem(storageKey(address));
    return;
  }
  const addresses = new Set(inflight.keys());
  Object.keys(sessionStorage).forEach((key) => {
    if (!key.startsWith(STORAGE_PREFIX)) return;
    addresses.add(key.slice(STORAGE_PREFIX.length));
    sessionStorage.removeItem(key);
  });
  addresses.forEach(invalidateAuth);
}

export async function revokeWalletAuth(address: string): Promise<void> {
  const auth = storedToken(address);
  clearWalletAuth(address);
  if (!auth) return;
  await postJson<{ revoked: boolean }>("/v1/auth/revoke", undefined, {
    Authorization: `Bearer ${auth.token}`,
  });
}
