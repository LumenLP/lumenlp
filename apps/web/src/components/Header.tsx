"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useIdentity } from "@/lib/identity";

export function Header() {
  const pathname = usePathname();
  const {
    address,
    input,
    connected,
    status,
    setAddress,
    connectWallet,
    disconnectWallet,
    error,
  } = useIdentity();
  const busy = status === "connecting" || status === "disconnecting";
  const buttonLabel = connected
    ? status === "disconnecting"
      ? "Disconnecting..."
      : "Disconnect"
    : status === "connecting"
      ? "Connecting..."
      : "Connect Wallet";

  return (
    <header className="nav">
      <div className="nav-left">
        <Link href="/" className="brand" aria-label="LumenLP home">
          <span className="brand-mark" aria-hidden="true">
            %
          </span>
          <span className="brand-name">LumenLP</span>
        </Link>
        <nav className="nav-links" aria-label="Primary">
          <Link href="/pools" className={pathname?.startsWith("/pools") ? "active" : ""}>
            Pools
          </Link>
          <Link href="/leaders" className={pathname?.startsWith("/leaders") ? "active" : ""}>
            Leaders
          </Link>
          <Link
            href="/strategies"
            className={pathname?.startsWith("/strategies") ? "active" : ""}
          >
            Strategies
          </Link>
          <Link href="/copy" className={pathname?.startsWith("/copy") ? "active" : ""}>
            Copy
          </Link>
        </nav>
      </div>

      <div className="identity">
        {!connected ? (
          <input
            className="identity-input"
            placeholder="Paste G… address"
            value={input}
            onChange={(e) => setAddress(e.target.value)}
            spellCheck={false}
            aria-label="Stellar address"
          />
        ) : (
          <span className="status-pill ok" title={address}>
            {address.slice(0, 6)}…{address.slice(-4)}
          </span>
        )}
        {connected ? (
          <button
            type="button"
            className="primary"
            onClick={() => void disconnectWallet()}
            disabled={busy}
          >
            {buttonLabel}
          </button>
        ) : (
          <button
            type="button"
            className="primary"
            onClick={() => void connectWallet()}
            disabled={busy}
          >
            {buttonLabel}
          </button>
        )}
        {error ? <span className="error inline-error">{error}</span> : null}
      </div>
    </header>
  );
}
