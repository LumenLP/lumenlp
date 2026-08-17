//! One-shot Aquarius pool snapshot cycle (RPC-first).

use {
    anyhow::Result,
    dex::{
        aquarius::{
            pool::{hydrate_pool, reserve_depth},
            pricing::price_book_from_pools,
            router::discover_pool_addresses,
        },
        db::Db,
        soroswap::{discover_mainnet_pool_addresses, hydrate_pool as hydrate_soroswap_pool},
        SorobanRpc, MAINNET_PASSPHRASE,
    },
    metrics::{fee_apr_24h, tvl_from_reserves},
    tracing::{info, warn},
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let rpc_url = std::env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8003".into());
    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "./data/lpagent.db".into());
    let top_n: usize = std::env::var("SNAPSHOT_TOP_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(400);

    let rpc = SorobanRpc::new(&rpc_url, MAINNET_PASSPHRASE);
    let db = Db::open(&db_path)?;

    info!(%rpc_url, %db_path, top_n, "snapshotter starting");

    let venues = std::env::var("SNAPSHOT_VENUES").unwrap_or_else(|_| "aquarius,soroswap".into());
    let mut pools = Vec::new();
    for venue in venues
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let discovered = match venue {
            "aquarius" => discover_pool_addresses(&rpc).await?,
            "soroswap" | "soroswap_amm" => discover_mainnet_pool_addresses(&rpc).await?,
            other => anyhow::bail!("unknown snapshot venue: {other}"),
        };
        info!(
            venue,
            discovered = discovered.len(),
            "venue pools discovered"
        );
        pools.extend(
            discovered
                .into_iter()
                .map(|address| (venue.to_string(), address)),
        );
    }
    pools.sort_by(|a, b| a.1.cmp(&b.1));
    pools.dedup_by(|a, b| a.1 == b.1);
    info!(discovered = pools.len(), "hydrating pools");

    let mut hydrated = Vec::new();
    for (i, (venue, addr)) in pools.iter().enumerate() {
        let state = match venue.as_str() {
            "aquarius" => hydrate_pool(&rpc, addr).await,
            "soroswap" | "soroswap_amm" => hydrate_soroswap_pool(&rpc, addr).await,
            _ => unreachable!("validated venue during discovery"),
        };
        match state {
            Ok(state) => {
                db.upsert_pool_with_venue(&state, venue)?;
                hydrated.push(state);
            }
            Err(e) => warn!(pool = %addr, error = %e, "hydrate failed"),
        }
        if (i + 1) % 20 == 0 {
            info!(done = i + 1, total = pools.len(), "hydrate progress");
        }
    }

    let book = price_book_from_pools(&hydrated);
    info!("XLM price book built from native/hop pools");

    hydrated.sort_by(|a, b| reserve_depth(b).cmp(&reserve_depth(a)));
    let take = hydrated.into_iter().take(top_n).collect::<Vec<_>>();
    info!(snapshotting = take.len(), "writing snapshots");

    for state in &take {
        let Some(prices) = book.required(&state.tokens) else {
            warn!(pool = %state.address, "skip snapshot — incomplete XLM prices");
            // Still store with TVL=0 so pool stays catalogued for positions scan.
            db.insert_snapshot(&state.address, 0.0, 0.0, 0.0, &state.reserves)?;
            continue;
        };
        let reserves_f: Vec<f64> = state.reserves.iter().map(|r| *r as f64).collect();
        let tvl = tvl_from_reserves(&reserves_f, &prices);

        let volume_24h = match db.previous_snapshot(&state.address)? {
            Some(prev) => {
                let prev_reserves: Vec<u128> =
                    serde_json::from_str(&prev.reserves_json).unwrap_or_default();
                let mut delta = 0.0f64;
                for (i, r) in state.reserves.iter().enumerate() {
                    let p = prices.get(i).copied().unwrap_or(0.0);
                    let old = prev_reserves.get(i).copied().unwrap_or(0);
                    delta += (*r as f64 - old as f64).abs() * p;
                }
                delta * 0.5
            }
            None => 0.0,
        };

        let est_apr = fee_apr_24h(state.fee_bps, volume_24h, tvl);
        db.insert_snapshot(&state.address, tvl, volume_24h, est_apr, &state.reserves)?;
        info!(
            pool = %state.address,
            pool_type = state.pool_type.as_str(),
            tvl,
            volume_24h,
            est_apr,
            "snapshot stored"
        );
    }

    info!("snapshotter done");
    Ok(())
}
