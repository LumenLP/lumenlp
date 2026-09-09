#!/usr/bin/env bash
set -euo pipefail

test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT

db="$test_dir/copy.db"
bin="$test_dir/bin"
log="$test_dir/stellar.log"
mkdir -p "$bin"

sqlite3 "$db" <<'SQL'
CREATE TABLE recorder_outbox (
  source_event_id TEXT PRIMARY KEY, leader_address TEXT, pool_address TEXT,
  kind TEXT, claim_token TEXT, amounts_json TEXT, quote_stroops TEXT,
  ledger INTEGER, status TEXT, attempts INTEGER DEFAULT 0, last_error TEXT,
  created_at INTEGER, updated_at INTEGER
);
CREATE TABLE recorder_deliveries (
  source_event_id TEXT, contract_address TEXT, status TEXT,
  attempts INTEGER DEFAULT 0, last_error TEXT, created_at INTEGER,
  updated_at INTEGER, PRIMARY KEY (source_event_id, contract_address)
);
CREATE TABLE copy_sessions (
  id TEXT PRIMARY KEY, contract_address TEXT, contract_session_id INTEGER
);
CREATE TABLE copy_ops (
  id TEXT PRIMARY KEY, session_id TEXT, source_event_id TEXT, pool_address TEXT,
  kind TEXT, scaled_quote_xlm REAL, scaled_amounts_json TEXT, status TEXT,
  note TEXT, tx_hash TEXT, created_at INTEGER, updated_at INTEGER
);
INSERT INTO recorder_outbox VALUES
  ('event-1', 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
   'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM', 'deposit', NULL,
   '["100","200"]', '30000000', 123, 'pending', 0, NULL, 1, 1),
  ('event-2', 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
   'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM', 'deposit', NULL,
   '["10","20"]', '3000000', 124, 'pending', 0, NULL, 2, 2);
INSERT INTO copy_sessions VALUES
  ('session-a', 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM', 41),
  ('session-b', 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM', 42);
INSERT INTO copy_ops VALUES
  ('op-a', 'session-a', 'event-1', 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM',
   'deposit', 1.5, '["50","100"]', 'pending', NULL, NULL, 1, 1),
  ('op-b', 'session-b', 'event-1', 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM',
   'deposit', 0.75, '["25","50"]', 'pending', NULL, NULL, 2, 2),
  ('op-c', 'session-a', 'event-2', 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM',
   'deposit', 0.15, '["5","10"]', 'skipped', NULL, NULL, 3, 3);
SQL

cat >"$bin/stellar" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$STELLAR_TEST_LOG"
if [[ "$*" == *"--help"* ]]; then
  printf '  record_leader_event \n  record_claim_event \n  execute_aquarius_standard_op \n'
elif [[ "$*" == *"execute_aquarius_standard_op"* ]]; then
  printf '%064d\n' 2
else
  printf '%064d\n' 1
fi
SH
chmod +x "$bin/stellar"
cat >"$bin/flock" <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod +x "$bin/flock"

assert_eq() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [[ "$actual" != "$expected" ]]; then
    echo "$label: expected '$expected', got '$actual'" >&2
    exit 1
  fi
}

run_relayer() {
  PATH="$bin:$PATH" STELLAR_TEST_LOG="$log" RUN_WRITE=1 \
    COPY_ALLOW_ZERO_MIN_OUTPUTS=1 STELLAR_NETWORK=testnet \
    COPY_POLICY=CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM \
    COPY_RECORDER_ACCOUNT=recorder COPY_RELAYER_ACCOUNT=relayer \
    COPY_INDEX_DB_PATH="$db" COPY_RELAYER_LOCK_FILE="$test_dir/relayer.lock" \
    "$(dirname "$0")/run-copy-relayer-testnet.sh" >/dev/null
}

run_relayer
assert_eq executed "$(sqlite3 "$db" "SELECT status FROM copy_ops WHERE id='op-a'")" "first operation"
assert_eq pending "$(sqlite3 "$db" "SELECT status FROM copy_ops WHERE id='op-b'")" "second operation after first run"
assert_eq pending "$(sqlite3 "$db" "SELECT status FROM recorder_outbox WHERE source_event_id='event-1'")" "outbox after first run"
assert_eq cancelled "$(sqlite3 "$db" "SELECT status FROM recorder_outbox WHERE source_event_id='event-2'")" "unused outbox cleanup"

run_relayer
assert_eq executed,executed "$(sqlite3 "$db" "SELECT group_concat(status, ',') FROM (SELECT status FROM copy_ops WHERE source_event_id='event-1' ORDER BY id)")" "operations after second run"
assert_eq submitted "$(sqlite3 "$db" "SELECT status FROM recorder_outbox WHERE source_event_id='event-1'")" "outbox after second run"
assert_eq 1 "$(grep -c 'record_leader_event' "$log")" "recorder call count"
assert_eq 2 "$(grep -c 'execute_aquarius_standard_op --session_id' "$log")" "execution call count"

sqlite3 "$db" <<'SQL'
INSERT INTO recorder_outbox VALUES
  ('event-3', 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF',
   'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM', 'withdraw', NULL,
   '["9000","8000"]', '30000000', 125, 'pending', 0, NULL, 4, 4);
INSERT INTO copy_ops VALUES
  ('op-d', 'session-a', 'event-3', 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM',
   'withdraw', 1.5, '[{"token":"CA","amount":"4500"}]', 'pending', NULL, NULL, 4, 4);
SQL
if run_relayer 2>/dev/null; then
  echo "legacy withdrawal unexpectedly reached execution" >&2
  exit 1
fi
assert_eq rejected "$(sqlite3 "$db" "SELECT status FROM copy_ops WHERE id='op-d'")" "legacy withdrawal"
assert_eq 2 "$(grep -c 'execute_aquarius_standard_op --session_id' "$log")" "execution count after rejected withdrawal"

echo "copy relayer multi-session test passed"
