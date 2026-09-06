# Copy Relayer Runbook

This runbook covers the first Copy LP execution path. It is intentionally
testnet-only until the Soroban policy instance, recorder authority, relayer
authority, and DEX operation fixtures have been validated together.

## Wallet authentication

The API exposes a SEP-53 challenge exchange as the authentication foundation
for Copy LP controls:

1. `POST /v1/auth/challenge` creates a five-minute, account-bound message.
2. The wallet signs the exact message using SEP-53 message signing.
3. `POST /v1/auth/verify` consumes the challenge and returns a 15-minute opaque token.
4. The database stores only the SHA-256 token hash; expired records are pruned during challenge creation.

Challenges are single-use and bind the message, account, nonce, and expiry.
Creating or updating a Copy session, preparing an operation, and updating an
operation status require the resulting bearer token. The authenticated account
must also match the request's follower address and the stored session owner.

## Boundary

The API and indexer create durable `recorder_outbox` and `copy_ops` rows. They
do not hold signing keys. The relayer reads one pending operation, records its
canonical source event on the Copy Policy contract, and submits the
Aquarius-specific `execute_aquarius_standard_op` entry point. Soroban remains
the final authority for replay, session, pool, action, expiry, and quote-limit
checks.

The helper executes Aquarius deposits, withdrawals, and claims. A claim is
accepted only when the recorder payload contains one unambiguous reward-token
address; multi-token or incomplete claim events remain fail-closed. The
execution call uses coefficient-scaled amounts from `copy_ops`,
not the raw Leader amounts stored in `recorder_outbox`.

## Dry run

Run from the repository root with a copy of the index database or the
server-side database path:

```sh
STELLAR_NETWORK=testnet \
COPY_POLICY=<testnet-policy-contract> \
COPY_RECORDER_ACCOUNT=<recorder-signer> \
COPY_RELAYER_ACCOUNT=<relayer-signer> \
COPY_INDEX_DB_PATH=./data/pool-indexer.db \
./deploy/run-copy-relayer-testnet.sh
```

Dry-run is the default. It prints the selected source event, deterministic
`BytesN<32>` replay key, normalized integer amounts, and quote values without
submitting a transaction.

## Configure policy

For a fresh isolated testnet policy, use
`deploy/configure-copy-policy-testnet.sh` to perform the owner-controlled
initialization, recorder configuration, and numeric session registration as
separate actions. Its default action is `help`; every write requires an
explicit `COPY_POLICY_ACTION` and the script refuses non-testnet networks.

The local database uses an opaque session string, while the current Soroban
entry point uses a `u32` session ID. Therefore a write also requires a
`contract_session_id` matching the session registered on the testnet policy
contract. This value can be supplied when creating or updating a Copy session
through the API. For one-off legacy sessions, `COPY_CONTRACT_SESSION_ID` can
override the missing database value. The helper refuses to write when this
mapping is not provided; it never guesses or hashes a local session ID.

## Soroswap standard operation

Soroswap AMM uses a separate venue-aware smoke tool because its Router call
needs two token amounts for deposits and an LP-share amount for withdrawals.
The tool performs all checks without writing by default:

```sh
STELLAR_NETWORK=testnet \
SOROSWAP_COPY_POLICY=<testnet-policy-contract> \
SOURCE_ACCOUNT=<owner-or-recorder-signer> \
SOROSWAP_POOL=<allowlisted-pair> \
SOROSWAP_LEADER=<registered-leader-address> \
SOROSWAP_SESSION_ID=<registered-u32-session-id> \
SOROSWAP_AMOUNT0=<token0-stroops> \
SOROSWAP_AMOUNT1=<token1-stroops> \
SOROSWAP_QUOTE=<quote-unit> \
./deploy/smoke-soroswap-copy-testnet.sh
```

Before enabling writes, configure the Router with the owner-only helper and
provision both token contracts to the policy. The provisioning command needs
the token admin signer; it must not be replaced with the policy owner unless
the token contract explicitly uses that same admin:

```sh
COPY_POLICY_ACTION=set-router \
STELLAR_NETWORK=testnet \
STELLAR_TESTNET_SOURCE=<policy-owner-signer> \
COPY_POLICY=<testnet-policy-contract> \
COPY_VENUE=soroswap_amm \
COPY_ROUTER_ADDRESS=<testnet-router> \
./deploy/configure-copy-policy-testnet.sh
```

Only after the preflight confirms the Router, pair tokens, session limits, and
policy balances should `RUN_WRITE=1` be added to the Soroswap command. The
command submits no mainnet transaction and has no fallback to generic
`execute_copy_op`.

## Testnet write

Only after the isolated testnet policy and accounts are configured, add
`RUN_WRITE=1` to the same command. Because session-level slippage limits are
not yet persisted, the script also requires
`COPY_ALLOW_ZERO_MIN_OUTPUTS=1`; use this only with an isolated Testnet
fixture. The script refuses any network value other than `testnet`. Confirm
the selected pool, session, event kind, scaled amounts, and quote in the
dry-run output before enabling writes.

```sh
RUN_WRITE=1 COPY_ALLOW_ZERO_MIN_OUTPUTS=1 STELLAR_NETWORK=testnet \
COPY_CONTRACT_SESSION_ID=<registered-u32-session-id> \
COPY_POLICY=<testnet-policy-contract> \
COPY_RECORDER_ACCOUNT=<recorder-signer> \
COPY_RELAYER_ACCOUNT=<relayer-signer> \
COPY_INDEX_DB_PATH=./data/pool-indexer.db \
./deploy/run-copy-relayer-testnet.sh
```

Do not put signing secrets in the repository or in the database. The account
values above are Stellar CLI account aliases or configured signer identities;
the helper does not accept private keys as command-line arguments.

## Operational checks

Before a testnet run, verify:

- the policy contract is an isolated testnet deployment;
- the recorder and relayer accounts are funded and authorized by that policy;
- the session allowlist includes the selected pool and action;
- the selected source event has not already been recorded on-chain;
- the corresponding DEX fixture has passed deterministic compatibility tests.

After a run, compare the transaction result with the local `recorder_outbox`
and `copy_ops` rows. A failed submission must remain visible for retry and
must not be treated as executed merely because a transaction was constructed.

## Bind a local session

After the Soroban owner has registered a session, bind its numeric ID to the
matching local Copy session through the API. The leader, allowed pools,
coefficient, claim setting, expiry, and quote limits must already match the
on-chain policy; `contract_session_id` is only an identity binding and does not
replace those checks.

```sh
curl -X PATCH https://api.lumenlp.xyz/v1/copy/sessions/<local-session-id> \
  -H 'authorization: Bearer <wallet-auth-token>' \
  -H 'content-type: application/json' \
  -d '{"follower_address":"G...","contract_session_id":42}'
```

Use the returned session JSON to confirm the binding before running the
relayer. If the local and on-chain policies do not describe the same follower
workflow, stop and create a new isolated testnet session instead of reusing
the numeric ID. The API verifies both the wallet bearer token and the stored
session owner before accepting the update. This API authentication protects
the control plane; the Soroban policy remains the execution authority.

Once `contract_session_id` is bound, the API rejects changes to the local
coefficient, claim setting, or contract session identity. Pause, resume, and
stop remain available. Create and bind a new Copy session when policy terms
change so local preparation cannot silently drift from the on-chain policy.
