#!/usr/bin/env bash
# Exercise one Soroswap AMM Copy LP deposit on Testnet.
# Read-only preflight is the default; writes require RUN_WRITE=1.
set -euo pipefail

RPC_URL="${RPC_URL:-https://soroban-testnet.stellar.org}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
POLICY="${SOROSWAP_COPY_POLICY:?Set SOROSWAP_COPY_POLICY to the isolated testnet Copy Policy contract}"
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:?Set SOURCE_ACCOUNT to the event-recorder/owner testnet account}"
RELAYER_ACCOUNT="${RELAYER_ACCOUNT:-bob}"
ROUTER="${SOROSWAP_ROUTER:-CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD}"
POOL="${SOROSWAP_POOL:?Set SOROSWAP_POOL to an allowlisted Soroswap pair}"
LEADER="${SOROSWAP_LEADER:?Set SOROSWAP_LEADER to the registered session leader}"
SESSION_ID="${SOROSWAP_SESSION_ID:?Set SOROSWAP_SESSION_ID to the registered policy session}"
EVENT_ID="${SOROSWAP_EVENT_ID:-$(printf '%064d' "${SOROSWAP_EVENT_NUMBER:-1}")}"
AMOUNT0="${SOROSWAP_AMOUNT0:-0}"
AMOUNT1="${SOROSWAP_AMOUNT1:-0}"
QUOTE="${SOROSWAP_QUOTE:?Set SOROSWAP_QUOTE in the policy quote unit}"
KIND="${SOROSWAP_KIND:-deposit}"
SHARE_AMOUNT="${SOROSWAP_SHARE_AMOUNT:-0}"
MIN_SHARES="${SOROSWAP_MIN_SHARES:-1}"
MIN_AMOUNT0="${SOROSWAP_MIN_AMOUNT0:-0}"
MIN_AMOUNT1="${SOROSWAP_MIN_AMOUNT1:-0}"

if [[ "${STELLAR_NETWORK:-testnet}" != "testnet" ]]; then
  echo "Refusing to run: STELLAR_NETWORK must be testnet" >&2
  exit 1
fi
if [[ "$KIND" != "deposit" && "$KIND" != "withdraw" ]]; then
  echo "SOROSWAP_KIND must be deposit or withdraw" >&2
  exit 1
fi
if [[ "$KIND" == "withdraw" && "$SHARE_AMOUNT" == "0" ]]; then
  echo "SOROSWAP_SHARE_AMOUNT must be positive for withdraw" >&2
  exit 1
fi
if [[ "$KIND" == "deposit" && ( "$AMOUNT0" == "0" || "$AMOUNT1" == "0" ) ]]; then
  echo "SOROSWAP_AMOUNT0 and SOROSWAP_AMOUNT1 must be positive for deposit" >&2
  exit 1
fi

for value in "$SESSION_ID" "$AMOUNT0" "$AMOUNT1" "$QUOTE" "$SHARE_AMOUNT" "$MIN_SHARES" "$MIN_AMOUNT0" "$MIN_AMOUNT1"; do
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

invoke_pool_read() {
  stellar contract invoke --id "$POOL" --source-account "$SOURCE_ACCOUNT" \
    --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
    --send no -- "$@"
}

normalize_address() {
  python3 -c 'import json,sys; value=sys.stdin.read().strip();
try:
    parsed=json.loads(value)
    print(parsed if isinstance(parsed, str) else value)
except json.JSONDecodeError:
    print(value)'
}

schema="$(invoke_read --help 2>&1 || true)"
for command in record_leader_event execute_standard_op; do
  if ! grep -q "^[[:space:]]*${command}[[:space:]]" <<< "$schema"; then
    echo "Policy ABI is missing ${command}; deploy the current Copy Policy first" >&2
    exit 1
  fi
done

if ! configured_router="$(invoke_read venue_router --venue soroswap_amm 2>/tmp/lumenlp-soroswap-router.err | normalize_address)"; then
  echo "Copy Policy has no configured Soroswap Router; run set_venue_router first" >&2
  cat /tmp/lumenlp-soroswap-router.err >&2
  rm -f /tmp/lumenlp-soroswap-router.err
  exit 1
fi
rm -f /tmp/lumenlp-soroswap-router.err
if [[ "$configured_router" != "$ROUTER" ]]; then
  echo "Copy Policy Router is $configured_router, expected $ROUTER" >&2
  exit 1
fi
if ! token0="$(invoke_pool_read token_0 2>/tmp/lumenlp-soroswap-pair.err | normalize_address)" || \
   ! token1="$(invoke_pool_read token_1 2>>/tmp/lumenlp-soroswap-pair.err | normalize_address)"; then
  echo "Soroswap pair token read failed; check SOROSWAP_POOL" >&2
  cat /tmp/lumenlp-soroswap-pair.err >&2
  rm -f /tmp/lumenlp-soroswap-pair.err
  exit 1
fi
rm -f /tmp/lumenlp-soroswap-pair.err
if [[ -z "$token0" || -z "$token1" || "$token0" == "$token1" ]]; then
  echo "Soroswap pair returned invalid token addresses" >&2
  exit 1
fi
for token in "$token0" "$token1"; do
  if ! invoke_token_read="$(stellar contract invoke --id "$token" --source-account "$SOURCE_ACCOUNT" \
      --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
      --send no -- decimals 2>/tmp/lumenlp-soroswap-token.err)"; then
    echo "Soroswap pair token is not a readable token contract: $token" >&2
    cat /tmp/lumenlp-soroswap-token.err >&2
    rm -f /tmp/lumenlp-soroswap-token.err
    exit 1
  fi
done
rm -f /tmp/lumenlp-soroswap-token.err

session_json="$(invoke_read session --session_id "$SESSION_ID")"
if ! python3 - "$session_json" "$POOL" "$LEADER" "$QUOTE" <<'PY'
import json, sys
session = json.loads(sys.argv[1])
pool, leader, quote = sys.argv[2], sys.argv[3], int(sys.argv[4])
if session.get("leader") != leader:
    raise SystemExit("policy session leader does not match SOROSWAP_LEADER")
if pool not in session.get("allowed_pools", []):
    raise SystemExit("policy session does not allowlist SOROSWAP_POOL")
if session.get("paused"):
    raise SystemExit("policy session is paused")
if quote > int(session["max_per_op_quote"]):
    raise SystemExit("quote exceeds policy per-operation limit")
if int(session["daily_used_quote"]) + quote > int(session["max_daily_quote"]):
    raise SystemExit("quote exceeds remaining policy daily limit")
PY
then
  echo "Soroswap policy session preflight failed" >&2
  exit 1
fi

if [[ "$KIND" == "deposit" ]]; then
  for token_amount in "$token0:$AMOUNT0" "$token1:$AMOUNT1"; do
    token="${token_amount%%:*}"
    amount="${token_amount##*:}"
    balance="$(stellar contract invoke --id "$token" --source-account "$SOURCE_ACCOUNT" \
      --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
      --send no -- balance --id "$POLICY")"
    if ! python3 - "$balance" "$amount" <<'PY'
import json, sys
balance = json.loads(sys.argv[1])
if int(balance) < int(sys.argv[2]):
    raise SystemExit(1)
PY
    then
      echo "Insufficient policy token balance for $token (need $amount, have $balance)" >&2
      exit 1
    fi
  done
fi

echo "Soroswap Copy Testnet preflight passed"
echo "  policy:  $POLICY"
echo "  pool:    $POOL"
echo "  router:  $configured_router"
echo "  tokens:  $token0 / $token1"
echo "  session: $SESSION_ID"
echo "  event:   $EVENT_ID"
if [[ "$KIND" == "deposit" ]]; then
  echo "  kind:    deposit"
  echo "  amounts: [$AMOUNT0, $AMOUNT1]"
else
  echo "  kind:    withdraw"
  echo "  shares:  $SHARE_AMOUNT"
fi
echo "  quote:   $QUOTE"

if [[ "${RUN_WRITE:-0}" != "1" ]]; then
  echo "No transaction was submitted. Set RUN_WRITE=1 to record and execute this Testnet $KIND."
  exit 0
fi

invoke_write() {
  stellar contract invoke --id "$POLICY" --source-account "$1" \
    --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
    --send yes -- "${@:2}"
}

echo "Recording one Soroswap $KIND source event"
if [[ "$KIND" == "deposit" ]]; then
  event_amounts="[\"$AMOUNT0\",\"$AMOUNT1\"]"
else
  event_amounts="[\"$SHARE_AMOUNT\"]"
fi
invoke_write "$SOURCE_ACCOUNT" record_leader_event \
  --source_event_id "0x$EVENT_ID" \
  --leader "$LEADER" \
  --pool "$POOL" \
  --kind "$KIND" \
  --amounts "$event_amounts" \
  --quote "$QUOTE" \
  --ledger "${SOROSWAP_SOURCE_LEDGER:-1}"

echo "Executing policy-gated Soroswap $KIND"
if [[ "$KIND" == "deposit" ]]; then
  desired_amounts="[\"$AMOUNT0\",\"$AMOUNT1\"]"
  min_shares="$MIN_SHARES"
  share_amount=0
else
  desired_amounts='["0","0"]'
  min_shares=0
  share_amount="$SHARE_AMOUNT"
fi
invoke_write "$RELAYER_ACCOUNT" execute_standard_op \
  --venue soroswap_amm \
  --session_id "$SESSION_ID" \
  --source_event_id "0x$EVENT_ID" \
  --pool "$POOL" \
  --kind "$KIND" \
  --quote "$QUOTE" \
  --desired_amounts "$desired_amounts" \
  --min_shares "$min_shares" \
  --share_amount "$share_amount" \
  --min_amounts "[\"$MIN_AMOUNT0\",\"$MIN_AMOUNT1\"]" \
  --claim_token "$POOL"

echo "Soroswap Copy Testnet deposit submitted"
