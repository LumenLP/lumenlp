#!/usr/bin/env bash
# Configure an isolated Copy Policy instance on Stellar Testnet.
# No action is selected by default; every write is explicit.
set -euo pipefail

RPC_URL="${STELLAR_TESTNET_RPC_URL:-https://soroban-testnet.stellar.org}"
NETWORK_PASSPHRASE="${STELLAR_TESTNET_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
POLICY="${COPY_POLICY:-}"
OWNER_ACCOUNT="${STELLAR_TESTNET_SOURCE:-}"
ACTION="${COPY_POLICY_ACTION:-help}"

if [[ "${STELLAR_NETWORK:-testnet}" != "testnet" ]]; then
  echo "Refusing to run: STELLAR_NETWORK must be testnet" >&2
  exit 1
fi

invoke() {
  : "${POLICY:?Set COPY_POLICY to the isolated testnet policy contract}"
  : "${OWNER_ACCOUNT:?Set STELLAR_TESTNET_SOURCE to the owner account alias}"
  stellar contract invoke --id "$POLICY" --source-account "$OWNER_ACCOUNT" \
    --rpc-url "$RPC_URL" --network-passphrase "$NETWORK_PASSPHRASE" \
    --send yes -- "${@}"
}

case "$ACTION" in
  initialize)
    : "${COPY_OWNER_ADDRESS:?Set COPY_OWNER_ADDRESS to the policy owner public key}"
    : "${COPY_RELAYER_ADDRESS:?Set COPY_RELAYER_ADDRESS to the relayer public key}"
    echo "Initializing testnet Copy Policy"
    invoke initialize --owner "$COPY_OWNER_ADDRESS" --relayer "$COPY_RELAYER_ADDRESS"
    ;;
  set-recorder)
    : "${COPY_RECORDER_ADDRESS:?Set COPY_RECORDER_ADDRESS to the recorder public key}"
    echo "Configuring testnet event recorder"
    invoke set_event_recorder --recorder "$COPY_RECORDER_ADDRESS"
    ;;
  register-session)
    : "${COPY_SESSION_ID:?Set COPY_SESSION_ID to a positive u32}"
    : "${COPY_LEADER_ADDRESS:?Set COPY_LEADER_ADDRESS to the tracked Leader public key}"
    : "${COPY_POOL_ADDRESS:?Set COPY_POOL_ADDRESS to an allowlisted testnet pool}"
    : "${COPY_EXPIRES_AT:?Set COPY_EXPIRES_AT to a future Unix timestamp}"
    COEFFICIENT_PPM="${COPY_COEFFICIENT_PPM:-100000}"
    FOLLOW_CLAIMS="${COPY_FOLLOW_CLAIMS:-false}"
    MAX_PER_OP="${COPY_MAX_PER_OP_QUOTE:-1000000}"
    MAX_DAILY="${COPY_MAX_DAILY_QUOTE:-5000000}"
    echo "Registering testnet Copy Policy session ${COPY_SESSION_ID}"
    invoke register_session_coeff \
      --session_id "$COPY_SESSION_ID" \
      --leader "$COPY_LEADER_ADDRESS" \
      --allowed_pools "[\"$COPY_POOL_ADDRESS\"]" \
      --coefficient_ppm "$COEFFICIENT_PPM" \
      --follow_claims "$FOLLOW_CLAIMS" \
      --max_per_op_quote "$MAX_PER_OP" \
      --max_daily_quote "$MAX_DAILY" \
      --expires_at "$COPY_EXPIRES_AT"
    ;;
  set-router)
    : "${COPY_VENUE:?Set COPY_VENUE to soroswap_amm or soroswap}"
    : "${COPY_ROUTER_ADDRESS:?Set COPY_ROUTER_ADDRESS to the allowlisted Testnet Router}"
    if [[ "$COPY_VENUE" != "soroswap" && "$COPY_VENUE" != "soroswap_amm" ]]; then
      echo "COPY_VENUE must be soroswap or soroswap_amm" >&2
      exit 1
    fi
    echo "Configuring ${COPY_VENUE} Router on the isolated Testnet policy"
    invoke set_venue_router \
      --venue "$COPY_VENUE" \
      --router "$COPY_ROUTER_ADDRESS"
    ;;
  help)
    cat <<'EOF'
Set COPY_POLICY_ACTION to one of: initialize, set-recorder, register-session,
or set-router.
All actions write to Stellar Testnet and require explicit environment values.
EOF
    ;;
  *)
    echo "Unknown COPY_POLICY_ACTION: $ACTION" >&2
    exit 1
    ;;
esac
