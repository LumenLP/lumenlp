#!/usr/bin/env bash
set -euo pipefail

# Deploy a new Copy Policy test instance without touching the existing contract.
# The signer is supplied through the environment; no secret is stored here.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WASM="${ROOT_DIR}/target/wasm32v1-none/release/lumenlp_copy_policy.wasm"
RPC_URL="${STELLAR_TESTNET_RPC_URL:-https://soroban-testnet.stellar.org}"
NETWORK_PASSPHRASE="${STELLAR_TESTNET_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
SALT="${STELLAR_TESTNET_SALT:-$(printf '%s' 'lumenlp-copy-policy-generic-v4' | shasum -a 256 | awk '{print $1}')}"

: "${STELLAR_TESTNET_SOURCE:?Set STELLAR_TESTNET_SOURCE to a testnet account alias or public key}"
: "${STELLAR_TESTNET_SIGNER:?Set STELLAR_TESTNET_SIGNER to a testnet signing key or key alias}"

if [[ "${STELLAR_NETWORK:-testnet}" != "testnet" ]]; then
  echo "Refusing to deploy: STELLAR_NETWORK must be testnet" >&2
  exit 1
fi

if [[ ! "${SALT}" =~ ^[0-9a-fA-F]{64}$ ]]; then
  echo "STELLAR_TESTNET_SALT must be a 32-byte hex value (64 hex characters)" >&2
  exit 1
fi

if [[ ! -f "${WASM}" ]]; then
  echo "Missing WASM: ${WASM}" >&2
  echo "Build it with: cargo build --release --target wasm32v1-none --manifest-path contracts/copy-policy/Cargo.toml" >&2
  exit 1
fi

echo "Deploying a new LumenLP Copy Policy testnet instance"
echo "RPC: ${RPC_URL}"

stellar contract deploy \
  --wasm "${WASM}" \
  --source-account "${STELLAR_TESTNET_SOURCE}" \
  --sign-with-key "${STELLAR_TESTNET_SIGNER}" \
  --rpc-url "${RPC_URL}" \
  --network-passphrase "${NETWORK_PASSPHRASE}" \
  --salt "${SALT}"
