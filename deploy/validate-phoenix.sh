#!/usr/bin/env bash
# Read-only Phoenix mainnet smoke test. No transaction is signed or submitted.
set -euo pipefail

RPC_URL="${RPC_URL:-http://127.0.0.1:8003}"
SOURCE_ACCOUNT="${SOURCE_ACCOUNT:-GBS3LFM2PIMRGZUC65G2GNMSWTQIX3FYSKB7ZF62ZLPVLG7MDXFUHQ64}"
NETWORK_PASSPHRASE="${NETWORK_PASSPHRASE:-Public Global Stellar Network ; September 2015}"
FACTORY="${PHOENIX_FACTORY:-CB4SVAWJA6TSRNOJZ7W2AWFW46D5VR4ZMFZKDIKXEINZCZEGZCJZCKMI}"
POOL="${PHOENIX_POOL:-CBENABXP6C4C7WG6KB7JQOTDS5GIIXF3IX3PIYNZFCDZDWUHITO2HZ4S}"

invoke() {
  stellar contract invoke \
    --id "$1" \
    --source "$SOURCE_ACCOUNT" \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$NETWORK_PASSPHRASE" \
    --send no -- "$2"
}

factory_json="$(invoke "$FACTORY" query_all_pools_details)"
pool_count="$(printf '%s' "$factory_json" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')"
if [ "$pool_count" -eq 0 ]; then
  echo "Phoenix factory returned no pools" >&2
  exit 1
fi

config_json="$(invoke "$POOL" query_config)"
pool_json="$(invoke "$POOL" query_pool_info)"
python3 - "$pool_count" "$config_json" "$pool_json" <<'PY'
import json
import sys

count = int(sys.argv[1])
config = json.loads(sys.argv[2])
pool = json.loads(sys.argv[3])
required_config = ("pool_type", "total_fee_bps", "token_a", "token_b", "share_token")
required_pool = ("asset_a", "asset_b", "asset_lp_share")
missing = [key for key in required_config if key not in config]
missing += [key for key in required_pool if key not in pool]
if missing:
    raise SystemExit(f"missing Phoenix fields: {', '.join(missing)}")
if config["pool_type"] not in (0, 1):
    raise SystemExit(f"unsupported Phoenix pool_type: {config['pool_type']}")
print(f"Phoenix smoke test passed: pools={count} pool_type={config['pool_type']} fee_bps={config['total_fee_bps']}")
print(f"tokens={config['token_a']} / {config['token_b']}")
print(f"reserves={pool['asset_a']['amount']} / {pool['asset_b']['amount']}")
PY
