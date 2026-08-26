# Copy Relayer Runbook

This runbook covers the first Copy LP execution path. It is intentionally
testnet-only until the Soroban policy instance, recorder authority, relayer
authority, and DEX operation fixtures have been validated together.

## Boundary

The API and indexer create durable `recorder_outbox` and `copy_ops` rows. They
do not hold signing keys. The relayer reads one pending operation, records its
canonical source event on the Copy Policy contract, and submits the
policy-gated Copy operation. Soroban remains the final authority for replay,
session, pool, action, expiry, and quote-limit checks.

The current helper only invokes `execute_copy_op`, which validates and records
the policy-approved intent. It does not call a DEX pool. Venue-specific
execution remains behind the adapter and policy entry points until the
corresponding testnet fixtures are enabled.

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

The local database uses an opaque session string, while the current Soroban
entry point uses a `u32` session ID. Therefore a write also requires a
`contract_session_id` matching the session registered on the testnet policy
contract. This value can be supplied when creating or updating a Copy session
through the API. For one-off legacy sessions, `COPY_CONTRACT_SESSION_ID` can
override the missing database value. The helper refuses to write when this
mapping is not provided; it never guesses or hashes a local session ID.

## Testnet write

Only after the isolated testnet policy and accounts are configured, add
`RUN_WRITE=1` to the same command. The script refuses any network value other
than `testnet`. Confirm the selected pool, session, event kind, and quote in
the dry-run output before enabling writes.

```sh
RUN_WRITE=1 STELLAR_NETWORK=testnet \
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
  -H 'content-type: application/json' \
  -d '{"contract_session_id":42}'
```

Use the returned session JSON to confirm the binding before running the
relayer. If the local and on-chain policies do not describe the same follower
workflow, stop and create a new isolated testnet session instead of reusing
the numeric ID.
