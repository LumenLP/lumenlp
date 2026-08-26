#!/usr/bin/env bash
# Exercise the Copy Policy authorization path on Stellar Testnet only.
# This script never provisions assets and never calls a DEX operation.
set -euo pipefail

RPC_URL="${RPC_URL:-https://soroban-testnet.stellar.org}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
POLICY="${COPY_POLICY:?Set COPY_POLICY to an isolated testnet Copy Policy contract}"
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:?Set SOURCE_ACCOUNT to a funded testnet CLI alias or account}"
RELAYER_ACCOUNT="${RELAYER_ACCOUNT:-bob}"
LEADER="${LEADER:-alice}"
POOL="${POOL:?Set POOL to one allowlisted testnet pool or pair}"
SESSION_ID="${SESSION_ID:-42}"
EVENT_ID="${EVENT_ID:-0000000000000000000000000000000000000000000000000000000000000042}"
EXPIRY="${EXPIRY:-2000000000}"

if [[ "${STELLAR_NETWORK:-testnet}" != "testnet" ]]; then
  echo "Refusing to run: STELLAR_NETWORK must be testnet" >&2
  exit 1
fi

invoke() {
  stellar contract invoke --id "$POLICY" --source-account "$1" \
    --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
    --send yes -- "${@:2}"
}

# Check the embedded contract schema before any state-changing call. This
# prevents accidentally running the current smoke flow against an older policy
# instance that predates the recorder boundary.
SCHEMA="$(stellar contract invoke --id "$POLICY" --source-account "$SOURCE_ACCOUNT" \
  --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
  --send no -- --help 2>&1 || true)"
for command in set_event_recorder record_leader_event execute_copy_op; do
  if ! grep -q "^[[:space:]]*${command}[[:space:]]" <<< "$SCHEMA"; then
    echo "Policy ABI is missing ${command}; deploy the current Copy Policy first" >&2
    exit 1
  fi
done

echo "Configuring event recorder"
invoke "$SOURCE_ACCOUNT" set_event_recorder --recorder "$SOURCE_ACCOUNT"

echo "Registering isolated policy session $SESSION_ID"
invoke "$SOURCE_ACCOUNT" register_session \
  --session_id "$SESSION_ID" \
  --leader "$LEADER" \
  --allowed_pools "[\"$POOL\"]" \
  --follow_claims false \
  --max_per_op_quote 10 \
  --max_daily_quote 10 \
  --expires_at "$EXPIRY"

echo "Recording one synthetic deposit event"
invoke "$SOURCE_ACCOUNT" record_leader_event \
  --source_event_id "$EVENT_ID" \
  --leader "$LEADER" \
  --pool "$POOL" \
  --kind deposit \
  --amounts '["10"]' \
  --quote 10 \
  --ledger 100000000

echo "Consuming event through the relayer authorization gate"
invoke "$RELAYER_ACCOUNT" execute_copy_op \
  --session_id "$SESSION_ID" \
  --source_event_id "$EVENT_ID" \
  --pool "$POOL" \
  --kind deposit \
  --quote 10

echo "Checking replay protection"
if invoke "$RELAYER_ACCOUNT" execute_copy_op \
  --session_id "$SESSION_ID" \
  --source_event_id "$EVENT_ID" \
  --pool "$POOL" \
  --kind deposit \
  --quote 10; then
  echo "Replay unexpectedly succeeded" >&2
  exit 1
fi

echo "Copy Policy testnet smoke passed: allowlist, relayer auth, quota, and replay protection"
