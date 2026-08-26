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
  active, the source event belongs to the session's configured Leader, the
  pool and operation are allowed, and both limits pass.
- Source-event identifiers are replay-protected and successful executions emit
  a `copy` event.

The current `execute_copy_op` records the policy-approved intent and consumes
the budget without invoking a DEX. `execute_aquarius_standard_op` now adds the
adapter boundary for standard Aquarius pools: it invokes only `deposit`,
`withdraw`, or `claim`, with the policy contract as the user argument. Claim
also requires an explicit reward-token address and derives the claim amount
from the pool before authorizing the transfer. It is still not enabled on
production and does not yet support concentrated-liquidity position calls.
For deposits, the requested token amounts must equal the coefficient-scaled
source event. For withdrawals, the requested share amount is likewise bound to
the coefficient-scaled source event. A relayer cannot replace the Leader,
change the copy size, or reuse an event by changing downstream call arguments.

The contract intentionally does not dispatch arbitrary calls to a DEX. A venue
must first have a reviewed adapter contract interface, deterministic fixtures,
capability reporting, and fail-closed behavior for unsupported operations.
Aquarius is the current contract reference path. Soroswap AMM and Phoenix XYK
have isolated, promotion-gated policy boundaries; Phoenix Stable, Sushi V3,
and Comet remain separate adapter-validation work and are not implicitly
enabled by the policy contract.

The `execute_standard_op` entry point is the venue-neutral boundary for future
adapters. It now contains a separately guarded Soroswap AMM branch, but that
branch requires an owner-configured Router address, validates the pair token
and amount boundary, and remains testnet/promotion gated. The separate
`phoenix_xyk` branch invokes only the explicit Phoenix XYK pool ABI; the
generic `phoenix` identifier, Phoenix Stable, Sushi V3, and Comet remain
rejected. No venue inherits Aquarius call semantics by passing a different
venue string, and unsupported operations remain rejected before authorization
consumes quota or writes a replay marker.

The non-Aquarius branches also construct explicit Soroban invoker
authorizations for the policy contract's token and LP-share transfers. The
venue call cannot pull follower assets unless those exact token contracts,
source, destination, and scaled amounts are present in the authorization tree;
a downstream failure therefore rolls back the policy budget, replay marker,
and transfer side effects atomically.

## Build

```sh
stellar contract build --package lumenlp-copy-policy --out-dir target/contracts
```

The current local build artifact is
`target/wasm32v1-none/release/lumenlp_copy_policy.wasm`. It was built on
2026-08-26 with hash:

`3ac2e5657e55b81b3539e2ab0f91ecad07c9f06799474bf3a0c2760052d4e204`

The deployed v3 testnet instance below is the previous promotion-gated build.
The Soroswap-gated build is deployed separately and remains isolated from
production users and mainnet funds.

To deploy a separate testnet instance without replacing this contract, use
`deploy/deploy-copy-policy-testnet.sh` with `STELLAR_TESTNET_SOURCE` and
`STELLAR_TESTNET_SIGNER` set to testnet-only credentials. The script refuses
non-testnet configuration and uses a deterministic, overridable deployment
salt.

## Testnet Deployment

The first testnet deployment completed on 2026-08-18:

- Network: Stellar Testnet
- Contract: `CCFF6EGDGRXQYXMYTW6KZHIGGVG6A7IPJTQUB2MHTCDXVSVHHMEXHHBI`
- Deployment transaction: [b47ac3822186fdf7c15593c6109b58cc01db16d8b89b9511166eba25b7efdb6f](https://stellar.expert/explorer/testnet/tx/b47ac3822186fdf7c15593c6109b58cc01db16d8b89b9511166eba25b7efdb6f)
- Lab view: [testnet contract](https://lab.stellar.org/r/testnet/contract/CCFF6EGDGRXQYXMYTW6KZHIGGVG6A7IPJTQUB2MHTCDXVSVHHMEXHHBI)
- Initialization transaction: [35de3ae514ee8f31736f2617dbe744f3a6265745d3edfb5ada61f28193c0f4ee](https://stellar.expert/explorer/testnet/tx/35de3ae514ee8f31736f2617dbe744f3a6265745d3edfb5ada61f28193c0f4ee)

The current ABI was deployed as a new isolated testnet instance on
2026-08-26. It is the schema-verified target for the next testnet vertical
slice and is not connected to production relayers or mainnet funds:

- Contract: `CC2M72PXE2W66T54NIL6FHIDORLIEETVEKP27MMGGGQ52OFAZS62B534`
- WASM upload transaction: [670e5de07588845cbd2b0cbacf6f360298621937ecf9d065807eaac43be6dc12](https://stellar.expert/explorer/testnet/tx/670e5de07588845cbd2b0cbacf6f360298621937ecf9d065807eaac43be6dc12)
- Deployment transaction: [7957bdfa72b81f0a522f1720eddf412e27ac46e35626473c81789ed4d4400926](https://stellar.expert/explorer/testnet/tx/7957bdfa72b81f0a522f1720eddf412e27ac46e35626473c81789ed4d4400926)
- Lab view: [testnet contract](https://lab.stellar.org/r/testnet/contract/CC2M72PXE2W66T54NIL6FHIDORLIEETVEKP27MMGGGQ52OFAZS62B534)
- Verified entry points: `set_event_recorder`, `record_leader_event`, `register_session_coeff`, `execute_copy_op`, and `execute_standard_op`.

The isolated testnet configuration was then applied and verified:

- Initialization transaction: [edf6cc32c4b9154b2435df3ddad6d72e65905bc4429be4814cd01a5c7494a869](https://stellar.expert/explorer/testnet/tx/edf6cc32c4b9154b2435df3ddad6d72e65905bc4429be4814cd01a5c7494a869)
- Event recorder configuration transaction: [fbd1f720cae46bfa8aa746dedc7fb1d2a06010c598ef99ad8c713b6ce42eaa92](https://stellar.expert/explorer/testnet/tx/fbd1f720cae46bfa8aa746dedc7fb1d2a06010c598ef99ad8c713b6ce42eaa92)
- Session `43` registration transaction: [4d1fd22bedcb514329e9531046d2e359f9451847e67df2163e3a3096a1853e06](https://stellar.expert/explorer/testnet/tx/4d1fd22bedcb514329e9531046d2e359f9451847e67df2163e3a3096a1853e06)
- Read-only session verification confirmed the configured Leader, pool allowlist, 10% coefficient, quote limits, and expiry.

The ABI check and session read were read-only. A synthetic recorder event and
policy-only Copy operation were subsequently verified on testnet; no DEX call
or token balance change was involved:

- Recorder event transaction: [2aabd8c48bfb838f389a02204419a32b26e4798201e55fe87b38e4358bd28d1f](https://stellar.expert/explorer/testnet/tx/2aabd8c48bfb838f389a02204419a32b26e4798201e55fe87b38e4358bd28d1f)
- `CopyExecuted` transaction: [551f237d9ddf28c9ddc4101505c3f8c9fbfb098be6923ff91a99fb6666710257](https://stellar.expert/explorer/testnet/tx/551f237d9ddf28c9ddc4101505c3f8c9fbfb098be6923ff91a99fb6666710257)

The isolated Soroswap routing instance was deployed and configured on
2026-08-24:

- Contract: `CBHMOPHLGLWVDW7EB4OGVP4FCWE6NDGJIPIN4SIUJ5BJB4R4PKLFB4TU`
- WASM upload transaction: [d2f05b344bae967caaf11beb11ab057e0f768258830cca130b0c65d675250a0b](https://stellar.expert/explorer/testnet/tx/d2f05b344bae967caaf11beb11ab057e0f768258830cca130b0c65d675250a0b)
- Deployment transaction: [09847de77a83aa8f1d84854a2e8a4e4b361e5bda9118891f7ed2f209299053f9](https://stellar.expert/explorer/testnet/tx/09847de77a83aa8f1d84854a2e8a4e4b361e5bda9118891f7ed2f209299053f9)
- Initialization transaction: [9436448a1dbf4a6a0db36b38bf50f0b3a4b88806697558bce4e03ac9755fbe36](https://stellar.expert/explorer/testnet/tx/9436448a1dbf4a6a0db36b38bf50f0b3a4b88806697558bce4e03ac9755fbe36)
- Soroswap Router allowlist transaction: [8e141616400408c82da9a5f1885f3a80a0111b0976930a82dd1d0c65a3e3f951](https://stellar.expert/explorer/testnet/tx/8e141616400408c82da9a5f1885f3a80a0111b0976930a82dd1d0c65a3e3f951)
- Soroswap Testnet Factory: `CDP3HMUH6SMS3S7NPGNDJLULCOXXEPSHY4JKUKMBNQMATHDHWXRRJTBY`
- Soroswap Testnet Router: `CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD`
- Read-only validation script: `deploy/validate-soroswap-testnet.sh`
- Asset preflight/provisioning script: `deploy/prepare-soroswap-copy-testnet.sh`

The testnet Router/Factory relationship and a real pair state read were
validated without signing a transaction. The policy instance has not been
connected to production relayers and no Copy LP deposit or withdrawal was
executed through it.

The read-only Soroswap Testnet preflight was repeated on 2026-08-26 after the
policy and adapter regression tests. It confirmed that the configured Router
`CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD` resolves to the
configured Factory
`CDP3HMUH6SMS3S7NPGNDJLULCOXXEPSHY4JKUKMBNQMATHDHWXRRJTBY`. The Factory
returned 225 pairs; a real pair read returned both token addresses, ordered
reserves `1232150007934 / 2069771239621`, and a normalized fee of 30 bps.
This confirms the read and configuration boundary only. It does not claim that
Soroswap Copy LP execution is production-enabled.

The provisioning helper is intentionally separate from execution. Its default
mode only checks the Router/Factory relationship and the policy's configured
Router. Its explicit write mode requires a caller-supplied testnet token-admin
signer and mints assets only to the isolated policy contract; the signer is
never stored in the repository and the helper does not invoke a DEX operation.

The authorization-tree build was deployed as a new, isolated testnet
instance on 2026-08-24. It is configured only for authorization-boundary
testing:

- Contract: `CBWRBKWB5FMVC4QX4CD3MFYE3APJKL2EZTFHF5RI5XROZ4ZOFHV3V2KV`
- Deployment transaction: [a4b91fa9d3a62529680dab0a0a4061272161add26b28449a3d235985999e9509](https://stellar.expert/explorer/testnet/tx/a4b91fa9d3a62529680dab0a0a4061272161add26b28449a3d235985999e9509)
- Lab view: [testnet contract](https://lab.stellar.org/r/testnet/contract/CBWRBKWB5FMVC4QX4CD3MFYE3APJKL2EZTFHF5RI5XROZ4ZOFHV3V2KV)
- WASM hash: `4c48263115c925f9806915bf2b9f26d5980a14c8a1ec66ff4e4c43ad9aee4d19`
- Initialization transaction: [168ca8a74e43b8e82cde8582f106735b7252d1a7f740ad5116500b709d3b33a5](https://stellar.expert/explorer/testnet/tx/168ca8a74e43b8e82cde8582f106735b7252d1a7f740ad5116500b709d3b33a5)
- Soroswap Router allowlist transaction: [f446f44fcd0a51f104ff49528ac8a9976e1e8dbf5cd24b7f905ab9dd71657b22](https://stellar.expert/explorer/testnet/tx/f446f44fcd0a51f104ff49528ac8a9976e1e8dbf5cd24b7f905ab9dd71657b22)
- Configured Router: `CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD`

The instance is reserved for authorization-tree and downstream-failure
smoke tests. It has no funded assets, is not connected to production users or
mainnet funds, and its isolated session below does not call a DEX.

### No-funds policy smoke test

On 2026-08-24, the isolated instance was used to validate the complete
authorization path without provisioning tokens or executing a liquidity
operation:

- Event recorder configured: [fd3b42d684d696fcf61ffb4d52a87e25c41cad0adfd71721926d6bfc5fa17ed5](https://stellar.expert/explorer/testnet/tx/fd3b42d684d696fcf61ffb4d52a87e25c41cad0adfd71721926d6bfc5fa17ed5)
- Session `42` registered with one allowlisted Soroswap pair, quote limit `10`, and expiry `2000000000`: [cf757f184bffd381c4039bf477a0bdd41d4aba10c23611ad54d4cbb794fcde0b](https://stellar.expert/explorer/testnet/tx/cf757f184bffd381c4039bf477a0bdd41d4aba10c23611ad54d4cbb794fcde0b)
- Leader deposit event recorded with source id `0x42`: [c58c7a6babd69b7605ab7c427befb168174e7c489446107aaed9a64960f88194](https://stellar.expert/explorer/testnet/tx/c58c7a6babd69b7605ab7c427befb168174e7c489446107aaed9a64960f88194)
- Relayer consumed the event and emitted the policy `copy` event: [28588bb4bbe0367f01d50ec63a05b1cbbc963f57b732c2cf6a96c0df7f557482](https://stellar.expert/explorer/testnet/tx/28588bb4bbe0367f01d50ec63a05b1cbbc963f57b732c2cf6a96c0df7f557482)

Re-submitting the same source id was rejected during simulation with the
contract replay error. This confirms that the allowlist, coefficient-bound
quote, relayer authorization, and replay marker are enforced before any
future DEX adapter call. No token balance changed in this test.

The local policy build also contains explicit `phoenix_xyk` and
`phoenix_stable` execution boundaries. They bind deposits and withdrawals to
the same recorded event, coefficient, pool allowlist, replay, and quota checks,
then invoke only the matching Phoenix pool ABI with the policy contract as
caller. These branches remain promotion-gated and are not connected to
production relayers. The generic `phoenix` identifier remains rejected so the
caller must declare which ABI was validated.

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
- Native SAC provisioning transaction: [f23241e2d4f814efd4ae6e16b97f959bd1d8bc42dcf6dc5db8e569850f66ea83](https://stellar.expert/explorer/testnet/tx/f23241e2d4f814efd4ae6e16b97f959bd1d8bc42dcf6dc5db8e569850f66ea83)
- Testnet USDC provisioning transaction: [cf59900ea9470ea8a91f1918b62ec75dfdde2593a9a59575534c3aa4d99e9524](https://stellar.expert/explorer/testnet/tx/cf59900ea9470ea8a91f1918b62ec75dfdde2593a9a59575534c3aa4d99e9524)
- Real Aquarius deposit: [ac5f3ff656cc8e0ff1113c1f0d05310806ebd5f75066913bb13e9506a26045ee](https://stellar.expert/explorer/testnet/tx/ac5f3ff656cc8e0ff1113c1f0d05310806ebd5f75066913bb13e9506a26045ee)
- Real Aquarius withdraw: [48e6f8254814b7895ec0749269d0f4ba20c065292fa95f796e36b1885b74b541](https://stellar.expert/explorer/testnet/tx/48e6f8254814b7895ec0749269d0f4ba20c065292fa95f796e36b1885b74b541)

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
the real Aquarius `claim` path and completed with a zero reward amount. The
local deterministic fixture also covers a positive reward and verifies the
exact pool-to-policy token transfer. A funded deposit and withdraw were then
executed against the same real pool;
the deposit minted 5,767,331 LP shares and the withdraw burned 1,000,000
shares while returning both pool assets. A funded positive-reward claim against
the real Aquarius reward stream remains a promotion-gate item.

## Promotion Gate

Do not deploy this contract as a production policy until the following are
complete:

1. Aquarius deposit, withdrawal, and claim calls with deterministic contract
   fixtures, including rollback when the downstream call fails. The local
   build covers all three authorization paths; funded real-Aquarius claim
   verification and downstream rollback coverage remain.
2. End-to-end tests for insufficient balance, expired sessions, replay,
   pause/disarm, daily limits, and relayer downtime.
3. Public transaction hashes and a documented recovery/manual-signing path.
