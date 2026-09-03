#![no_std]

use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contracterror, contractevent, contractimpl, contracttype, symbol_short, Address, BytesN, Env, IntoVal,
    Symbol, Vec, U256,
};

const MAX_POOLS: u32 = 32;
const COEFFICIENT_SCALE: u128 = 1_000_000;
const MAX_COEFFICIENT_PPM: u32 = 10_000_000;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    NotOwner = 3,
    NotRelayer = 4,
    SessionNotFound = 5,
    SessionPaused = 6,
    SessionExpired = 7,
    PoolNotAllowed = 8,
    OperationNotAllowed = 9,
    PerOperationLimit = 10,
    DailyLimit = 11,
    Replay = 12,
    InvalidLimit = 13,
    TooManyPools = 14,
    EventRecorderNotConfigured = 15,
    EventNotFound = 16,
    EventMismatch = 17,
    InvalidEvent = 18,
    RouterNotConfigured = 19,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Session {
    pub leader: Address,
    pub allowed_pools: Vec<Address>,
    pub coefficient_ppm: u32,
    pub follow_claims: bool,
    pub max_per_op_quote: i128,
    pub max_daily_quote: i128,
    pub expires_at: u64,
    pub paused: bool,
    pub daily_day: u64,
    pub daily_used_quote: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaderEvent {
    pub leader: Address,
    pub pool: Address,
    pub kind: Symbol,
    pub claim_token: Option<Address>,
    pub amounts: Vec<u128>,
    pub quote: i128,
    pub ledger: u32,
    pub recorded_at: u64,
}

// Keep the legacy event topics and Vec data layout stable for the indexer while
// using Soroban's type-safe event definition instead of Events::publish.
#[contractevent(data_format = "vec", topics = ["copy"])]
pub struct CopyExecuted {
    #[topic]
    pub session_id: u32,
    pub source_event_id: BytesN<32>,
    pub pool: Address,
    pub kind: Symbol,
    pub quote: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhoenixAsset {
    pub address: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhoenixPoolInfo {
    pub asset_a: PhoenixAsset,
    pub asset_b: PhoenixAsset,
    pub asset_lp_share: PhoenixAsset,
}

fn add_transfer_auth(
    env: &Env,
    entries: &mut Vec<InvokerContractAuthEntry>,
    token: Address,
    from: Address,
    to: Address,
    amount: i128,
) {
    if amount <= 0 {
        return;
    }
    entries.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
        context: ContractContext {
            contract: token,
            fn_name: symbol_short!("transfer"),
            args: Vec::from_array(env, [from.into_val(env), to.into_val(env), amount.into_val(env)]),
        },
        sub_invocations: Vec::new(env),
    }));
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Owner,
    Relayer,
    EventRecorder,
    Session(u32),
    Replay(u32, BytesN<32>),
    LeaderEvent(BytesN<32>),
    VenueRouter(Symbol),
}

#[contract]
pub struct CopyPolicy;

#[contractimpl]
impl CopyPolicy {
    /// Initialize the policy owner and the service account allowed to submit
    /// already-authorized copy intents.
    pub fn initialize(env: Env, owner: Address, relayer: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Owner) {
            return Err(Error::AlreadyInitialized);
        }
        owner.require_auth();
        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage().instance().set(&DataKey::Relayer, &relayer);
        Ok(())
    }

    /// Configure the trusted ingest role used to notarize indexed Leader
    /// events. Execution remains permissionless, but only this role may add
    /// the source event that execution is allowed to consume.
    pub fn set_event_recorder(env: Env, recorder: Address) -> Result<(), Error> {
        Self::owner(&env)?.require_auth();
        env.storage().instance().set(&DataKey::EventRecorder, &recorder);
        Ok(())
    }

    /// Configure the only Router contract that the policy may call for a
    /// supported venue. Router addresses are never accepted from the relayer.
    pub fn set_venue_router(env: Env, venue: Symbol, router: Address) -> Result<(), Error> {
        Self::owner(&env)?.require_auth();
        if venue != Symbol::new(&env, "soroswap") && venue != Symbol::new(&env, "soroswap_amm") {
            return Err(Error::OperationNotAllowed);
        }
        env.storage().instance().set(&DataKey::VenueRouter(venue), &router);
        Ok(())
    }

    pub fn venue_router(env: Env, venue: Symbol) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::VenueRouter(venue))
            .ok_or(Error::RouterNotConfigured)
    }

    /// Store one canonical Leader liquidity event. Re-submitting the same
    /// source id is idempotent and never replaces the original payload.
    pub fn record_leader_event(
        env: Env,
        source_event_id: BytesN<32>,
        leader: Address,
        pool: Address,
        kind: Symbol,
        amounts: Vec<u128>,
        quote: i128,
        ledger: u32,
    ) -> Result<(), Error> {
        let recorder: Address = env
            .storage()
            .instance()
            .get(&DataKey::EventRecorder)
            .ok_or(Error::EventRecorderNotConfigured)?;
        recorder.require_auth();
        if amounts.is_empty() || quote <= 0 {
            return Err(Error::InvalidEvent);
        }
        if kind != symbol_short!("deposit") && kind != symbol_short!("withdraw") && kind != symbol_short!("claim") {
            return Err(Error::InvalidEvent);
        }
        let key = DataKey::LeaderEvent(source_event_id);
        if env.storage().persistent().has(&key) {
            return Ok(());
        }
        let event = LeaderEvent {
            leader,
            pool,
            kind,
            claim_token: None,
            amounts,
            quote,
            ledger,
            recorded_at: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&key, &event);
        Ok(())
    }

    /// Record a claim event together with the exact reward token authorized by
    /// the source event. Claims are kept separate so legacy event producers
    /// remain ABI-compatible while new claim records fail closed if omitted.
    pub fn record_claim_event(
        env: Env,
        source_event_id: BytesN<32>,
        leader: Address,
        pool: Address,
        amounts: Vec<u128>,
        quote: i128,
        ledger: u32,
        claim_token: Address,
    ) -> Result<(), Error> {
        let recorder: Address = env
            .storage()
            .instance()
            .get(&DataKey::EventRecorder)
            .ok_or(Error::EventRecorderNotConfigured)?;
        recorder.require_auth();
        if amounts.is_empty() || quote <= 0 {
            return Err(Error::InvalidEvent);
        }
        let key = DataKey::LeaderEvent(source_event_id);
        if env.storage().persistent().has(&key) {
            return Ok(());
        }
        env.storage().persistent().set(
            &key,
            &LeaderEvent {
                leader,
                pool,
                kind: symbol_short!("claim"),
                claim_token: Some(claim_token),
                amounts,
                quote,
                ledger,
                recorded_at: env.ledger().timestamp(),
            },
        );
        Ok(())
    }

    pub fn leader_event(env: Env, source_event_id: BytesN<32>) -> Result<LeaderEvent, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::LeaderEvent(source_event_id))
            .ok_or(Error::EventNotFound)
    }

    pub fn register_session(
        env: Env,
        session_id: u32,
        leader: Address,
        allowed_pools: Vec<Address>,
        follow_claims: bool,
        max_per_op_quote: i128,
        max_daily_quote: i128,
        expires_at: u64,
    ) -> Result<(), Error> {
        Self::register_session_inner(
            &env,
            session_id,
            leader,
            allowed_pools,
            COEFFICIENT_SCALE as u32,
            follow_claims,
            max_per_op_quote,
            max_daily_quote,
            expires_at,
        )
    }

    /// Register a session with a fixed-point copy coefficient. `1_000_000`
    /// represents 1.0, `100_000` represents 10%, and `2_000_000` represents
    /// 2.0. The coefficient is stored on-chain so a relayer cannot change the
    /// scale while submitting an operation.
    pub fn register_session_coeff(
        env: Env,
        session_id: u32,
        leader: Address,
        allowed_pools: Vec<Address>,
        coefficient_ppm: u32,
        follow_claims: bool,
        max_per_op_quote: i128,
        max_daily_quote: i128,
        expires_at: u64,
    ) -> Result<(), Error> {
        Self::register_session_inner(
            &env,
            session_id,
            leader,
            allowed_pools,
            coefficient_ppm,
            follow_claims,
            max_per_op_quote,
            max_daily_quote,
            expires_at,
        )
    }

    fn register_session_inner(
        env: &Env,
        session_id: u32,
        leader: Address,
        allowed_pools: Vec<Address>,
        coefficient_ppm: u32,
        follow_claims: bool,
        max_per_op_quote: i128,
        max_daily_quote: i128,
        expires_at: u64,
    ) -> Result<(), Error> {
        Self::owner(&env)?.require_auth();
        if allowed_pools.len() > MAX_POOLS {
            return Err(Error::TooManyPools);
        }
        if coefficient_ppm == 0 ||
            coefficient_ppm > MAX_COEFFICIENT_PPM ||
            max_per_op_quote <= 0 ||
            max_daily_quote <= 0 ||
            expires_at <= env.ledger().timestamp()
        {
            return Err(Error::InvalidLimit);
        }
        let session = Session {
            leader,
            allowed_pools,
            coefficient_ppm,
            follow_claims,
            max_per_op_quote,
            max_daily_quote,
            expires_at,
            paused: false,
            daily_day: day(env.ledger().timestamp()),
            daily_used_quote: 0,
        };
        env.storage().persistent().set(&DataKey::Session(session_id), &session);
        Ok(())
    }

    pub fn pause_session(env: Env, session_id: u32) -> Result<(), Error> {
        Self::owner(&env)?.require_auth();
        let mut session = Self::load_session(&env, session_id)?;
        session.paused = true;
        env.storage().persistent().set(&DataKey::Session(session_id), &session);
        Ok(())
    }

    pub fn resume_session(env: Env, session_id: u32) -> Result<(), Error> {
        Self::owner(&env)?.require_auth();
        let mut session = Self::load_session(&env, session_id)?;
        if env.ledger().timestamp() >= session.expires_at {
            return Err(Error::SessionExpired);
        }
        session.paused = false;
        env.storage().persistent().set(&DataKey::Session(session_id), &session);
        Ok(())
    }

    pub fn disarm_session(env: Env, session_id: u32) -> Result<(), Error> {
        Self::owner(&env)?.require_auth();
        env.storage().persistent().remove(&DataKey::Session(session_id));
        Ok(())
    }

    /// Policy-only execution gate. It records a source event and consumes the
    /// daily budget, but deliberately does not call a DEX in this prototype.
    pub fn execute_copy_op(
        env: Env,
        session_id: u32,
        source_event_id: BytesN<32>,
        pool: Address,
        kind: Symbol,
        quote: i128,
    ) -> Result<(), Error> {
        let event = Self::load_leader_event(&env, &source_event_id)?;
        if event.pool != pool || event.kind != kind {
            return Err(Error::EventMismatch);
        }
        let session = Self::load_session(&env, session_id)?;
        if event.leader != session.leader {
            return Err(Error::EventMismatch);
        }
        if scale_i128(event.quote, session.coefficient_ppm)? != quote {
            return Err(Error::EventMismatch);
        }
        Self::authorize_copy_op(&env, session_id, &source_event_id, &pool, &kind, quote)?;
        CopyExecuted {
            session_id,
            source_event_id,
            pool,
            kind,
            quote,
        }
        .publish(&env);
        Ok(())
    }

    /// Generic venue boundary. The dispatch stays explicit so a venue cannot
    /// smuggle an arbitrary Router address or operation through the relayer.
    /// Unsupported venues are rejected before authorization consumes quota or
    /// creates a replay marker.
    pub fn execute_standard_op(
        env: Env,
        venue: Symbol,
        session_id: u32,
        source_event_id: BytesN<32>,
        pool: Address,
        kind: Symbol,
        quote: i128,
        desired_amounts: Vec<u128>,
        min_shares: u128,
        share_amount: u128,
        min_amounts: Vec<u128>,
        claim_token: Address,
    ) -> Result<(), Error> {
        if venue == Symbol::new(&env, "phoenix_xyk") {
            return Self::execute_phoenix_xyk_standard_op(
                env,
                session_id,
                source_event_id,
                pool,
                kind,
                quote,
                desired_amounts,
                share_amount,
                min_amounts,
            );
        }
        if venue == Symbol::new(&env, "phoenix_stable") {
            return Self::execute_phoenix_stable_op(
                env,
                session_id,
                source_event_id,
                pool,
                kind,
                quote,
                desired_amounts,
                min_shares,
                share_amount,
                min_amounts,
            );
        }
        if venue == Symbol::new(&env, "soroswap") || venue == Symbol::new(&env, "soroswap_amm") {
            return Self::execute_soroswap_standard_op(
                env,
                venue,
                session_id,
                source_event_id,
                pool,
                kind,
                quote,
                desired_amounts,
                min_shares,
                share_amount,
                min_amounts,
            );
        }
        if venue != symbol_short!("aquarius") {
            return Err(Error::OperationNotAllowed);
        }
        Self::execute_aquarius_standard_op(
            env,
            session_id,
            source_event_id,
            pool,
            kind,
            quote,
            desired_amounts,
            min_shares,
            share_amount,
            min_amounts,
            claim_token,
        )
    }

    /// Execute a Soroswap AMM operation through an owner-configured Router.
    /// Soroswap pairs use the pair itself as the LP token; the Router keeps
    /// token transfers and minimum-output checks in the venue boundary.
    pub fn execute_soroswap_standard_op(
        env: Env,
        venue: Symbol,
        session_id: u32,
        source_event_id: BytesN<32>,
        pool: Address,
        kind: Symbol,
        quote: i128,
        desired_amounts: Vec<u128>,
        min_shares: u128,
        share_amount: u128,
        min_amounts: Vec<u128>,
    ) -> Result<(), Error> {
        if venue != Symbol::new(&env, "soroswap") && venue != Symbol::new(&env, "soroswap_amm") {
            return Err(Error::OperationNotAllowed);
        }
        if kind != symbol_short!("deposit") && kind != symbol_short!("withdraw") {
            return Err(Error::OperationNotAllowed);
        }
        if desired_amounts.len() != 2 || min_amounts.len() != 2 {
            return Err(Error::InvalidEvent);
        }
        let event = Self::load_leader_event(&env, &source_event_id)?;
        let session = Self::load_session(&env, session_id)?;
        if event.pool != pool || event.kind != kind || event.leader != session.leader {
            return Err(Error::EventMismatch);
        }
        if scale_i128(event.quote, session.coefficient_ppm)? != quote {
            return Err(Error::EventMismatch);
        }
        if kind == symbol_short!("deposit") &&
            scale_amounts(&env, &event.amounts, session.coefficient_ppm) != desired_amounts
        {
            return Err(Error::EventMismatch);
        }
        if kind == symbol_short!("withdraw") {
            let expected_shares = event
                .amounts
                .get(0)
                .map(|amount| scale_amount(&env, amount, session.coefficient_ppm))
                .unwrap_or(0);
            if expected_shares != share_amount {
                return Err(Error::EventMismatch);
            }
        }
        let router = Self::venue_router(env.clone(), venue)?;
        Self::authorize_copy_op(&env, session_id, &source_event_id, &pool, &kind, quote)?;

        let token_a: Address = env.invoke_contract(&pool, &Symbol::new(&env, "token_0"), Vec::new(&env));
        let token_b: Address = env.invoke_contract(&pool, &Symbol::new(&env, "token_1"), Vec::new(&env));
        let user = env.current_contract_address();
        let deadline = env.ledger().timestamp().saturating_add(300);
        if kind == symbol_short!("deposit") {
            let amount_a = to_i128(desired_amounts.get(0).unwrap())?;
            let amount_b = to_i128(desired_amounts.get(1).unwrap())?;
            let min_a = to_i128(min_amounts.get(0).unwrap())?;
            let min_b = to_i128(min_amounts.get(1).unwrap())?;
            let mut auth_entries = Vec::new(&env);
            add_transfer_auth(
                &env,
                &mut auth_entries,
                token_a.clone(),
                user.clone(),
                pool.clone(),
                amount_a,
            );
            add_transfer_auth(
                &env,
                &mut auth_entries,
                token_b.clone(),
                user.clone(),
                pool.clone(),
                amount_b,
            );
            env.authorize_as_current_contract(auth_entries);
            let minted: Result<(i128, i128, i128), soroban_sdk::Error> = env.invoke_contract(
                &router,
                &Symbol::new(&env, "add_liquidity"),
                Vec::from_array(
                    &env,
                    [
                        token_a.into_val(&env),
                        token_b.into_val(&env),
                        amount_a.into_val(&env),
                        amount_b.into_val(&env),
                        min_a.into_val(&env),
                        min_b.into_val(&env),
                        user.into_val(&env),
                        deadline.into_val(&env),
                    ],
                ),
            );
            let (_, _, liquidity) = minted.map_err(|_| Error::OperationNotAllowed)?;
            if liquidity < to_i128(min_shares)? {
                return Err(Error::OperationNotAllowed);
            }
        } else {
            let liquidity = to_i128(share_amount)?;
            let min_a = to_i128(min_amounts.get(0).unwrap())?;
            let min_b = to_i128(min_amounts.get(1).unwrap())?;
            let mut auth_entries = Vec::new(&env);
            // Soroswap pairs are also the LP share token contracts.
            add_transfer_auth(
                &env,
                &mut auth_entries,
                pool.clone(),
                user.clone(),
                pool.clone(),
                liquidity,
            );
            env.authorize_as_current_contract(auth_entries);
            let withdrawn: Result<(i128, i128), soroban_sdk::Error> = env.invoke_contract(
                &router,
                &Symbol::new(&env, "remove_liquidity"),
                Vec::from_array(
                    &env,
                    [
                        token_a.into_val(&env),
                        token_b.into_val(&env),
                        liquidity.into_val(&env),
                        min_a.into_val(&env),
                        min_b.into_val(&env),
                        user.into_val(&env),
                        deadline.into_val(&env),
                    ],
                ),
            );
            withdrawn.map_err(|_| Error::OperationNotAllowed)?;
        }
        CopyExecuted {
            session_id,
            source_event_id,
            pool,
            kind,
            quote,
        }
        .publish(&env);
        Ok(())
    }

    /// Execute the Phoenix XYK pool ABI. Phoenix Stable has a different
    /// provide_liquidity signature and must use a separate adapter path.
    pub fn execute_phoenix_xyk_standard_op(
        env: Env,
        session_id: u32,
        source_event_id: BytesN<32>,
        pool: Address,
        kind: Symbol,
        quote: i128,
        desired_amounts: Vec<u128>,
        share_amount: u128,
        min_amounts: Vec<u128>,
    ) -> Result<(), Error> {
        if kind != symbol_short!("deposit") && kind != symbol_short!("withdraw") {
            return Err(Error::OperationNotAllowed);
        }
        if min_amounts.len() != 2 || (kind == symbol_short!("deposit") && desired_amounts.len() != 2) {
            return Err(Error::InvalidEvent);
        }
        let event = Self::load_leader_event(&env, &source_event_id)?;
        let session = Self::load_session(&env, session_id)?;
        if event.pool != pool || event.kind != kind || event.leader != session.leader {
            return Err(Error::EventMismatch);
        }
        if scale_i128(event.quote, session.coefficient_ppm)? != quote {
            return Err(Error::EventMismatch);
        }
        if kind == symbol_short!("deposit") {
            if scale_amounts(&env, &event.amounts, session.coefficient_ppm) != desired_amounts {
                return Err(Error::EventMismatch);
            }
            if min_amounts.get(0).unwrap() > desired_amounts.get(0).unwrap() ||
                min_amounts.get(1).unwrap() > desired_amounts.get(1).unwrap()
            {
                return Err(Error::InvalidLimit);
            }
        } else {
            let expected_shares = event
                .amounts
                .get(0)
                .map(|amount| scale_amount(&env, amount, session.coefficient_ppm))
                .unwrap_or(0);
            if expected_shares != share_amount {
                return Err(Error::EventMismatch);
            }
        }
        Self::authorize_copy_op(&env, session_id, &source_event_id, &pool, &kind, quote)?;

        let user = env.current_contract_address();
        let deadline = env.ledger().timestamp().saturating_add(300);
        let pool_info: PhoenixPoolInfo =
            env.invoke_contract(&pool, &Symbol::new(&env, "query_pool_info"), Vec::new(&env));
        if kind == symbol_short!("deposit") {
            let mut auth_entries = Vec::new(&env);
            add_transfer_auth(
                &env,
                &mut auth_entries,
                pool_info.asset_a.address.clone(),
                user.clone(),
                pool.clone(),
                to_i128(desired_amounts.get(0).unwrap())?,
            );
            add_transfer_auth(
                &env,
                &mut auth_entries,
                pool_info.asset_b.address.clone(),
                user.clone(),
                pool.clone(),
                to_i128(desired_amounts.get(1).unwrap())?,
            );
            env.authorize_as_current_contract(auth_entries);
            let desired_a = to_i128(desired_amounts.get(0).unwrap())?;
            let desired_b = to_i128(desired_amounts.get(1).unwrap())?;
            let min_a = to_i128(min_amounts.get(0).unwrap())?;
            let min_b = to_i128(min_amounts.get(1).unwrap())?;
            let _: () = env.invoke_contract(
                &pool,
                &Symbol::new(&env, "provide_liquidity"),
                Vec::from_array(
                    &env,
                    [
                        user.into_val(&env),
                        Option::<i128>::Some(desired_a).into_val(&env),
                        Option::<i128>::Some(min_a).into_val(&env),
                        Option::<i128>::Some(desired_b).into_val(&env),
                        Option::<i128>::Some(min_b).into_val(&env),
                        Option::<i64>::None.into_val(&env),
                        Option::<u64>::Some(deadline).into_val(&env),
                        false.into_val(&env),
                    ],
                ),
            );
        } else {
            let mut auth_entries = Vec::new(&env);
            add_transfer_auth(
                &env,
                &mut auth_entries,
                pool_info.asset_lp_share.address,
                user.clone(),
                pool.clone(),
                to_i128(share_amount)?,
            );
            env.authorize_as_current_contract(auth_entries);
            let _: (i128, i128) = env.invoke_contract(
                &pool,
                &Symbol::new(&env, "withdraw_liquidity"),
                Vec::from_array(
                    &env,
                    [
                        user.into_val(&env),
                        to_i128(share_amount)?.into_val(&env),
                        to_i128(min_amounts.get(0).unwrap())?.into_val(&env),
                        to_i128(min_amounts.get(1).unwrap())?.into_val(&env),
                        Option::<u64>::Some(deadline).into_val(&env),
                        Option::<u64>::None.into_val(&env),
                    ],
                ),
            );
        }
        CopyExecuted {
            session_id,
            source_event_id,
            pool,
            kind,
            quote,
        }
        .publish(&env);
        Ok(())
    }

    /// Execute the Phoenix Stable pool ABI. This is separate from the XYK
    /// entry point because Stable takes required token amounts and an optional
    /// minimum share output instead of per-token minimum amounts.
    pub fn execute_phoenix_stable_op(
        env: Env,
        session_id: u32,
        source_event_id: BytesN<32>,
        pool: Address,
        kind: Symbol,
        quote: i128,
        desired_amounts: Vec<u128>,
        min_shares: u128,
        share_amount: u128,
        min_amounts: Vec<u128>,
    ) -> Result<(), Error> {
        if kind != symbol_short!("deposit") && kind != symbol_short!("withdraw") {
            return Err(Error::OperationNotAllowed);
        }
        if desired_amounts.len() != 2 || min_amounts.len() != 2 {
            return Err(Error::InvalidEvent);
        }
        let event = Self::load_leader_event(&env, &source_event_id)?;
        let session = Self::load_session(&env, session_id)?;
        if event.pool != pool || event.kind != kind || event.leader != session.leader {
            return Err(Error::EventMismatch);
        }
        if scale_i128(event.quote, session.coefficient_ppm)? != quote {
            return Err(Error::EventMismatch);
        }
        if kind == symbol_short!("deposit") {
            if scale_amounts(&env, &event.amounts, session.coefficient_ppm) != desired_amounts {
                return Err(Error::EventMismatch);
            }
        } else {
            let expected_shares = event
                .amounts
                .get(0)
                .map(|amount| scale_amount(&env, amount, session.coefficient_ppm))
                .unwrap_or(0);
            if expected_shares != share_amount {
                return Err(Error::EventMismatch);
            }
        }
        Self::authorize_copy_op(&env, session_id, &source_event_id, &pool, &kind, quote)?;

        let user = env.current_contract_address();
        let deadline = env.ledger().timestamp().saturating_add(300);
        let pool_info: PhoenixPoolInfo =
            env.invoke_contract(&pool, &Symbol::new(&env, "query_pool_info"), Vec::new(&env));
        if kind == symbol_short!("deposit") {
            let mut auth_entries = Vec::new(&env);
            add_transfer_auth(
                &env,
                &mut auth_entries,
                pool_info.asset_a.address.clone(),
                user.clone(),
                pool.clone(),
                to_i128(desired_amounts.get(0).unwrap())?,
            );
            add_transfer_auth(
                &env,
                &mut auth_entries,
                pool_info.asset_b.address.clone(),
                user.clone(),
                pool.clone(),
                to_i128(desired_amounts.get(1).unwrap())?,
            );
            env.authorize_as_current_contract(auth_entries);
            let _: () = env.invoke_contract(
                &pool,
                &Symbol::new(&env, "provide_liquidity"),
                Vec::from_array(
                    &env,
                    [
                        user.into_val(&env),
                        to_i128(desired_amounts.get(0).unwrap())?.into_val(&env),
                        to_i128(desired_amounts.get(1).unwrap())?.into_val(&env),
                        Option::<i64>::None.into_val(&env),
                        Option::<u64>::Some(deadline).into_val(&env),
                        Option::<u128>::Some(min_shares).into_val(&env),
                        false.into_val(&env),
                    ],
                ),
            );
        } else {
            let mut auth_entries = Vec::new(&env);
            add_transfer_auth(
                &env,
                &mut auth_entries,
                pool_info.asset_lp_share.address,
                user.clone(),
                pool.clone(),
                to_i128(share_amount)?,
            );
            env.authorize_as_current_contract(auth_entries);
            let _: (i128, i128) = env.invoke_contract(
                &pool,
                &Symbol::new(&env, "withdraw_liquidity"),
                Vec::from_array(
                    &env,
                    [
                        user.into_val(&env),
                        to_i128(share_amount)?.into_val(&env),
                        to_i128(min_amounts.get(0).unwrap())?.into_val(&env),
                        to_i128(min_amounts.get(1).unwrap())?.into_val(&env),
                        Option::<u64>::Some(deadline).into_val(&env),
                        Option::<u64>::None.into_val(&env),
                    ],
                ),
            );
        }
        CopyExecuted {
            session_id,
            source_event_id,
            pool,
            kind,
            quote,
        }
        .publish(&env);
        Ok(())
    }

    /// Authorize and invoke one Aquarius standard-pool operation. The policy
    /// contract is the `user` argument, so the relayer never receives custody
    /// of follower funds. Token balances must be provisioned separately.
    pub fn execute_aquarius_standard_op(
        env: Env,
        session_id: u32,
        source_event_id: BytesN<32>,
        pool: Address,
        kind: Symbol,
        quote: i128,
        desired_amounts: Vec<u128>,
        min_shares: u128,
        share_amount: u128,
        min_amounts: Vec<u128>,
        claim_token: Address,
    ) -> Result<(), Error> {
        if kind != symbol_short!("deposit") && kind != symbol_short!("withdraw") && kind != symbol_short!("claim") {
            return Err(Error::OperationNotAllowed);
        }
        let event = Self::load_leader_event(&env, &source_event_id)?;
        let session = Self::load_session(&env, session_id)?;
        if event.pool != pool || event.kind != kind {
            return Err(Error::EventMismatch);
        }
        if event.leader != session.leader {
            return Err(Error::EventMismatch);
        }
        if kind == symbol_short!("claim") && event.claim_token.as_ref() != Some(&claim_token) {
            return Err(Error::EventMismatch);
        }
        if scale_i128(event.quote, session.coefficient_ppm)? != quote {
            return Err(Error::EventMismatch);
        }
        if kind == symbol_short!("deposit") &&
            scale_amounts(&env, &event.amounts, session.coefficient_ppm) != desired_amounts
        {
            return Err(Error::EventMismatch);
        }
        if kind == symbol_short!("withdraw") {
            let expected_shares = if event.amounts.is_empty() {
                0
            } else {
                scale_amount(&env, event.amounts.get(0).unwrap(), session.coefficient_ppm)
            };
            if expected_shares != share_amount {
                return Err(Error::EventMismatch);
            }
        }
        Self::authorize_copy_op(&env, session_id, &source_event_id, &pool, &kind, quote)?;
        let user = env.current_contract_address();
        if kind == symbol_short!("deposit") {
            let tokens: Vec<Address> = env.invoke_contract(&pool, &Symbol::new(&env, "get_tokens"), Vec::new(&env));
            if tokens.len() == desired_amounts.len() {
                let mut auth_entries = Vec::new(&env);
                for i in 0..desired_amounts.len() {
                    let amount = desired_amounts.get(i).unwrap();
                    if amount == 0 {
                        continue;
                    }
                    let token = tokens.get(i).unwrap();
                    auth_entries.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
                        context: ContractContext {
                            contract: token,
                            fn_name: symbol_short!("transfer"),
                            args: Vec::from_array(
                                &env,
                                [
                                    user.clone().into_val(&env),
                                    pool.clone().into_val(&env),
                                    (amount as i128).into_val(&env),
                                ],
                            ),
                        },
                        sub_invocations: Vec::new(&env),
                    }));
                }
                env.authorize_as_current_contract(auth_entries);
            }
            let _: (Vec<u128>, u128) = env.invoke_contract(
                &pool,
                &symbol_short!("deposit"),
                Vec::from_array(
                    &env,
                    [
                        user.into_val(&env),
                        desired_amounts.into_val(&env),
                        min_shares.into_val(&env),
                    ],
                ),
            );
        } else if kind == symbol_short!("withdraw") {
            let tokens: Vec<Address> = env.invoke_contract(&pool, &Symbol::new(&env, "get_tokens"), Vec::new(&env));
            let reserves: Vec<u128> = env.invoke_contract(&pool, &Symbol::new(&env, "get_reserves"), Vec::new(&env));
            let total_shares: u128 = env.invoke_contract(&pool, &Symbol::new(&env, "get_total_shares"), Vec::new(&env));
            if tokens.len() == 2 && reserves.len() == 2 && total_shares > 0 {
                let out_a = proportional_floor(&env, reserves.get(0).unwrap(), share_amount, total_shares);
                let out_b = proportional_floor(&env, reserves.get(1).unwrap(), share_amount, total_shares);
                let share_id: Address = env.invoke_contract(&pool, &symbol_short!("share_id"), Vec::new(&env));
                let mut auth_entries = Vec::new(&env);
                auth_entries.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
                    context: ContractContext {
                        contract: share_id,
                        fn_name: symbol_short!("burn"),
                        args: Vec::from_array(
                            &env,
                            [user.clone().into_val(&env), (share_amount as i128).into_val(&env)],
                        ),
                    },
                    sub_invocations: Vec::new(&env),
                }));
                for (token, amount) in [(tokens.get(0).unwrap(), out_a), (tokens.get(1).unwrap(), out_b)] {
                    if amount == 0 {
                        continue;
                    }
                    auth_entries.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
                        context: ContractContext {
                            contract: token,
                            fn_name: symbol_short!("transfer"),
                            args: Vec::from_array(
                                &env,
                                [
                                    pool.clone().into_val(&env),
                                    user.clone().into_val(&env),
                                    (amount as i128).into_val(&env),
                                ],
                            ),
                        },
                        sub_invocations: Vec::new(&env),
                    }));
                }
                env.authorize_as_current_contract(auth_entries);
            }
            let _: Vec<u128> = env.invoke_contract(
                &pool,
                &symbol_short!("withdraw"),
                Vec::from_array(
                    &env,
                    [
                        user.into_val(&env),
                        share_amount.into_val(&env),
                        min_amounts.into_val(&env),
                    ],
                ),
            );
        } else if kind == symbol_short!("claim") {
            let reward_amount: u128 = env.invoke_contract(
                &pool,
                &Symbol::new(&env, "get_user_reward"),
                Vec::from_array(&env, [user.clone().into_val(&env)]),
            );
            if reward_amount > 0 {
                let mut auth_entries = Vec::new(&env);
                auth_entries.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
                    context: ContractContext {
                        contract: claim_token,
                        fn_name: symbol_short!("transfer"),
                        args: Vec::from_array(
                            &env,
                            [
                                pool.clone().into_val(&env),
                                user.clone().into_val(&env),
                                (reward_amount as i128).into_val(&env),
                            ],
                        ),
                    },
                    sub_invocations: Vec::new(&env),
                }));
                env.authorize_as_current_contract(auth_entries);
            }
            let _: u128 = env.invoke_contract(
                &pool,
                &symbol_short!("claim"),
                Vec::from_array(&env, [user.into_val(&env)]),
            );
        }
        CopyExecuted {
            session_id,
            source_event_id,
            pool,
            kind,
            quote,
        }
        .publish(&env);
        Ok(())
    }

    pub fn session(env: Env, session_id: u32) -> Result<Session, Error> {
        Self::load_session(&env, session_id)
    }

    fn owner(env: &Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Owner)
            .ok_or(Error::NotInitialized)
    }

    fn relayer(env: &Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Relayer)
            .ok_or(Error::NotInitialized)
    }

    fn load_session(env: &Env, session_id: u32) -> Result<Session, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Session(session_id))
            .ok_or(Error::SessionNotFound)
    }

    fn load_leader_event(env: &Env, source_event_id: &BytesN<32>) -> Result<LeaderEvent, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::LeaderEvent(source_event_id.clone()))
            .ok_or(Error::EventNotFound)
    }

    fn authorize_copy_op(
        env: &Env,
        session_id: u32,
        source_event_id: &BytesN<32>,
        pool: &Address,
        kind: &Symbol,
        quote: i128,
    ) -> Result<(), Error> {
        Self::relayer(env)?.require_auth();
        if env
            .storage()
            .persistent()
            .has(&DataKey::Replay(session_id, source_event_id.clone()))
        {
            return Err(Error::Replay);
        }
        let mut session = Self::load_session(env, session_id)?;
        let now = env.ledger().timestamp();
        if session.paused {
            return Err(Error::SessionPaused);
        }
        if now >= session.expires_at {
            return Err(Error::SessionExpired);
        }
        if !session.allowed_pools.iter().any(|allowed| allowed == *pool) {
            return Err(Error::PoolNotAllowed);
        }
        if *kind == symbol_short!("claim") && !session.follow_claims {
            return Err(Error::OperationNotAllowed);
        }
        if *kind != symbol_short!("deposit") && *kind != symbol_short!("withdraw") && *kind != symbol_short!("claim") {
            return Err(Error::OperationNotAllowed);
        }
        if quote <= 0 || quote > session.max_per_op_quote {
            return Err(Error::PerOperationLimit);
        }
        if day(now) != session.daily_day {
            session.daily_day = day(now);
            session.daily_used_quote = 0;
        }
        if session.daily_used_quote + quote > session.max_daily_quote {
            return Err(Error::DailyLimit);
        }
        session.daily_used_quote += quote;
        env.storage().persistent().set(&DataKey::Session(session_id), &session);
        env.storage()
            .persistent()
            .set(&DataKey::Replay(session_id, source_event_id.clone()), &true);
        Ok(())
    }
}

fn day(timestamp: u64) -> u64 {
    timestamp / 86_400
}

fn scale_i128(value: i128, coefficient_ppm: u32) -> Result<i128, Error> {
    if value < 0 {
        return Err(Error::InvalidEvent);
    }
    value
        .checked_mul(i128::from(coefficient_ppm))
        .and_then(|scaled| scaled.checked_div(COEFFICIENT_SCALE as i128))
        .ok_or(Error::InvalidEvent)
}

fn scale_amounts(env: &Env, amounts: &Vec<u128>, coefficient_ppm: u32) -> Vec<u128> {
    let mut scaled = Vec::new(env);
    for i in 0..amounts.len() {
        let amount = amounts.get(i).unwrap();
        let value = U256::from_u128(env, amount)
            .mul(&U256::from_u128(env, u128::from(coefficient_ppm)))
            .div(&U256::from_u128(env, COEFFICIENT_SCALE))
            .to_u128()
            .unwrap();
        scaled.push_back(value);
    }
    scaled
}

fn scale_amount(env: &Env, amount: u128, coefficient_ppm: u32) -> u128 {
    U256::from_u128(env, amount)
        .mul(&U256::from_u128(env, u128::from(coefficient_ppm)))
        .div(&U256::from_u128(env, COEFFICIENT_SCALE))
        .to_u128()
        .unwrap()
}

fn to_i128(amount: u128) -> Result<i128, Error> {
    i128::try_from(amount).map_err(|_| Error::InvalidLimit)
}

fn proportional_floor(env: &Env, reserve: u128, shares: u128, total_shares: u128) -> u128 {
    U256::from_u128(env, reserve)
        .mul(&U256::from_u128(env, shares))
        .div(&U256::from_u128(env, total_shares))
        .to_u128()
        .unwrap()
}

#[cfg(test)]
mod test {
    use {
        super::*,
        soroban_sdk::{
            testutils::{Address as _, Events as _, Ledger as _},
            vec, Val,
        },
    };

    #[contract]
    struct MockPool;

    #[contract]
    struct MockStablePool;

    #[contract]
    struct MockToken;

    #[contract]
    struct MockSoroswapRouter;

    #[contracterror]
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    #[repr(u32)]
    enum MockRouterError {
        Failed = 1,
    }

    #[contractimpl]
    impl MockToken {
        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            from.require_auth();
            env.storage().instance().set(&symbol_short!("from"), &from);
            env.storage().instance().set(&symbol_short!("to"), &to);
            env.storage().instance().set(&symbol_short!("amount"), &amount);
        }

        pub fn last_transfer(env: Env) -> (Address, Address, i128) {
            (
                env.storage().instance().get(&symbol_short!("from")).unwrap(),
                env.storage().instance().get(&symbol_short!("to")).unwrap(),
                env.storage().instance().get(&symbol_short!("amount")).unwrap(),
            )
        }
    }

    #[contractimpl]
    impl MockPool {
        pub fn token_0(env: Env) -> Address {
            Address::generate(&env)
        }

        pub fn token_1(env: Env) -> Address {
            Address::generate(&env)
        }

        pub fn get_tokens(env: Env) -> Vec<Address> {
            Vec::new(&env)
        }

        pub fn get_reserves(env: Env) -> Vec<u128> {
            Vec::new(&env)
        }

        pub fn get_total_shares(_env: Env) -> u128 {
            0
        }

        pub fn share_id(env: Env) -> Address {
            Address::generate(&env)
        }

        pub fn query_pool_info(env: Env) -> PhoenixPoolInfo {
            PhoenixPoolInfo {
                asset_a: PhoenixAsset {
                    address: Address::generate(&env),
                    amount: 1_000_000,
                },
                asset_b: PhoenixAsset {
                    address: Address::generate(&env),
                    amount: 1_000_000,
                },
                asset_lp_share: PhoenixAsset {
                    address: Address::generate(&env),
                    amount: 1_000_000,
                },
            }
        }

        pub fn deposit(env: Env, user: Address, desired_amounts: Vec<u128>, _min_shares: u128) -> (Vec<u128>, u128) {
            if env.storage().instance().get(&symbol_short!("fail")).unwrap_or(false) {
                panic!("configured mock pool failure");
            }
            env.storage().instance().set(&symbol_short!("user"), &user);
            (desired_amounts, 1)
        }

        pub fn provide_liquidity(
            env: Env,
            user: Address,
            _desired_a: Option<i128>,
            _min_a: Option<i128>,
            _desired_b: Option<i128>,
            _min_b: Option<i128>,
            _slippage: Option<i64>,
            _deadline: Option<u64>,
            _auto_stake: bool,
        ) {
            env.storage().instance().set(&symbol_short!("user"), &user);
        }

        pub fn withdraw(env: Env, user: Address, share_amount: u128, _min_amounts: Vec<u128>) -> Vec<u128> {
            env.storage().instance().set(&symbol_short!("user"), &user);
            Vec::from_array(&env, [share_amount])
        }

        pub fn withdraw_liquidity(
            env: Env,
            user: Address,
            share_amount: i128,
            _min_a: i128,
            _min_b: i128,
            _deadline: Option<u64>,
            _auto_unstake: Option<u64>,
        ) -> (i128, i128) {
            env.storage().instance().set(&symbol_short!("user"), &user);
            (share_amount, share_amount)
        }

        pub fn claim(env: Env, user: Address) -> u128 {
            env.storage().instance().set(&symbol_short!("user"), &user);
            let amount: u128 = env.storage().instance().get(&symbol_short!("reward")).unwrap_or(0);
            env.storage().instance().set(&symbol_short!("claimed"), &amount);
            if amount > 0 {
                let token: Address = env.storage().instance().get(&symbol_short!("token")).unwrap();
                env.invoke_contract::<()>(
                    &token,
                    &symbol_short!("transfer"),
                    Vec::from_array(
                        &env,
                        [
                            env.current_contract_address().into_val(&env),
                            user.into_val(&env),
                            (amount as i128).into_val(&env),
                        ],
                    ),
                );
            }
            amount
        }

        pub fn get_user_reward(env: Env, _user: Address) -> u128 {
            env.storage().instance().get(&symbol_short!("reward")).unwrap_or(0)
        }

        pub fn configure_reward(env: Env, token: Address, amount: u128) {
            env.storage().instance().set(&symbol_short!("token"), &token);
            env.storage().instance().set(&symbol_short!("reward"), &amount);
        }

        pub fn configure_failure(env: Env, fail: bool) {
            env.storage().instance().set(&symbol_short!("fail"), &fail);
        }

        pub fn last_user(env: Env) -> Address {
            env.storage().instance().get(&symbol_short!("user")).unwrap()
        }

        pub fn last_claim_amount(env: Env) -> u128 {
            env.storage().instance().get(&symbol_short!("claimed")).unwrap_or(0)
        }
    }

    #[contractimpl]
    impl MockStablePool {
        pub fn query_pool_info(env: Env) -> PhoenixPoolInfo {
            PhoenixPoolInfo {
                asset_a: PhoenixAsset {
                    address: Address::generate(&env),
                    amount: 1_000_000,
                },
                asset_b: PhoenixAsset {
                    address: Address::generate(&env),
                    amount: 1_000_000,
                },
                asset_lp_share: PhoenixAsset {
                    address: Address::generate(&env),
                    amount: 1_000_000,
                },
            }
        }

        pub fn provide_liquidity(
            env: Env,
            user: Address,
            _desired_a: i128,
            _desired_b: i128,
            _slippage: Option<i64>,
            _deadline: Option<u64>,
            _min_shares: Option<u128>,
            _auto_stake: bool,
        ) {
            env.storage().instance().set(&symbol_short!("user"), &user);
        }

        pub fn withdraw_liquidity(
            env: Env,
            user: Address,
            share_amount: i128,
            _min_a: i128,
            _min_b: i128,
            _deadline: Option<u64>,
            _auto_unstake: Option<u64>,
        ) -> (i128, i128) {
            env.storage().instance().set(&symbol_short!("user"), &user);
            (share_amount, share_amount)
        }

        pub fn last_user(env: Env) -> Address {
            env.storage().instance().get(&symbol_short!("user")).unwrap()
        }
    }

    #[contractimpl]
    impl MockSoroswapRouter {
        pub fn add_liquidity(
            _env: Env,
            _token_a: Address,
            _token_b: Address,
            amount_a_desired: i128,
            amount_b_desired: i128,
            _amount_a_min: i128,
            _amount_b_min: i128,
            to: Address,
            _deadline: u64,
        ) -> Result<(i128, i128, i128), MockRouterError> {
            to.require_auth();
            Ok((amount_a_desired, amount_b_desired, 123))
        }

        pub fn remove_liquidity(
            _env: Env,
            _token_a: Address,
            _token_b: Address,
            liquidity: i128,
            _amount_a_min: i128,
            _amount_b_min: i128,
            to: Address,
            _deadline: u64,
        ) -> Result<(i128, i128), MockRouterError> {
            to.require_auth();
            Ok((liquidity, liquidity))
        }
    }

    #[test]
    fn policy_enforces_limits_pause_and_replay() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = env.register(CopyPolicy, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let pool = Address::generate(&env);
        let id = BytesN::from_array(&env, &[7; 32]);
        CopyPolicyClient::new(&env, &contract).initialize(&owner, &relayer);
        CopyPolicyClient::new(&env, &contract).set_event_recorder(&recorder);
        let mut pools = Vec::new(&env);
        pools.push_back(pool.clone());
        CopyPolicyClient::new(&env, &contract).register_session(&1, &owner, &pools, &true, &10, &15, &100_000);
        CopyPolicyClient::new(&env, &contract).record_leader_event(
            &id,
            &owner,
            &pool,
            &symbol_short!("deposit"),
            &Vec::from_array(&env, [100u128]),
            &10,
            &1,
        );
        CopyPolicyClient::new(&env, &contract).execute_copy_op(&1, &id, &pool, &symbol_short!("deposit"), &10);
        assert_eq!(
            env.events().all(),
            vec![
                &env,
                (
                    contract.clone(),
                    (symbol_short!("copy"), 1u32).into_val(&env),
                    Vec::<Val>::from_array(
                        &env,
                        [
                            id.clone().into_val(&env),
                            pool.clone().into_val(&env),
                            symbol_short!("deposit").into_val(&env),
                            10i128.into_val(&env),
                        ],
                    )
                    .into_val(&env),
                ),
            ]
        );
        assert!(CopyPolicyClient::new(&env, &contract)
            .try_execute_copy_op(&1, &id, &pool, &symbol_short!("deposit"), &10)
            .is_err());
        CopyPolicyClient::new(&env, &contract).pause_session(&1);
        let second = BytesN::from_array(&env, &[8; 32]);
        assert!(CopyPolicyClient::new(&env, &contract)
            .try_execute_copy_op(&1, &second, &pool, &symbol_short!("deposit"), &1)
            .is_err());
    }

    #[test]
    fn daily_limit_and_expiry_are_enforced_on_chain() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let pool = Address::generate(&env);
        let client = CopyPolicyClient::new(&env, &policy);

        client.initialize(&owner, &relayer);
        client.set_event_recorder(&recorder);
        let mut pools = Vec::new(&env);
        pools.push_back(pool.clone());
        client.register_session(&12, &owner, &pools, &true, &10, &10, &100);

        let first = BytesN::from_array(&env, &[12; 32]);
        client.record_leader_event(
            &first,
            &owner,
            &pool,
            &symbol_short!("deposit"),
            &Vec::from_array(&env, [100u128]),
            &7,
            &1,
        );
        client.execute_copy_op(&12, &first, &pool, &symbol_short!("deposit"), &7);
        let second = BytesN::from_array(&env, &[13; 32]);
        assert!(client
            .try_execute_copy_op(&12, &second, &pool, &symbol_short!("deposit"), &4)
            .is_err());
        assert_eq!(client.session(&12).daily_used_quote, 7);

        env.ledger().set_timestamp(100);
        let expired = BytesN::from_array(&env, &[14; 32]);
        assert!(client
            .try_execute_copy_op(&12, &expired, &pool, &symbol_short!("deposit"), &1)
            .is_err());
    }

    #[test]
    fn unrecorded_or_mismatched_events_cannot_execute() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let pool = Address::generate(&env);
        let client = CopyPolicyClient::new(&env, &policy);

        client.initialize(&owner, &relayer);
        client.set_event_recorder(&recorder);
        client.register_session(
            &21,
            &owner,
            &Vec::from_array(&env, [pool.clone()]),
            &true,
            &100,
            &100,
            &100_000,
        );

        let event_id = BytesN::from_array(&env, &[21; 32]);
        assert!(client
            .try_execute_copy_op(&21, &event_id, &pool, &symbol_short!("deposit"), &10)
            .is_err());

        client.record_leader_event(
            &event_id,
            &owner,
            &pool,
            &symbol_short!("deposit"),
            &Vec::from_array(&env, [100u128]),
            &10,
            &1,
        );
        assert!(client
            .try_execute_copy_op(&21, &event_id, &pool, &symbol_short!("withdraw"), &10)
            .is_err());
        assert!(client
            .try_execute_copy_op(&21, &event_id, &pool, &symbol_short!("deposit"), &11)
            .is_err());
        client.execute_copy_op(&21, &event_id, &pool, &symbol_short!("deposit"), &10);
    }

    #[test]
    fn coefficient_is_applied_to_recorded_quote_on_chain() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let pool = Address::generate(&env);
        let client = CopyPolicyClient::new(&env, &policy);

        client.initialize(&owner, &relayer);
        client.set_event_recorder(&recorder);
        client.register_session_coeff(
            &22,
            &owner,
            &Vec::from_array(&env, [pool.clone()]),
            &250_000,
            &true,
            &25,
            &25,
            &100_000,
        );
        let event_id = BytesN::from_array(&env, &[22; 32]);
        client.record_leader_event(
            &event_id,
            &owner,
            &pool,
            &symbol_short!("deposit"),
            &Vec::from_array(&env, [100u128]),
            &100,
            &1,
        );

        assert!(client
            .try_execute_copy_op(&22, &event_id, &pool, &symbol_short!("deposit"), &100)
            .is_err());
        client.execute_copy_op(&22, &event_id, &pool, &symbol_short!("deposit"), &25);
        assert_eq!(client.session(&22).daily_used_quote, 25);
    }

    #[test]
    fn session_must_match_event_leader() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let pool = env.register(MockPool, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let registered_leader = Address::generate(&env);
        let other_leader = Address::generate(&env);
        let client = CopyPolicyClient::new(&env, &policy);

        client.initialize(&owner, &relayer);
        client.set_event_recorder(&recorder);
        client.register_session(
            &23,
            &registered_leader,
            &Vec::from_array(&env, [pool.clone()]),
            &true,
            &100,
            &100,
            &100_000,
        );
        let event_id = BytesN::from_array(&env, &[23; 32]);
        client.record_leader_event(
            &event_id,
            &other_leader,
            &pool,
            &symbol_short!("deposit"),
            &Vec::from_array(&env, [10u128]),
            &10,
            &1,
        );

        assert!(client
            .try_execute_copy_op(&23, &event_id, &pool, &symbol_short!("deposit"), &10)
            .is_err());
    }

    #[test]
    fn withdraw_share_amount_must_match_scaled_event_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let pool = env.register(MockPool, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let leader = Address::generate(&env);
        let client = CopyPolicyClient::new(&env, &policy);

        client.initialize(&owner, &relayer);
        client.set_event_recorder(&recorder);
        client.register_session_coeff(
            &24,
            &leader,
            &Vec::from_array(&env, [pool.clone()]),
            &500_000,
            &true,
            &100,
            &100,
            &100_000,
        );
        let event_id = BytesN::from_array(&env, &[24; 32]);
        client.record_leader_event(
            &event_id,
            &leader,
            &pool,
            &symbol_short!("withdraw"),
            &Vec::from_array(&env, [10u128]),
            &10,
            &1,
        );

        assert!(client
            .try_execute_aquarius_standard_op(
                &24,
                &event_id,
                &pool,
                &symbol_short!("withdraw"),
                &5,
                &Vec::new(&env),
                &0,
                &4,
                &Vec::new(&env),
                &Address::generate(&env),
            )
            .is_err());
        client.execute_aquarius_standard_op(
            &24,
            &event_id,
            &pool,
            &symbol_short!("withdraw"),
            &5,
            &Vec::new(&env),
            &0,
            &5,
            &Vec::new(&env),
            &Address::generate(&env),
        );
    }

    #[test]
    fn disarm_removes_session_and_expiry_blocks_resume() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let pool = Address::generate(&env);
        let client = CopyPolicyClient::new(&env, &policy);

        client.initialize(&owner, &relayer);
        let mut pools = Vec::new(&env);
        pools.push_back(pool);
        client.register_session(&13, &owner, &pools, &true, &10, &10, &100);
        client.pause_session(&13);
        client.resume_session(&13);
        client.disarm_session(&13);
        assert!(client.try_session(&13).is_err());

        client.register_session(&14, &owner, &Vec::new(&env), &true, &10, &10, &100);
        env.ledger().set_timestamp(100);
        assert!(client.try_resume_session(&14).is_err());
    }

    #[test]
    fn standard_operations_call_pool_as_policy_contract() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let pool = env.register(MockPool, ());
        let token = env.register(MockToken, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let leader = Address::generate(&env);
        let pool_address = pool.clone();
        let policy_client = CopyPolicyClient::new(&env, &policy);

        policy_client.initialize(&owner, &relayer);
        policy_client.set_event_recorder(&recorder);
        let mut pools = Vec::new(&env);
        pools.push_back(pool_address.clone());
        policy_client.register_session(&7, &leader, &pools, &true, &100, &300, &100_000);

        let deposit_id = BytesN::from_array(&env, &[1; 32]);
        let mut desired = Vec::new(&env);
        desired.push_back(10);
        policy_client.record_leader_event(
            &deposit_id,
            &leader,
            &pool_address,
            &symbol_short!("deposit"),
            &desired,
            &10,
            &1,
        );
        assert!(policy_client
            .try_execute_standard_op(
                &Symbol::new(&env, "soroswap_amm"),
                &7,
                &deposit_id,
                &pool_address,
                &symbol_short!("deposit"),
                &10,
                &desired,
                &0,
                &0,
                &Vec::new(&env),
                &Address::generate(&env),
            )
            .is_err());
        assert_eq!(policy_client.session(&7).daily_used_quote, 0);
        policy_client.execute_aquarius_standard_op(
            &7,
            &deposit_id,
            &pool_address,
            &symbol_short!("deposit"),
            &10,
            &desired,
            &0,
            &0,
            &Vec::new(&env),
            &Address::generate(&env),
        );
        let pool_client = MockPoolClient::new(&env, &pool);
        assert_eq!(pool_client.last_user(), policy);

        let withdraw_id = BytesN::from_array(&env, &[2; 32]);
        policy_client.record_leader_event(
            &withdraw_id,
            &leader,
            &pool_address,
            &symbol_short!("withdraw"),
            &Vec::from_array(&env, [4u128]),
            &10,
            &2,
        );
        policy_client.execute_aquarius_standard_op(
            &7,
            &withdraw_id,
            &pool_address,
            &symbol_short!("withdraw"),
            &10,
            &Vec::new(&env),
            &0,
            &4,
            &Vec::new(&env),
            &Address::generate(&env),
        );
        assert_eq!(pool_client.last_user(), policy);

        let claim_id = BytesN::from_array(&env, &[3; 32]);
        policy_client.record_claim_event(
            &claim_id,
            &leader,
            &pool_address,
            &Vec::from_array(&env, [7u128]),
            &10,
            &3,
            &token,
        );
        MockPoolClient::new(&env, &pool).configure_reward(&token, &7);
        policy_client.execute_aquarius_standard_op(
            &7,
            &claim_id,
            &pool_address,
            &symbol_short!("claim"),
            &10,
            &Vec::new(&env),
            &0,
            &0,
            &Vec::new(&env),
            &token,
        );
        assert_eq!(pool_client.last_user(), policy);
        assert_eq!(pool_client.last_claim_amount(), 7);
        assert_eq!(
            MockTokenClient::new(&env, &token).last_transfer(),
            (pool_address, policy, 7)
        );
    }

    #[test]
    fn soroswap_route_requires_owner_configured_router_and_keeps_budget_intact() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let pool = env.register(MockPool, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let leader = Address::generate(&env);
        let client = CopyPolicyClient::new(&env, &policy);

        client.initialize(&owner, &relayer);
        client.set_event_recorder(&recorder);
        client.register_session(
            &31,
            &leader,
            &Vec::from_array(&env, [pool.clone()]),
            &true,
            &100,
            &100,
            &100_000,
        );
        let event_id = BytesN::from_array(&env, &[31; 32]);
        client.record_leader_event(
            &event_id,
            &leader,
            &pool,
            &symbol_short!("deposit"),
            &Vec::from_array(&env, [10u128, 20u128]),
            &10,
            &1,
        );

        let venue = Symbol::new(&env, "soroswap_amm");
        assert!(client
            .try_execute_standard_op(
                &venue,
                &31,
                &event_id,
                &pool,
                &symbol_short!("deposit"),
                &10,
                &Vec::from_array(&env, [10u128, 20u128]),
                &0,
                &0,
                &Vec::from_array(&env, [0u128, 0u128]),
                &Address::generate(&env),
            )
            .is_err());
        assert_eq!(client.session(&31).daily_used_quote, 0);
        assert!(client.try_venue_router(&venue).is_err());

        assert!(client
            .try_set_venue_router(&Symbol::new(&env, "phoenix"), &Address::generate(&env))
            .is_err());
        let router = Address::generate(&env);
        client.set_venue_router(&venue, &router);
        assert_eq!(client.venue_router(&venue), router);
    }

    #[test]
    fn soroswap_route_executes_deposit_and_withdraw_through_allowlisted_router() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let pool = env.register(MockPool, ());
        let router = env.register(MockSoroswapRouter, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let leader = Address::generate(&env);
        let client = CopyPolicyClient::new(&env, &policy);
        let venue = Symbol::new(&env, "soroswap_amm");

        client.initialize(&owner, &relayer);
        client.set_event_recorder(&recorder);
        client.set_venue_router(&venue, &router);
        client.register_session(
            &32,
            &leader,
            &Vec::from_array(&env, [pool.clone()]),
            &true,
            &100,
            &100,
            &100_000,
        );

        let deposit_id = BytesN::from_array(&env, &[32; 32]);
        client.record_leader_event(
            &deposit_id,
            &leader,
            &pool,
            &symbol_short!("deposit"),
            &Vec::from_array(&env, [10u128, 20u128]),
            &10,
            &1,
        );
        client.execute_standard_op(
            &venue,
            &32,
            &deposit_id,
            &pool,
            &symbol_short!("deposit"),
            &10,
            &Vec::from_array(&env, [10u128, 20u128]),
            &100,
            &0,
            &Vec::from_array(&env, [9u128, 19u128]),
            &Address::generate(&env),
        );
        assert_eq!(client.session(&32).daily_used_quote, 10);

        let withdraw_id = BytesN::from_array(&env, &[33; 32]);
        client.record_leader_event(
            &withdraw_id,
            &leader,
            &pool,
            &symbol_short!("withdraw"),
            &Vec::from_array(&env, [4u128]),
            &5,
            &2,
        );
        client.execute_standard_op(
            &venue,
            &32,
            &withdraw_id,
            &pool,
            &symbol_short!("withdraw"),
            &5,
            &Vec::from_array(&env, [0u128, 0u128]),
            &0,
            &4,
            &Vec::from_array(&env, [3u128, 3u128]),
            &Address::generate(&env),
        );
        assert_eq!(client.session(&32).daily_used_quote, 15);
    }

    #[test]
    fn phoenix_xyk_route_uses_explicit_pool_abi_and_rejects_generic_phoenix() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let pool = env.register(MockPool, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let leader = Address::generate(&env);
        let client = CopyPolicyClient::new(&env, &policy);
        client.initialize(&owner, &relayer);
        client.set_event_recorder(&recorder);
        client.register_session(
            &41,
            &leader,
            &Vec::from_array(&env, [pool.clone()]),
            &true,
            &100,
            &100,
            &100_000,
        );

        let deposit_id = BytesN::from_array(&env, &[41; 32]);
        client.record_leader_event(
            &deposit_id,
            &leader,
            &pool,
            &symbol_short!("deposit"),
            &Vec::from_array(&env, [100u128, 200u128]),
            &10,
            &1,
        );
        assert!(client
            .try_execute_standard_op(
                &Symbol::new(&env, "phoenix"),
                &41,
                &deposit_id,
                &pool,
                &symbol_short!("deposit"),
                &10,
                &Vec::from_array(&env, [100u128, 200u128]),
                &0,
                &0,
                &Vec::from_array(&env, [99u128, 199u128]),
                &Address::generate(&env),
            )
            .is_err());
        client.execute_standard_op(
            &Symbol::new(&env, "phoenix_xyk"),
            &41,
            &deposit_id,
            &pool,
            &symbol_short!("deposit"),
            &10,
            &Vec::from_array(&env, [100u128, 200u128]),
            &0,
            &0,
            &Vec::from_array(&env, [99u128, 199u128]),
            &Address::generate(&env),
        );
        assert_eq!(MockPoolClient::new(&env, &pool).last_user(), policy);

        let withdraw_id = BytesN::from_array(&env, &[42; 32]);
        client.record_leader_event(
            &withdraw_id,
            &leader,
            &pool,
            &symbol_short!("withdraw"),
            &Vec::from_array(&env, [40u128]),
            &5,
            &2,
        );
        client.execute_phoenix_xyk_standard_op(
            &41,
            &withdraw_id,
            &pool,
            &symbol_short!("withdraw"),
            &5,
            &Vec::new(&env),
            &40,
            &Vec::from_array(&env, [1u128, 1u128]),
        );
        assert_eq!(client.session(&41).daily_used_quote, 15);
    }

    #[test]
    fn phoenix_stable_route_uses_required_amount_abi() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let pool = env.register(MockStablePool, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let leader = Address::generate(&env);
        let client = CopyPolicyClient::new(&env, &policy);
        client.initialize(&owner, &relayer);
        client.set_event_recorder(&recorder);
        client.register_session(
            &43,
            &leader,
            &Vec::from_array(&env, [pool.clone()]),
            &true,
            &100,
            &100,
            &100_000,
        );

        let event_id = BytesN::from_array(&env, &[43; 32]);
        client.record_leader_event(
            &event_id,
            &leader,
            &pool,
            &symbol_short!("deposit"),
            &Vec::from_array(&env, [100u128, 200u128]),
            &10,
            &1,
        );
        client.execute_standard_op(
            &Symbol::new(&env, "phoenix_stable"),
            &43,
            &event_id,
            &pool,
            &symbol_short!("deposit"),
            &10,
            &Vec::from_array(&env, [100u128, 200u128]),
            &90,
            &0,
            &Vec::from_array(&env, [0u128, 0u128]),
            &Address::generate(&env),
        );
        assert_eq!(MockStablePoolClient::new(&env, &pool).last_user(), policy);
    }

    #[test]
    fn claim_is_rejected_when_session_disables_claims() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let pool = env.register(MockPool, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let leader = Address::generate(&env);
        let pool_address = pool.clone();
        let client = CopyPolicyClient::new(&env, &policy);

        client.initialize(&owner, &relayer);
        client.set_event_recorder(&recorder);
        let mut pools = Vec::new(&env);
        pools.push_back(pool_address.clone());
        client.register_session(&9, &leader, &pools, &false, &100, &300, &100_000);

        let claim_id = BytesN::from_array(&env, &[9; 32]);
        client.record_leader_event(
            &claim_id,
            &leader,
            &pool_address,
            &symbol_short!("claim"),
            &Vec::from_array(&env, [1u128]),
            &10,
            &1,
        );
        assert!(client
            .try_execute_aquarius_standard_op(
                &9,
                &claim_id,
                &pool_address,
                &symbol_short!("claim"),
                &10,
                &Vec::new(&env),
                &0,
                &0,
                &Vec::new(&env),
                &Address::generate(&env),
            )
            .is_err());
    }

    #[test]
    fn downstream_failure_rolls_back_budget_and_replay_marker() {
        let env = Env::default();
        env.mock_all_auths();
        let policy = env.register(CopyPolicy, ());
        let pool = env.register(MockPool, ());
        let owner = Address::generate(&env);
        let relayer = Address::generate(&env);
        let recorder = Address::generate(&env);
        let leader = Address::generate(&env);
        let pool_address = pool.clone();
        let client = CopyPolicyClient::new(&env, &policy);

        client.initialize(&owner, &relayer);
        client.set_event_recorder(&recorder);
        let mut pools = Vec::new(&env);
        pools.push_back(pool_address.clone());
        client.register_session(&11, &leader, &pools, &true, &10, &10, &100_000);
        MockPoolClient::new(&env, &pool).configure_failure(&true);

        let source_event_id = BytesN::from_array(&env, &[11; 32]);
        client.record_leader_event(
            &source_event_id,
            &leader,
            &pool_address,
            &symbol_short!("deposit"),
            &Vec::from_array(&env, [10u128]),
            &10,
            &1,
        );
        assert!(client
            .try_execute_aquarius_standard_op(
                &11,
                &source_event_id,
                &pool_address,
                &symbol_short!("deposit"),
                &10,
                &Vec::from_array(&env, [10u128]),
                &0,
                &0,
                &Vec::new(&env),
                &Address::generate(&env),
            )
            .is_err());
        assert_eq!(client.session(&11).daily_used_quote, 0);

        MockPoolClient::new(&env, &pool).configure_failure(&false);
        client.execute_aquarius_standard_op(
            &11,
            &source_event_id,
            &pool_address,
            &symbol_short!("deposit"),
            &10,
            &Vec::from_array(&env, [10u128]),
            &0,
            &0,
            &Vec::new(&env),
            &Address::generate(&env),
        );
        assert_eq!(client.session(&11).daily_used_quote, 10);
    }

    #[test]
    fn proportional_floor_handles_u128_products_without_overflow() {
        let env = Env::default();
        let reserve = u128::MAX - 1;
        let shares = u128::MAX - 2;
        assert_eq!(proportional_floor(&env, reserve, shares, u128::MAX), u128::MAX - 3);
    }
}
