# Copy Policy Contract

`contracts/copy-policy` is the first Soroban testnet vertical-slice contract for
LumenLP Copy LP. It is a policy gate, not a custody vault. Its adapter boundary
contains guarded calls for Aquarius standard-pool operations; production
promotion remains blocked until all downstream authorization paths are tested.

## Current Scope

- Owner initializes the policy and names the LumenLP relayer.
- Owner registers a session with a Leader, allowed pool list, claim flag,
  per-operation quote limit, UTC daily quote limit, and expiry.
- Owner can pause, resume, or disarm a session.
- The relayer can submit a source-event identifier only when the session is
  active, the pool and operation are allowed, and both limits pass.
- Source-event identifiers are replay-protected and successful executions emit
  a `copy` event.

The current `execute_copy_op` records the policy-approved intent and consumes
the budget without invoking a DEX. `execute_aquarius_standard_op` now adds the
adapter boundary for standard Aquarius pools: it invokes only `deposit`,
`withdraw`, or `claim`, with the policy contract as the user argument. Claim
also requires an explicit reward-token address and derives the claim amount
from the pool before authorizing the transfer. It is still not enabled on
production and does not yet support concentrated-liquidity position calls.

## Build

```sh
stellar contract build --package lumenlp-copy-policy --out-dir target/contracts
```

The current local build artifact is
`target/wasm32v1-none/release/lumenlp_copy_policy.wasm`. It was built on
2026-08-18 with hash:

`b4e0bde92aa7549c630b7f16a049e40bfeb9ac8cc96ca8e817d890a318be3d58`

This build is deployed as the v3 testnet instance below.

## Testnet Deployment

The first testnet deployment completed on 2026-08-18:

- Network: Stellar Testnet
- Contract: `CCFF6EGDGRXQYXMYTW6KZHIGGVG6A7IPJTQUB2MHTCDXVSVHHMEXHHBI`
- Deployment transaction: [b47ac3822186fdf7c15593c6109b58cc01db16d8b89b9511166eba25b7efdb6f](https://stellar.expert/explorer/testnet/tx/b47ac3822186fdf7c15593c6109b58cc01db16d8b89b9511166eba25b7efdb6f)
- Lab view: [testnet contract](https://lab.stellar.org/r/testnet/contract/CCFF6EGDGRXQYXMYTW6KZHIGGVG6A7IPJTQUB2MHTCDXVSVHHMEXHHBI)
- Initialization transaction: [35de3ae514ee8f31736f2617dbe744f3a6265745d3edfb5ada61f28193c0f4ee](https://stellar.expert/explorer/testnet/tx/35de3ae514ee8f31736f2617dbe744f3a6265745d3edfb5ada61f28193c0f4ee)

The v2 authorization build was deployed separately so the original testnet
instance remains available for comparison:

- Contract: `CCKPRYNSJJT75IE73YZ4V7RDALHD43BCFWPHL6QFVRYY464444ORTXK7`
- WASM hash: `48c95a984e72a2caa0126f78f8c6766a67ae3ac117f20be33caad058e732d387`
- WASM upload transaction: [30ed3caba87550c01231ae3d5ffef57672561fc8abd0d71cdc1d65d569cfb616](https://stellar.expert/explorer/testnet/tx/30ed3caba87550c01231ae3d5ffef57672561fc8abd0d71cdc1d65d569cfb616)
- Deployment transaction: [ea024811b0f66c4ed3097ddc68ccdd0d2a56baac9286a8fcf4a2b82c50e34cfb](https://stellar.expert/explorer/testnet/tx/ea024811b0f66c4ed3097ddc68ccdd0d2a56baac9286a8fcf4a2b82c50e34cfb)
- Initialization transaction: [d91106a484bd09c5a4bf3d0ce54bd5a422f060d55b530906d1826956d4a6001c](https://stellar.expert/explorer/testnet/tx/d91106a484bd09c5a4bf3d0ce54bd5a422f060d55b530906d1826956d4a6001c)
- Session registration transaction: [e245de55fd219b0e7e3c539c49bf7d4d09f17ce842b6716c564677aa9bedc095](https://stellar.expert/explorer/testnet/tx/e245de55fd219b0e7e3c539c49bf7d4d09f17ce842b6716c564677aa9bedc095)
- Native asset provisioning transaction: [df9b6fa2a5e160b61b420d2160804d98220e1c92714f356a47306c8e9f4f1e69](https://stellar.expert/explorer/testnet/tx/df9b6fa2a5e160b61b420d2160804d98220e1c92714f356a47306c8e9f4f1e69)

The v3 build adds the explicit `claim_token` argument and overflow-safe
withdraw calculation:

- Contract: `CDDEM34TOAN5DOG5LBJCC676QV2M27V3SSXZ7IPVA76RUSLSZEM5KLNJ`
- WASM hash: `b4e0bde92aa7549c630b7f16a049e40bfeb9ac8cc96ca8e817d890a318be3d58`
- WASM upload transaction: [a3b213732a4b3d76aa9f583e0eaf97dc9d0d52e50eb2a1d05c8bff98684b1cdb](https://stellar.expert/explorer/testnet/tx/a3b213732a4b3d76aa9f583e0eaf97dc9d0d52e50eb2a1d05c8bff98684b1cdb)
- Deployment transaction: [99770fbbdf7655c66a1bf63b292cf46472d1ea4a56bc788bb07697b72212d33c](https://stellar.expert/explorer/testnet/tx/99770fbbdf7655c66a1bf63b292cf46472d1ea4a56bc788bb07697b72212d33c)
- Initialization transaction: [3cdfe48789e7528f5236475314604469fe56d21ef2f817b598820e9363f26dad](https://stellar.expert/explorer/testnet/tx/3cdfe48789e7528f5236475314604469fe56d21ef2f817b598820e9363f26dad)
- Session registration transaction: [e730341ca5b7fe93ed777ee06796bfba5539a11ba87daef6e3c45142eaa84c75](https://stellar.expert/explorer/testnet/tx/e730341ca5b7fe93ed777ee06796bfba5539a11ba87daef6e3c45142eaa84c75)
- Zero-reward claim smoke test: [2352b8a75b5961d0c4d00f16e04e4da2e47ece3f8fe7f4c99f83434c131777ac](https://stellar.expert/explorer/testnet/tx/2352b8a75b5961d0c4d00f16e04e4da2e47ece3f8fe7f4c99f83434c131777ac)

This deployment is for contract and integration testing only. It is not
connected to production users, production relayers, or mainnet funds.

The first registered Aquarius Testnet target and policy smoke test are also
recorded on-chain:

- Aquarius Testnet router: `CBCFTQSPDBAIZ6R6PJQKSQWKNKWH2QIV3I4J72SHWBIK3ADRRAM5A6GD`
- Aquarius Testnet API: `https://amm-api-testnet.aqua.network/api/external/v1`
- Registered standard pool: `CAYBMZYJCOMMOHOGOGBK7ANIKF3JPZAL4D7SWAPVPHC4WBMOUT5DJN5B` (USDC/native)
- Session registration: [01b74dd2429a3b94c784ade9e6bfd78268316d58e937103fc8b8a25992b2c436](https://stellar.expert/explorer/testnet/tx/01b74dd2429a3b94c784ade9e6bfd78268316d58e937103fc8b8a25992b2c436)
- Policy intent smoke test: [47f4fa37e7163a9cd21a4b14d889f702e26f826468a103d97410fc0b4e16c139](https://stellar.expert/explorer/testnet/tx/47f4fa37e7163a9cd21a4b14d889f702e26f826468a103d97410fc0b4e16c139)

The smoke test exercised the relayer authorization, pool allowlist, operation
allowlist, quote limit, replay key, and emitted `copy` event. It did not move
tokens. A real deposit/withdraw/claim requires provisioning the policy
contract with the relevant Testnet assets and completing the Aquarius-specific
authorization and balance setup. The v2 deposit simulation reached the real
Aquarius `deposit` call and passed the nested SAC authorization; it currently
stops at the expected zero-USDC balance check. The v3 claim smoke test reached
the real Aquarius `claim` path and completed with a zero reward amount. A
positive-reward claim and a funded withdraw remain required before promotion.

## Promotion Gate

Do not deploy this contract as a production policy until the following are
complete:

1. Aquarius deposit, withdrawal, and claim calls with deterministic contract
   fixtures, including rollback when the downstream call fails. The current
   local build covers deposit, withdraw, and claim authorization setup; all
   three paths still require real Aquarius execution verification.
2. End-to-end tests for insufficient balance, expired sessions, replay,
   pause/disarm, daily limits, and relayer downtime.
3. Public transaction hashes and a documented recovery/manual-signing path.
