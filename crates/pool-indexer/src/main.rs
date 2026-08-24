mod db;
mod events;
mod rollups;
mod types;

use {
    crate::{
        db::IndexDb,
        events::{backfill_missing_actors, EventScanResult, PoolEventScanner},
        rollups::sync_derived_tables,
    },
    anyhow::Result,
    dex::{SorobanRpc, MAINNET_PASSPHRASE},
    std::time::{Duration, Instant},
    tracing::{info, warn},
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let cmd = args.first().map(String::as_str).unwrap_or("run");

    let rpc_url = std::env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8003".into());
    let db_path = std::env::var("INDEXER_DB_PATH").unwrap_or_else(|_| "./data/pool-indexer.db".into());
    let snapshot_db_path = std::env::var("SNAPSHOT_DATABASE_PATH").ok();
    let poll_secs: u64 = std::env::var("INDEXER_POLL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    let rpc = SorobanRpc::new(&rpc_url, MAINNET_PASSPHRASE);
    let db = IndexDb::open_with_snapshot_path(&db_path, snapshot_db_path.as_deref())?;

    match cmd {
        "run" => run(&rpc, &db, poll_secs).await,
        "backfill" => backfill(&rpc, &db).await,
        "status" => status(&db),
        other => anyhow::bail!("unknown command: {other} (expected run|backfill|status)"),
    }
}

async fn run(rpc: &SorobanRpc, db: &IndexDb, poll_secs: u64) -> Result<()> {
    const DISCOVERY_REFRESH_SECS: u64 = 300;
    const DISCOVERY_LOOKBACK_LEDGERS: u32 = 360; // ~30 minutes at ~5s/ledger
    sync_derived_tables(db)?;
    let mut scanner = PoolEventScanner::discover(rpc, db).await?;
    let mut last_discovery = Instant::now();
    let health = rpc.get_health().await?;
    let mut cursor = match db.cursor_ledger()? {
        Some(saved) => saved,
        None => {
            let start = health.latest_ledger.saturating_sub(1);
            db.set_cursor_ledger(start)?;
            start
        }
    };
    if cursor < health.oldest_ledger {
        warn!(
            saved_cursor = cursor,
            oldest_ledger = health.oldest_ledger,
            latest_ledger = health.latest_ledger,
            retention_window = health.ledger_retention_window,
            "saved cursor fell out of RPC retention window; clamping to oldest available ledger"
        );
        cursor = health.oldest_ledger;
        db.set_cursor_ledger(cursor)?;
    }

    info!(cursor, poll_secs, "pool-indexer started");
    loop {
        if last_discovery.elapsed() >= Duration::from_secs(DISCOVERY_REFRESH_SECS) {
            match PoolEventScanner::discover(rpc, db).await {
                Ok(refreshed) => {
                    // A newly discovered pool may have emitted events after the
                    // main cursor passed it. Re-scan a bounded overlap; event
                    // and swap ids make this safe and idempotent.
                    let lookback_start = cursor.saturating_sub(DISCOVERY_LOOKBACK_LEDGERS);
                    if lookback_start < cursor {
                        let overlap = refreshed.scan(rpc, lookback_start + 1, cursor).await?;
                        persist_scan(db, &overlap)?;
                        sync_derived_tables(db)?;
                        info!(
                            start_ledger = lookback_start + 1,
                            end_ledger = cursor,
                            events = overlap.events.len(),
                            swaps = overlap.swaps.len(),
                            "replayed discovery overlap"
                        );
                    }
                    scanner = refreshed;
                    last_discovery = Instant::now();
                    info!("pool discovery refreshed while indexer was running");
                }
                Err(error) => {
                    warn!(%error, "pool discovery refresh failed; keeping existing pool set");
                    last_discovery = Instant::now();
                }
            }
        }
        let latest = rpc.get_latest_ledger().await?.sequence;
        if latest > cursor {
            // Chunk catch-up so a large lag cannot starve pagination / RPC timeouts.
            const MAX_LEDGER_SPAN: u32 = 360; // ~30 minutes at ~5s/ledger
            let mut from = cursor + 1;
            while from <= latest {
                let to = (from + MAX_LEDGER_SPAN - 1).min(latest);
                let scan = scanner.scan(rpc, from, to).await?;
                persist_scan(db, &scan)?;
                db.set_cursor_ledger(scan.latest_ledger)?;
                sync_derived_tables(db)?;
                cursor = scan.latest_ledger;
                info!(
                    cursor,
                    known_pools = scan.known_pools,
                    raw_events = scan.raw_event_count,
                    events = scan.events.len(),
                    swaps = scan.swaps.len(),
                    "pool-indexer poll applied"
                );
                from = cursor + 1;
            }
        }
        if let Err(error) = backfill_missing_actors(rpc, db, 25).await {
            warn!(%error, "actor backfill failed");
        }
        tokio::time::sleep(std::time::Duration::from_secs(poll_secs)).await;
    }
}

fn persist_scan(db: &IndexDb, scan: &EventScanResult) -> Result<()> {
    for event in &scan.events {
        let _ = db.insert_event(event)?;
    }
    for swap in &scan.swaps {
        let _ = db.insert_swap(swap)?;
    }
    Ok(())
}

async fn backfill(rpc: &SorobanRpc, db: &IndexDb) -> Result<()> {
    sync_derived_tables(db)?;
    let scanner = PoolEventScanner::discover(rpc, db).await?;
    let health = rpc.get_health().await?;
    let requested_start = std::env::var("INDEXER_START_LEDGER")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(health.latest_ledger.saturating_sub(17_280));
    let start = requested_start.max(health.oldest_ledger);
    if requested_start < health.oldest_ledger {
        warn!(
            requested_start,
            oldest_ledger = health.oldest_ledger,
            latest_ledger = health.latest_ledger,
            retention_window = health.ledger_retention_window,
            "requested backfill start is older than RPC retention window; clamping to oldest available ledger"
        );
    }
    let scan_start = start;
    let latest = health.latest_ledger;
    let mut from = scan_start;
    let mut total_raw = 0usize;
    let mut total_events = 0usize;
    let mut total_swaps = 0usize;
    const MAX_LEDGER_SPAN: u32 = 360;
    while from <= latest {
        let to = (from + MAX_LEDGER_SPAN - 1).min(latest);
        let scan = scanner.scan(rpc, from, to).await?;
        for event in &scan.events {
            let _ = db.insert_event(event)?;
        }
        for swap in &scan.swaps {
            let _ = db.insert_swap(swap)?;
        }
        db.set_cursor_ledger(scan.latest_ledger)?;
        total_raw += scan.raw_event_count;
        total_events += scan.events.len();
        total_swaps += scan.swaps.len();
        from = scan.latest_ledger + 1;
        info!(
            cursor = scan.latest_ledger,
            raw_events = scan.raw_event_count,
            events = scan.events.len(),
            swaps = scan.swaps.len(),
            "pool-indexer backfill chunk"
        );
    }
    sync_derived_tables(db)?;
    info!(
        start = scan_start,
        latest,
        known_pools = 0,
        raw_events = total_raw,
        events = total_events,
        swaps = total_swaps,
        "pool-indexer backfill completed"
    );
    Ok(())
}

fn status(db: &IndexDb) -> Result<()> {
    let stats = db.stats()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "cursor_ledger": stats.cursor_ledger,
            "event_count": stats.event_count,
            "distinct_event_pools": stats.distinct_event_pools,
            "swap_count": stats.swap_count,
            "snapshot_5m_count": stats.snapshot_5m_count,
            "rollup_count": stats.rollup_count,
            "distinct_rollup_pools": stats.distinct_rollup_pools,
        }))?
    );
    Ok(())
}
