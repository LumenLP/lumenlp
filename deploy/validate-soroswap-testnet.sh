#!/usr/bin/env bash
# Read-only Soroswap AMM testnet smoke test. No transaction is signed or submitted.
set -euo pipefail

RPC_URL="${RPC_URL:-https://soroban-testnet.stellar.org}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
FACTORY="${SOROSWAP_FACTORY:-CDP3HMUH6SMS3S7NPGNDJLULCOXXEPSHY4JKUKMBNQMATHDHWXRRJTBY}"
ROUTER="${SOROSWAP_ROUTER:-CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD}"

: "${SOURCE_ACCOUNT:?Set SOURCE_ACCOUNT to a funded testnet account or CLI alias}"

router_factory_json="$(stellar contract invoke \
  --id "$ROUTER" \
  --source "$SOURCE_ACCOUNT" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$NETWORK_PASSPHRASE" \
  --send no -- get_factory)"
router_factory="$(printf '%s' "$router_factory_json" | python3 -c 'import json,sys; print(json.load(sys.stdin))')"
if [ "$router_factory" != "$FACTORY" ]; then
  echo "Soroswap Router points to $router_factory, expected $FACTORY" >&2
  exit 1
fi

echo "Soroswap testnet Router/Factory match: router=$ROUTER factory=$FACTORY"
RPC_URL="$RPC_URL" \
SOURCE_ACCOUNT="$SOURCE_ACCOUNT" \
NETWORK_PASSPHRASE="$NETWORK_PASSPHRASE" \
SOROSWAP_FACTORY="$FACTORY" \
  "$(dirname "$0")/validate-soroswap.sh"
