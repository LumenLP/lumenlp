#!/usr/bin/env bash
# Prepare isolated Soroswap Copy Policy testnet balances.
#
# Default mode is read-only. Set RUN_WRITE_SMOKE=1 to mint test assets to the
# policy contract. The token-admin signer is supplied by the caller and is
# never persisted or printed.
set -euo pipefail

RPC_URL="${RPC_URL:-https://soroban-testnet.stellar.org}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
FACTORY="${SOROSWAP_FACTORY:-CDP3HMUH6SMS3S7NPGNDJLULCOXXEPSHY4JKUKMBNQMATHDHWXRRJTBY}"
ROUTER="${SOROSWAP_ROUTER:-CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD}"
POLICY="${SOROSWAP_COPY_POLICY:?Set SOROSWAP_COPY_POLICY to the isolated testnet Copy Policy contract}"
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:?Set SOURCE_ACCOUNT to a funded testnet account or CLI alias}"
TOKEN0="${SOROSWAP_TOKEN0:-CB3TLW74NBIOT3BUWOZ3TUM6RFDF6A4GVIRUQRQZABG5KPOUL4JJOV2F}"
TOKEN1="${SOROSWAP_TOKEN1:-CCZGLAUBDKJSQK72QOZHVU7CUWKW45OZWYWCLL27AEK74U2OIBK6LXF2}"
AMOUNT0="${SOROSWAP_TOKEN0_AMOUNT:-1000000}"
AMOUNT1="${SOROSWAP_TOKEN1_AMOUNT:-1000000}"

if [[ "${STELLAR_NETWORK:-testnet}" != "testnet" ]]; then
  echo "Refusing to prepare balances: STELLAR_NETWORK must be testnet" >&2
  exit 1
fi

echo "Checking Soroswap testnet Router/Factory relationship"
router_factory_json="$(stellar contract invoke \
  --id "$ROUTER" \
  --source "$SOURCE_ACCOUNT" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --send no -- get_factory)"
router_factory="$(printf '%s' "$router_factory_json" | python3 -c 'import json,sys; print(json.load(sys.stdin))')"
if [[ "$router_factory" != "$FACTORY" ]]; then
  echo "Soroswap Router points to $router_factory, expected $FACTORY" >&2
  exit 1
fi

echo "Checking isolated Copy Policy Router configuration"
configured_router_json="$(stellar contract invoke \
  --id "$POLICY" \
  --source "$SOURCE_ACCOUNT" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --send no -- venue_router --venue soroswap_amm)"
configured_router="$(printf '%s' "$configured_router_json" | python3 -c 'import json,sys; print(json.load(sys.stdin))')"
if [[ "$configured_router" != "$ROUTER" ]]; then
  echo "Copy Policy Router is $configured_router, expected $ROUTER" >&2
  exit 1
fi

echo "Read-only preflight passed: policy=$POLICY router=$ROUTER"

if [[ "${RUN_WRITE_SMOKE:-0}" != "1" ]]; then
  cat <<'EOF'
No transaction was submitted.
Set RUN_WRITE_SMOKE=1 and provide SOROSWAP_TOKEN_ADMIN_SOURCE plus
SOROSWAP_TOKEN_ADMIN_SIGNER to mint the configured test assets to the policy.
EOF
  exit 0
fi

: "${SOROSWAP_TOKEN_ADMIN_SOURCE:?Set SOROSWAP_TOKEN_ADMIN_SOURCE for write mode}"
: "${SOROSWAP_TOKEN_ADMIN_SIGNER:?Set SOROSWAP_TOKEN_ADMIN_SIGNER for write mode}"

if [[ ! "$AMOUNT0" =~ ^[1-9][0-9]*$ || ! "$AMOUNT1" =~ ^[1-9][0-9]*$ ]]; then
  echo "Token amounts must be positive integer stroops" >&2
  exit 1
fi

echo "Minting isolated test assets to Copy Policy (testnet only)"
stellar contract invoke \
  --id "$TOKEN0" \
  --source "$SOROSWAP_TOKEN_ADMIN_SOURCE" \
  --sign-with-key "$SOROSWAP_TOKEN_ADMIN_SIGNER" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --send yes -- mint --to "$POLICY" --amount "$AMOUNT0"
stellar contract invoke \
  --id "$TOKEN1" \
  --source "$SOROSWAP_TOKEN_ADMIN_SOURCE" \
  --sign-with-key "$SOROSWAP_TOKEN_ADMIN_SIGNER" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --send yes -- mint --to "$POLICY" --amount "$AMOUNT1"

echo "Policy balances after provisioning"
for token in "$TOKEN0" "$TOKEN1"; do
  stellar contract invoke \
    --id "$token" \
    --source "$SOURCE_ACCOUNT" \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    --send no -- balance --id "$POLICY"
done

echo "Asset provisioning complete. This script does not execute a Copy LP operation."
