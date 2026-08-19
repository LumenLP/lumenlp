"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { ensureWalletKit } from "@/lib/wallet-kit";

type IdentityStatus = "idle" | "connecting" | "connected" | "disconnecting";

type IdentityCtx = {
  address: string;
  input: string;
  connected: boolean;
  status: IdentityStatus;
  setAddress: (a: string) => void;
  connectWallet: () => Promise<void>;
  disconnectWallet: () => Promise<void>;
  error: string | null;
};

const Ctx = createContext<IdentityCtx | null>(null);
const KEY = "lumenlp.activeAddress";
const LEGACY_KEY = "lpagent.activeAddress";

function isGAddress(a: string) {
  return a.startsWith("G") && a.length >= 56;
}

function isUserCancel(err: unknown): boolean {
  const msg = err instanceof Error ? err.message : String(err ?? "");
  return /cancel|closed|dismiss|abort/i.test(msg);
}

export function IdentityProvider({ children }: { children: ReactNode }) {
  const [address, setAddressState] = useState("");
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<IdentityStatus>("idle");

  const commitAddress = useCallback((next: string, nextError: string | null = null) => {
    setAddressState(next);
    setInput(next);
    setError(nextError);
    if (isGAddress(next)) {
      localStorage.setItem(KEY, next);
    } else {
      localStorage.removeItem(KEY);
    }
  }, []);

  useEffect(() => {
    const saved = localStorage.getItem(KEY) ?? localStorage.getItem(LEGACY_KEY);
    if (saved && isGAddress(saved)) {
      commitAddress(saved);
      localStorage.removeItem(LEGACY_KEY);
      setStatus("connected");
    }

    let unsubState: (() => void) | undefined;
    let unsubDisconnect: (() => void) | undefined;
    let cancelled = false;

    void (async () => {
      try {
        const Kit = await ensureWalletKit();
        const { KitEventType } = await import("@creit.tech/stellar-wallets-kit/types");
        if (cancelled) return;
        unsubState = Kit.on(KitEventType.STATE_UPDATED, (event) => {
          const next = event.payload.address?.trim() ?? "";
          if (next && isGAddress(next)) {
            commitAddress(next);
            setStatus("connected");
          }
        });
        unsubDisconnect = Kit.on(KitEventType.DISCONNECT, () => {
          commitAddress("");
          setStatus("idle");
        });
      } catch {
        /* kit init failures surface on Connect click */
      }
    })();

    return () => {
      cancelled = true;
      unsubState?.();
      unsubDisconnect?.();
    };
  }, [commitAddress]);

  const setAddress = useCallback((a: string) => {
    const trimmed = a.trim();
    if (isGAddress(trimmed)) {
      commitAddress(trimmed);
      setStatus("connected");
    } else if (trimmed.length > 0) {
      setAddressState("");
      setInput(trimmed);
      setError("Need a valid G… Stellar address");
      setStatus("idle");
    } else {
      commitAddress("");
      setStatus("idle");
    }
  }, [commitAddress]);

  const connectWallet = useCallback(async () => {
    setError(null);
    setStatus("connecting");
    try {
      const Kit = await ensureWalletKit();
      const { address: addr } = await Kit.authModal();
      const next = addr?.trim() ?? "";
      if (next && isGAddress(next)) {
        commitAddress(next);
        setStatus("connected");
        return;
      }
      setStatus("idle");
      setError("Wallet did not return a public address");
    } catch (e) {
      if (isUserCancel(e)) {
        setStatus(address ? "connected" : "idle");
        return;
      }
      setStatus(address ? "connected" : "idle");
      setError(e instanceof Error ? e.message : "Wallet connect failed");
    }
  }, [address, commitAddress]);

  const disconnectWallet = useCallback(async () => {
    setError(null);
    setStatus("disconnecting");
    try {
      const Kit = await ensureWalletKit();
      await Kit.disconnect();
    } catch {
      /* still clear local identity */
    }
    commitAddress("");
    setStatus("idle");
  }, [commitAddress]);

  const connected = isGAddress(address);

  const value = useMemo(
    () => ({
      address,
      input,
      connected,
      status,
      setAddress,
      connectWallet,
      disconnectWallet,
      error,
    }),
    [address, input, connected, status, setAddress, connectWallet, disconnectWallet, error],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useIdentity() {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useIdentity outside provider");
  return ctx;
}
