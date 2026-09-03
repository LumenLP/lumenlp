export type CopyPolicyNetwork = "testnet" | "public";

const DEFAULT_POLICY_CONTRACT = "CDDEM34TOAN5DOG5LBJCC676QV2M27V3SSXZ7IPVA76RUSLSZEM5KLNJ";

function readNetwork(): CopyPolicyNetwork {
  return process.env.NEXT_PUBLIC_COPY_POLICY_NETWORK === "public" ? "public" : "testnet";
}

export type CopyPolicyConfig = {
  contractId: string;
  network: CopyPolicyNetwork;
  configured: boolean;
  executionEnabled: boolean;
  explorerUrl: string;
};

/** Sign only a server-prepared transaction; transaction construction stays off the client. */
export async function signPreparedPolicyTransaction(xdr: string, address: string) {
  const { ensureWalletKit } = await import("@/lib/wallet-kit");
  const Kit = await ensureWalletKit();
  const networkPassphrase =
    readNetwork() === "testnet"
      ? "Test SDF Network ; September 2015"
      : "Public Global Stellar Network ; September 2015";
  return Kit.signTransaction(xdr, { address, networkPassphrase });
}

/** Public client configuration only. Secrets and relayer credentials stay server-side. */
export function copyPolicyConfig(): CopyPolicyConfig {
  const network = readNetwork();
  const contractId =
    process.env.NEXT_PUBLIC_COPY_POLICY_CONTRACT?.trim() || DEFAULT_POLICY_CONTRACT;
  const configured = Boolean(contractId);

  return {
    contractId,
    network,
    configured,
    // The current contract is a testnet policy vertical slice, not a mainnet switch.
    executionEnabled: configured && network === "testnet",
    explorerUrl: `https://lab.stellar.org/r/${network}/contract/${contractId}`,
  };
}
