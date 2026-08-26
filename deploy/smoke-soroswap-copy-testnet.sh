#!/usr/bin/env bash
# Exercise one Soroswap AMM Copy LP deposit on Testnet.
# Read-only preflight is the default; writes require RUN_WRITE=1.
set -euo pipefail

RPC_URL="${RPC_URL:-https://soroban-testnet.stellar.org}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
POLICY="${SOROSWAP_COPY_POLICY:?Set SOROSWAP_COPY_POLICY to the isolated testnet Copy Policy contract}"
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:?Set SOURCE_ACCOUNT to the event-recorder/owner testnet account}"
RELAYER_ACCOUNT="${RELAYER_ACCOUNT:-bob}"
POOL="${SOROSWAP_POOL:?Set SOROSWAP_POOL to an allowlisted Soroswap pair}"
LEADER="${SOROSWAP_LEADER:?Set SOROSWAP_LEADER to the registered session leader}"
SESSION_ID="${SOROSWAP_SESSION_ID:?Set SOROSWAP_SESSION_ID to the registered policy session}"
EVENT_ID="${SOROSWAP_EVENT_ID:-$(printf '%064d' "${SOROSWAP_EVENT_NUMBER:-1}")}"
AMOUNT0="${SOROSWAP_AMOUNT0:?Set SOROSWAP_AMOUNT0 in token0 stroops}"
AMOUNT1="${SOROSWAP_AMOUNT1:?Set SOROSWAP_AMOUNT1 in token1 stroops}"
QUOTE="${SOROSWAP_QUOTE:?Set SOROSWAP_QUOTE in the policy quote unit}"
MIN_SHARES="${SOROSWAP_MIN_SHARES:-1}"
MIN_AMOUNT0="${SOROSWAP_MIN_AMOUNT0:-0}"
MIN_AMOUNT1="${SOROSWAP_MIN_AMOUNT1:-0}"

if [[ "${STELLAR_NETWORK:-testnet}" != "testnet" ]]; then
  echo "Refusing to run: STELLAR_NETWORK must be testnet" >&2
  exit 1
fi

for value in "$SESSION_ID" "$AMOUNT0" "$AMOUNT1" "$QUOTE" "$MIN_SHARES" "$MIN_AMOUNT0" "$MIN_AMOUNT1"; do
  if [[ ! "$value" =~ ^[0-9]+$ ]]; then
    echo "Expected unsigned integer, got: $value" >&2
    exit 1
  fi
done
if [[ ! "$EVENT_ID" =~ ^[0-9a-fA-F]{64}$ ]]; then
  echo "SOROSWAP_EVENT_ID must be exactly 32 bytes as 64 hex characters" >&2
  exit 1
fi

invoke_read() {
  stellar contract invoke --id "$POLICY" --source-account "$SOURCE_ACCOUNT" \
    --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
    --send no -- "$@"
}

schema="$(invoke_read --help 2>&1 || true)"
for command in record_leader_event execute_standard_op; do
  if ! grep -q "^[[:space:]]*${command}[[:space:]]" <<< "$schema"; then
    echo "Policy ABI is missing ${command}; deploy the current Copy Policy first" >&2
    exit 1
  fi
done

echo "Soroswap Copy Testnet preflight passed"
echo "  policy:  $POLICY"
echo "  pool:    $POOL"
echo "  session: $SESSION_ID"
echo "  event:   $EVENT_ID"
echo "  amounts: [$AMOUNT0, $AMOUNT1]"
echo "  quote:   $QUOTE"

if [[ "${RUN_WRITE:-0}" != "1" ]]; then
  echo "No transaction was submitted. Set RUN_WRITE=1 to record and execute this Testnet deposit."
  exit 0
fi

invoke_write() {
  stellar contract invoke --id "$POLICY" --source-account "$1" \
    --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
    --send yes -- "${@:2}"
}

echo "Recording one Soroswap deposit source event"
invoke_write "$SOURCE_ACCOUNT" record_leader_event \
  --source_event_id "0x$EVENT_ID" \
  --leader "$LEADER" \
  --pool "$POOL" \
  --kind deposit \
  --amounts "[\"$AMOUNT0\",\"$AMOUNT1\"]" \
  --quote "$QUOTE" \
  --ledger "${SOROSWAP_SOURCE_LEDGER:-1}"

echo "Executing policy-gated Soroswap deposit"
invoke_write "$RELAYER_ACCOUNT" execute_standard_op \
  --venue soroswap_amm \
  --session_id "$SESSION_ID" \
  --source_event_id "0x$EVENT_ID" \
  --pool "$POOL" \
  --kind deposit \
  --quote "$QUOTE" \
  --desired_amounts "[\"$AMOUNT0\",\"$AMOUNT1\"]" \
  --min_shares "$MIN_SHARES" \
  --share_amount 0 \
  --min_amounts "[\"$MIN_AMOUNT0\",\"$MIN_AMOUNT1\"]" \
  --claim_token "$POOL"

echo "Soroswap Copy Testnet deposit submitted"
