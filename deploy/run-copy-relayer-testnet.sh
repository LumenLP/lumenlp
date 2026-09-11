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
LOCK_FILE="${COPY_RELAYER_LOCK_FILE:-${DATABASE_PATH}.copy-relayer.lock}"

# A timer retry or manual invocation must not submit the same pending row in
# parallel. Soroban replay protection is a last line of defense, not a local
# job coordination mechanism.
mkdir -p "$(dirname "$LOCK_FILE")"
exec 9>"$LOCK_FILE"
if ! flock -n 9; then
  echo "Another Copy relayer instance is already running."
  exit 0
fi

if [[ "${STELLAR_NETWORK:-testnet}" != "testnet" ]]; then
  echo "Refusing to run: STELLAR_NETWORK must be testnet" >&2
  exit 1
fi
if [[ ! -f "$DATABASE_PATH" ]]; then
  echo "Missing index database: $DATABASE_PATH" >&2
  exit 1
fi
if [[ ! "$POLICY" =~ ^C[A-Z2-7]{55}$ ]]; then
  echo "Refusing to run: COPY_POLICY must be a contract address" >&2
  exit 1
fi

# A pending operation can be skipped or rejected after another session has
# already used the same source event. Do not leave that outbox row looking
# perpetually backlogged once no executable operation references it.
sqlite3 "$DATABASE_PATH" <<'SQL'
UPDATE recorder_outbox
   SET status = CASE
         WHEN EXISTS (
           SELECT 1 FROM recorder_deliveries d
            WHERE d.source_event_id = recorder_outbox.source_event_id
              AND d.status = 'recorded'
         ) THEN 'submitted'
         ELSE 'cancelled'
       END,
       updated_at = strftime('%s','now')
 WHERE status = 'pending'
   AND NOT EXISTS (
     SELECT 1 FROM copy_ops c
      WHERE c.source_event_id = recorder_outbox.source_event_id
        AND c.status = 'pending'
   );
SQL

row="$(sqlite3 -separator '|' "$DATABASE_PATH" \
  "SELECT c.id, o.source_event_id, o.leader_address, o.pool_address, o.kind,
          o.claim_token, o.amounts_json, o.quote_stroops, o.ledger,
          c.session_id, s.contract_session_id, c.status, c.scaled_quote_xlm,
          c.scaled_amounts_json, COALESCE(d.status, 'pending')
     FROM recorder_outbox o
     JOIN copy_ops c ON c.source_event_id = o.source_event_id
     JOIN copy_sessions s ON s.id = c.session_id
     LEFT JOIN recorder_deliveries d
       ON d.source_event_id = o.source_event_id
      AND d.contract_address = s.contract_address
    WHERE c.status = 'pending'
      AND s.contract_address = '$POLICY'
      AND COALESCE(d.status, 'pending') IN ('pending', 'recorded')
    ORDER BY o.created_at ASC
    LIMIT 1;")"

if [[ -z "$row" ]]; then
  echo "No pending Copy operation found."
  exit 0
fi

IFS='|' read -r op_id source_event_id leader pool kind claim_token amounts quote_stroops ledger session_id db_contract_session_id op_status scaled_quote_xlm scaled_amounts recorder_status <<< "$row"
if [[ ! "$op_id" =~ ^[A-Za-z0-9._:-]+$ ]]; then
  echo "Unsupported Copy operation ID characters" >&2
  exit 1
fi
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
  if [[ "$kind" == "withdraw" ]] && ! share_amount="$(python3 -c 'import json,sys; source=json.loads(sys.argv[1]); scaled=json.loads(sys.argv[2]); assert isinstance(source,list) and len(source)==1 and str(source[0]).isdigit(), "withdraw recorder payload must contain exactly one LP share amount"; assert isinstance(scaled,list) and len(scaled)==1 and isinstance(scaled[0],dict) and scaled[0].get("unit")=="lp_shares", "withdraw scaled amount must be tagged as LP shares"; value=str(scaled[0].get("amount")); assert value.isdigit(), "withdraw share amount must be an unsigned integer"; print(value)' "$amounts" "$scaled_amounts")"; then
    sqlite3 "$DATABASE_PATH" "UPDATE copy_ops SET status='rejected', note='withdraw_share_amount_missing: regenerate operation from indexed LP shares', updated_at=strftime('%s','now') WHERE id='$op_id' AND status='pending';" || true
    echo "Refusing withdrawal: queue payload does not contain canonical LP shares" >&2
    exit 1
  fi
  share_amount="${share_amount:-0}"
fi
min_amounts_vec='["0","0"]'

scaled_quote_stroops="$(python3 -c 'import decimal,sys; value=decimal.Decimal(sys.argv[1]); assert value.is_finite() and value>0, "scaled quote must be positive"; print(int((value*decimal.Decimal(10000000)).to_integral_value(rounding=decimal.ROUND_FLOOR)))' "$scaled_quote_xlm")"

echo "Copy relayer candidate"
echo "  source event: $source_event_id"
echo "  copy op:      $op_id"
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

reconcile_execution() {
  local receipt_hash="$1"
  local receipt_note="$2"
  sqlite3 "$DATABASE_PATH" <<SQL
BEGIN;
UPDATE copy_ops
   SET status = 'executed', tx_hash = NULLIF('$receipt_hash', ''),
       note = '$receipt_note', updated_at = strftime('%s','now')
 WHERE id = '$op_id' AND status = 'pending';
UPDATE recorder_outbox
   SET status = CASE
         WHEN EXISTS (
           SELECT 1 FROM copy_ops
            WHERE source_event_id = '$source_event_id' AND status = 'pending'
         ) THEN 'pending'
         ELSE 'submitted'
       END,
       last_error = NULL, updated_at = strftime('%s','now')
 WHERE source_event_id = '$source_event_id';
COMMIT;
SQL
}

if [[ "$recorder_status" != "recorded" ]]; then
  sqlite3 "$DATABASE_PATH" \
    "INSERT OR IGNORE INTO recorder_deliveries (source_event_id, contract_address, status, attempts, created_at, updated_at) VALUES ('$source_event_id', '$POLICY', 'pending', 0, strftime('%s','now'), strftime('%s','now'));"
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
      sqlite3 "$DATABASE_PATH" "UPDATE recorder_deliveries SET status='pending', attempts=attempts+1, last_error='record_claim_event failed', updated_at=strftime('%s','now') WHERE source_event_id='$source_event_id' AND contract_address='$POLICY';" || true
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
      sqlite3 "$DATABASE_PATH" "UPDATE recorder_deliveries SET status='pending', attempts=attempts+1, last_error='record_leader_event failed', updated_at=strftime('%s','now') WHERE source_event_id='$source_event_id' AND contract_address='$POLICY';" || true
      echo "$output" >&2
      exit 1
    fi
  fi
  if [[ -z "${output:-}" ]]; then
    sqlite3 "$DATABASE_PATH" "UPDATE recorder_deliveries SET status='pending', attempts=attempts+1, last_error='record_leader_event failed', updated_at=strftime('%s','now') WHERE source_event_id='$source_event_id' AND contract_address='$POLICY';" || true
    echo "Recorder invocation returned no output" >&2
    exit 1
  fi
  sqlite3 "$DATABASE_PATH" "UPDATE recorder_deliveries SET status='recorded', attempts=attempts+1, last_error=NULL, updated_at=strftime('%s','now') WHERE source_event_id='$source_event_id' AND contract_address='$POLICY';"
  echo "$output"
else
  echo "Canonical source event already recorded for this policy."
fi

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
  # A network/CLI failure can happen after the transaction committed. Newer
  # policy builds expose the replay receipt so local state can be reconciled
  # without submitting the DEX operation again.
  if grep -q '^[[:space:]]*copy_executed[[:space:]]' <<< "$SCHEMA"; then
    consumed="$(stellar contract invoke --id "$POLICY" --source-account "$RELAYER_ACCOUNT" \
      --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
      --send no -- copy_executed --session_id "$contract_session_id" \
      --source_event_id "$source_event_key" 2>/dev/null || true)"
    if [[ "$(tr -d '[:space:]' <<< "$consumed")" == "true" ]]; then
      if ! reconcile_execution "" "testnet relayer recovered confirmed on-chain execution"; then
        echo "On-chain Copy is confirmed but local status reconciliation failed" >&2
        exit 1
      fi
      echo "Recovered an already-confirmed testnet Copy execution."
      exit 0
    fi
  fi
  sqlite3 "$DATABASE_PATH" "UPDATE recorder_deliveries SET status='recorded', last_error='execute_copy_op failed', updated_at=strftime('%s','now') WHERE source_event_id='$source_event_id' AND contract_address='$POLICY';" || true
  exit 1
fi
echo "$output"

# The Stellar CLI prints the submitted transaction hash in its receipt. Keep
# it when available so the API can expose an auditable execution link; the
# operation remains executable-status even if an older CLI omits the hash.
tx_hash="$(grep -Eio '[0-9a-f]{64}' <<< "$output" | tail -1 || true)"

if ! reconcile_execution "$tx_hash" "testnet relayer confirmed policy execution"; then
  echo "On-chain Copy succeeded but local status reconciliation failed" >&2
  exit 1
fi
echo "Copy relayer submitted one testnet policy operation."
