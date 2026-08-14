"use client";

import { useState } from "react";

export type TokenVisual = {
  key: string;
  label: string;
  name?: string | null;
  issuer?: string | null;
  domain?: string | null;
  icon?: string | null;
  seed?: string;
};

type MetaToken = {
  address: string;
  symbol: string;
  name?: string | null;
  issuer?: string | null;
  domain?: string | null;
  icon?: string | null;
};

function hashHue(seed: string) {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return h % 360;
}

function avatarStyle(seed: string) {
  const hue = hashHue(seed || "token");
  return { background: `hsl(${hue} 42% 32%)` };
}

function initials(label: string) {
  const cleaned = label.replace(/^native$/i, "XLM").trim();
  if (!cleaned) return "?";
  const parts = cleaned.split(/[\/\s_-]+/).filter(Boolean);
  if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
  return cleaned.slice(0, 2).toUpperCase();
}

export function tokenVisualsFromMeta(
  meta: MetaToken[] | null | undefined,
  tokens: string[] | null | undefined,
): TokenVisual[] {
  if (meta && meta.length > 0) {
    return meta.map((token) => ({
      key: token.address,
      label: token.symbol?.trim() || token.address.slice(0, 4),
      name: token.name,
      issuer: token.issuer,
      domain: token.domain,
      icon: token.icon,
      seed: token.address,
    }));
  }
  return (tokens ?? []).slice(0, 4).map((address) => ({
    key: address,
    label: address.slice(0, 4),
    seed: address,
  }));
}

function TokenAvatar({ token, sizeClass }: { token: TokenVisual; sizeClass: string }) {
  const [broken, setBroken] = useState(false);
  const seed = token.seed || token.key;
  return (
    <span className={`token-mark ${sizeClass}`}>
      <span className="token-avatar" style={avatarStyle(seed)} title={token.label}>
        {token.icon && !broken ? (
          // eslint-disable-next-line @next/next/no-img-element
          <img
            className="token-avatar-img"
            src={token.icon}
            alt={token.label}
            onError={() => setBroken(true)}
          />
        ) : (
          initials(token.label)
        )}
      </span>
    </span>
  );
}

export function TokenPairMark({
  tokens,
  size = "md",
}: {
  tokens: TokenVisual[];
  size?: "sm" | "md" | "lg";
}) {
  const sizeClass = `token-mark-${size}`;
  return (
    <span className="pair-heading" style={{ gap: 0 }}>
      {tokens.map((token) => (
        <TokenAvatar key={token.key} token={token} sizeClass={sizeClass} />
      ))}
    </span>
  );
}
