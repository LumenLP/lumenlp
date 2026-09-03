#!/usr/bin/env bash
# Consume one Copy LP recorder task on Stellar Testnet.
# Dry-run is the default. This script never permits a mainnet submission.
set -euo pipefail

RPC_URL="${STELLAR_TESTNET_RPC_URL:-https://soroban-testnet.stellar.org}"
NETWORK_PASSPHRASE="${STELLAR_TESTNET_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
DATABASE_PATH="${COPY_INDEX_DB_PATH:-./data/pool-indexer.db}"
POLICY="${COPY_POLICY:?Set COPY_POLICY to the isolated testnet Copy Policy contract}"
RECORDER_ACCOUNT="${COPY_RECORDER_ACCOUNT:?Set COPY_RECORDER_ACCOUNT to the event-recorder signer}"
RELAYER_ACCOUNT="${COPY_RELAYER_ACCOUNT:?Set COPY_RELAYER_ACCOUNT to the policy relayer signer}"

if [[ "${STELLAR_NETWORK:-testnet}" != "testnet" ]]; then
  echo "Refusing to run: STELLAR_NETWORK must be testnet" >&2
  exit 1
fi
if [[ ! -f "$DATABASE_PATH" ]]; then
  echo "Missing index database: $DATABASE_PATH" >&2
  exit 1
fi

row="$(sqlite3 -separator '|' "$DATABASE_PATH" \
  "SELECT o.source_event_id, o.leader_address, o.pool_address, o.kind,
          o.claim_token, o.amounts_json, o.quote_stroops, o.ledger,
          c.session_id, s.contract_session_id, c.status, c.scaled_quote_xlm,
          c.scaled_amounts_json
     FROM recorder_outbox o
     JOIN copy_ops c ON c.source_event_id = o.source_event_id
     JOIN copy_sessions s ON s.id = c.session_id
    WHERE o.status = 'pending' AND c.status = 'pending'
    ORDER BY o.created_at ASC
    LIMIT 1;")"

if [[ -z "$row" ]]; then
  echo "No pending Copy operation found."
  exit 0
fi

IFS='|' read -r source_event_id leader pool kind claim_token amounts quote_stroops ledger session_id db_contract_session_id op_status scaled_quote_xlm scaled_amounts <<< "$row"
if [[ ! "$source_event_id" =~ ^[A-Za-z0-9._:-]+$ ]]; then
  echo "Unsupported source event ID characters" >&2
  exit 1
fi
if [[ "$op_status" != "pending" || -z "$session_id" ]]; then
  echo "Invalid pending Copy operation row" >&2
  exit 1
fi
if [[ "$kind" != "deposit" && "$kind" != "withdraw" && "$kind" != "claim" ]]; then
  echo "Refusing to execute unsupported operation '$kind'" >&2
  exit 1
fi
if [[ "$kind" == "claim" && ! "$claim_token" =~ ^C[A-Z2-7]{55}$ ]]; then
  echo "Refusing claim: recorder payload has no valid reward token" >&2
  exit 1
fi
contract_session_id="${db_contract_session_id:-${COPY_CONTRACT_SESSION_ID:-}}"
if [[ "${RUN_WRITE:-0}" == "1" && ! "$contract_session_id" =~ ^[0-9]+$ ]]; then
  echo "Refusing to write: COPY_CONTRACT_SESSION_ID must be the registered Soroban u32 session ID" >&2
  exit 1
fi

source_event_key="$(python3 -c 'import sys; value=sys.argv[1].encode("ascii"); assert len(value)<=32, "source event ID exceeds 32 bytes"; print((value+b"\x00"*(32-len(value))).hex())' "$source_event_id")"

amounts_vec="$(python3 -c 'import json,sys; rows=json.loads(sys.argv[1]); assert isinstance(rows,list) and rows, "event amounts must be a non-empty JSON array"; values=[str(row.get("amount") if isinstance(row,dict) else row) for row in rows]; assert all(value.isdigit() for value in values), "event amount must be an unsigned integer"; print(json.dumps(values,separators=(",",":")))' "$amounts")"
scaled_amounts_vec="$(python3 -c 'import json,sys; rows=json.loads(sys.argv[1]); assert isinstance(rows,list) and rows, "scaled amounts must be a non-empty JSON array"; values=[str(row.get("amount") if isinstance(row,dict) else row) for row in rows]; assert all(value.isdigit() for value in values), "scaled amount must be an unsigned integer"; print(json.dumps(values,separators=(",",":")))' "$scaled_amounts")"

if [[ "$kind" == "deposit" ]]; then
  desired_amounts_vec="$scaled_amounts_vec"
  share_amount="0"
else
  desired_amounts_vec='["0","0"]'
  share_amount="$(python3 -c 'import json,sys; rows=json.loads(sys.argv[1]); assert rows, "withdraw scaled amounts are empty"; value=str(rows[0].get("amount") if isinstance(rows[0],dict) else rows[0]); assert value.isdigit(), "withdraw share amount must be an unsigned integer"; print(value)' "$scaled_amounts")"
fi
min_amounts_vec='["0","0"]'

scaled_quote_stroops="$(python3 -c 'import decimal,sys; value=decimal.Decimal(sys.argv[1]); assert value.is_finite() and value>0, "scaled quote must be positive"; print(int((value*decimal.Decimal(10000000)).to_integral_value(rounding=decimal.ROUND_FLOOR)))' "$scaled_quote_xlm")"

echo "Copy relayer candidate"
echo "  source event: $source_event_id"
echo "  replay key:   $source_event_key"
echo "  local session: $session_id"
echo "  chain session: ${contract_session_id:-not configured}"
echo "  leader:       $leader"
echo "  pool:         $pool"
echo "  kind:         $kind"
echo "  source quote: $quote_stroops stroops"
echo "  scaled quote: $scaled_quote_stroops stroops"
echo "  amounts:      $amounts_vec"

if [[ "${RUN_WRITE:-0}" != "1" ]]; then
  echo "Dry-run only. Set RUN_WRITE=1 to submit this operation on Testnet."
  exit 0
fi
if [[ "${COPY_ALLOW_ZERO_MIN_OUTPUTS:-0}" != "1" ]]; then
  echo "Refusing to write without explicit zero-min-output acknowledgement" >&2
  echo "Set COPY_ALLOW_ZERO_MIN_OUTPUTS=1 only for an isolated Testnet fixture; add session slippage limits before production use." >&2
  exit 1
fi

# Refuse writes against an older policy instance before touching either the
# recorder or execution entry point. The CLI returns a non-zero status for
# --help on some contract versions, so validate the printed schema explicitly.
SCHEMA="$(stellar contract invoke --id "$POLICY" --source-account "$RECORDER_ACCOUNT" \
  --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
  --send no -- --help 2>&1 || true)"
for command in record_leader_event record_claim_event execute_aquarius_standard_op; do
  if ! grep -q "^[[:space:]]*${command}[[:space:]]" <<< "$SCHEMA"; then
    echo "Policy ABI is missing ${command}; deploy the current Copy Policy first" >&2
    exit 1
  fi
done

invoke() {
  stellar contract invoke --id "$POLICY" --source-account "$1" \
    --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
    --send yes -- "${@:2}"
}

echo "Recording canonical source event on testnet"
if [[ "$kind" == "claim" ]]; then
  if ! output="$(invoke "$RECORDER_ACCOUNT" record_claim_event \
    --source_event_id "$source_event_key" \
    --leader "$leader" \
    --pool "$pool" \
    --amounts "$amounts_vec" \
    --quote "$quote_stroops" \
    --ledger "$ledger" \
    --claim_token "$claim_token" 2>&1)"; then
    sqlite3 "$DATABASE_PATH" "UPDATE recorder_outbox SET status='pending', last_error='record_claim_event failed', updated_at=strftime('%s','now') WHERE source_event_id='$source_event_id';" || true
    echo "$output" >&2
    exit 1
  fi
else
  if ! output="$(invoke "$RECORDER_ACCOUNT" record_leader_event \
    --source_event_id "$source_event_key" \
    --leader "$leader" \
    --pool "$pool" \
    --kind "$kind" \
    --amounts "$amounts_vec" \
    --quote "$quote_stroops" \
    --ledger "$ledger" 2>&1)"; then
    sqlite3 "$DATABASE_PATH" "UPDATE recorder_outbox SET status='pending', last_error='record_leader_event failed', updated_at=strftime('%s','now') WHERE source_event_id='$source_event_id';" || true
    echo "$output" >&2
    exit 1
  fi
fi
if [[ -z "${output:-}" ]]; then
  sqlite3 "$DATABASE_PATH" "UPDATE recorder_outbox SET status='pending', last_error='record_leader_event failed', updated_at=strftime('%s','now') WHERE source_event_id='$source_event_id';" || true
  echo "Recorder invocation returned no output" >&2
  exit 1
fi
echo "$output"

echo "Executing Aquarius policy-gated Copy operation on testnet"
if ! output="$(invoke "$RELAYER_ACCOUNT" execute_aquarius_standard_op \
  --session_id "$contract_session_id" \
  --source_event_id "$source_event_key" \
  --pool "$pool" \
  --kind "$kind" \
  --quote "$scaled_quote_stroops" \
  --desired_amounts "$desired_amounts_vec" \
  --min_shares 0 \
  --share_amount "$share_amount" \
  --min_amounts "$min_amounts_vec" \
  --claim_token "${claim_token:-$pool}" 2>&1)"; then
  echo "$output" >&2
  sqlite3 "$DATABASE_PATH" "UPDATE recorder_outbox SET status='pending', last_error='execute_copy_op failed', updated_at=strftime('%s','now') WHERE source_event_id='$source_event_id';" || true
  exit 1
fi
echo "$output"

if ! sqlite3 "$DATABASE_PATH" <<SQL
BEGIN;
UPDATE recorder_outbox
   SET status = 'submitted', last_error = NULL, updated_at = strftime('%s','now')
 WHERE source_event_id = '$source_event_id';
UPDATE copy_ops
   SET status = 'signed', note = 'testnet relayer submitted policy execution', updated_at = strftime('%s','now')
 WHERE source_event_id = '$source_event_id' AND status = 'pending';
COMMIT;
SQL
then
  echo "On-chain Copy succeeded but local status reconciliation failed" >&2
  exit 1
fi
echo "Copy relayer submitted one testnet policy operation."
