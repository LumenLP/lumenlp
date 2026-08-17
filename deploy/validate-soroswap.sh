#!/usr/bin/env bash
# Read-only Soroswap AMM mainnet smoke test. No transaction is signed or submitted.
set -euo pipefail

RPC_URL="${RPC_URL:-http://127.0.0.1:8003}"
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:-GBS3LFM2PIMRGZUC65G2GNMSWTQIX3FYSKB7ZF62ZLPVLG7MDXFUHQ64}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Public Global Stellar Network ; September 2015}"
FACTORY="${SOROSWAP_FACTORY:-CA4HEQTL2WPEUYKYKCDOHCDNIV4QHNJ7EL4J4NQ6VADP7SYHVRYZ7AW2}"
INDEX="${SOROSWAP_PAIR_INDEX:-0}"

invoke() {
  if [ -n "${3:-}" ]; then
    stellar contract invoke --id "$1" --source "$SOURCE_ACCOUNT" \
      --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
      --send no -- "$2" --n "$3"
  else
    stellar contract invoke --id "$1" --source "$SOURCE_ACCOUNT" \
      --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
      --send no -- "$2"
  fi
}

length_json="$(invoke "$FACTORY" all_pairs_length)"
pair_count="$(printf '%s' "$length_json" | python3 -c 'import json,sys; print(int(json.load(sys.stdin)))')"
if [ "$pair_count" -le 0 ]; then
  echo "Soroswap factory returned no pairs" >&2
  exit 1
fi
if [ "$INDEX" -ge "$pair_count" ]; then
  echo "Soroswap pair index $INDEX is outside pair count $pair_count" >&2
  exit 1
fi

pair_json="$(invoke "$FACTORY" all_pairs "$INDEX")"
pair="$(printf '%s' "$pair_json" | python3 -c 'import json,sys; print(json.load(sys.stdin))')"
token_0="$(invoke "$pair" token_0)"
token_1="$(invoke "$pair" token_1)"
reserves="$(invoke "$pair" get_reserves)"

python3 - "$pair_count" "$pair" "$token_0" "$token_1" "$reserves" <<'PY'
import json
import sys

count = int(sys.argv[1])
pair = sys.argv[2]
token_0 = json.loads(sys.argv[3])
token_1 = json.loads(sys.argv[4])
reserves = json.loads(sys.argv[5])
if not pair or not token_0 or not token_1:
    raise SystemExit("Soroswap pair or token query returned an empty value")
if not isinstance(reserves, list) or len(reserves) < 2:
    raise SystemExit("Soroswap get_reserves returned fewer than two values")
if any(int(value) < 0 for value in reserves[:2]):
    raise SystemExit("Soroswap returned a negative reserve")
print(f"Soroswap smoke test passed: pairs={count} pair={pair}")
print(f"tokens={token_0} / {token_1}")
print(f"reserves={reserves[0]} / {reserves[1]} fee_bps=30")
PY
