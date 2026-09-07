export type CopyPolicyNetwork = "testnet" | "public";
export type CopyPolicyControl = "pause" | "resume" | "disarm";

const DEFAULT_POLICY_CONTRACT = "CDDEM34TOAN5DOG5LBJCC676QV2M27V3SSXZ7IPVA76RUSLSZEM5KLNJ";

export function copyPolicyNetworkPassphrase(network: CopyPolicyNetwork): string {
  return network === "testnet"
    ? "Test SDF Network ; September 2015"
    : "Public Global Stellar Network ; September 2015";
}

function readNetwork(): CopyPolicyNetwork {
  return process.env.NEXT_PUBLIC_COPY_POLICY_NETWORK === "public" ? "public" : "testnet";
}

export type CopyPolicyConfig = {
  contractId: string;
  network: CopyPolicyNetwork;
  configured: boolean;
  executionEnabled: boolean;
  explorerUrl: string;
  rpcUrl: string;
};

const CONTROL_METHODS: Record<CopyPolicyControl, string> = {
  pause: "pause_session",
  resume: "resume_session",
  disarm: "disarm_session",
};

export function copyPolicyControlMethod(control: CopyPolicyControl): string {
  return CONTROL_METHODS[control];
}

export function copyPolicyTransactionUrl(network: CopyPolicyNetwork, hash: string): string {
  return `https://lab.stellar.org/r/${network}/tx/${encodeURIComponent(hash)}`;
}

/** Sign a prepared transaction after verifying the wallet is on the configured network. */
export async function signPreparedPolicyTransaction(xdr: string, address: string) {
  const { ensureWalletKit } = await import("@/lib/wallet-kit");
  const Kit = await ensureWalletKit();
  const networkPassphrase = copyPolicyNetworkPassphrase(readNetwork());
  const walletNetwork = await Kit.getNetwork();
  if (walletNetwork.networkPassphrase !== networkPassphrase) {
    throw new Error(`Switch the connected wallet to the LumenLP ${readNetwork()} network`);
  }
  return Kit.signTransaction(xdr, { address, networkPassphrase });
}

/** Build, authorize, submit, and confirm an owner-controlled policy action. */
export async function submitPolicyControl(
  contractId: string,
  sessionId: number,
  control: CopyPolicyControl,
  address: string,
): Promise<string> {
  const {
    BASE_FEE,
    Contract,
    TransactionBuilder,
    nativeToScVal,
    rpc,
  } = await import("@stellar/stellar-sdk/minimal");
  const config = copyPolicyConfig();
  if (!config.executionEnabled) {
    throw new Error("Copy Policy controls are only enabled on the configured testnet");
  }
  const networkPassphrase = copyPolicyNetworkPassphrase(config.network);
  const server = new rpc.Server(config.rpcUrl);
  const account = await server.getAccount(address);
  const contract = new Contract(contractId);
  const transaction = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase,
  })
    .addOperation(
      contract.call(
        copyPolicyControlMethod(control),
        nativeToScVal(sessionId, { type: "u32" }),
      ),
    )
    .setTimeout(60)
    .build();
  const prepared = await server.prepareTransaction(transaction);
  const { signedTxXdr } = await signPreparedPolicyTransaction(prepared.toXDR(), address);
  const signed = TransactionBuilder.fromXDR(signedTxXdr, networkPassphrase);
  const submitted = await server.sendTransaction(signed);
  if (submitted.status === "ERROR") {
    throw new Error("Policy transaction was rejected by Soroban RPC");
  }

  for (let attempt = 0; attempt < 20; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 1_000));
    const result = await server.getTransaction(submitted.hash);
    if (result.status === rpc.Api.GetTransactionStatus.SUCCESS) return submitted.hash;
    if (result.status === rpc.Api.GetTransactionStatus.FAILED) {
      throw new Error("Policy transaction failed on-chain");
    }
  }
  throw new Error(`Policy transaction is still pending: ${submitted.hash}`);
}

/** Public client configuration only. Secrets and relayer credentials stay server-side. */
export function copyPolicyConfig(): CopyPolicyConfig {
  const network = readNetwork();
  const contractId =
    process.env.NEXT_PUBLIC_COPY_POLICY_CONTRACT?.trim() || DEFAULT_POLICY_CONTRACT;
  const configured = Boolean(contractId);
  const rpcUrl =
    process.env.NEXT_PUBLIC_COPY_POLICY_RPC_URL?.trim() ||
    (network === "testnet"
      ? "https://soroban-testnet.stellar.org"
      : "https://soroban-rpc.mainnet.stellar.gateway.fm");

  return {
    contractId,
    network,
    configured,
    // The current contract is a testnet policy vertical slice, not a mainnet switch.
    executionEnabled: configured && network === "testnet",
    explorerUrl: `https://lab.stellar.org/r/${network}/contract/${contractId}`,
    rpcUrl,
  };
}
