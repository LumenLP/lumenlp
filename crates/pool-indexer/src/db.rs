use {
    crate::types::{PoolEvent, PoolEventKind, PoolRollup, PoolSnapshot5m, PoolSwap, WindowSpec, WINDOWS},
    anyhow::{Context, Result},
    chrono::{TimeZone, Utc},
    rusqlite::{params, Connection},
    serde_json::Value,
    std::collections::HashSet,
};

pub struct IndexDb {
    conn: Connection,
    // Snapshots are produced by the standalone snapshotter in the primary
    // database. Keep event/rollup writes isolated while reading that source.
    snapshot_conn: Option<Connection>,
}

#[derive(Debug, Clone)]
pub struct CachedPoolState {
    pub address: String,
    pub tokens: Vec<String>,
    pub reserves: Vec<u128>,
    pub fee_bps: u32,
}

pub struct IndexStats {
    pub cursor_ledger: Option<u32>,
    pub event_count: usize,
    pub distinct_event_pools: usize,
    pub swap_count: usize,
    pub snapshot_5m_count: usize,
    pub rollup_count: usize,
    pub distinct_rollup_pools: usize,
}

impl IndexDb {
    pub fn open_with_snapshot_path(path: &str, snapshot_path: Option<&str>) -> Result<Self> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = open_sqlite_with_retry(path)?;
        let snapshot_conn = snapshot_path
            .filter(|candidate| *candidate != path)
            .map(open_sqlite_with_retry)
            .transpose()?;
        let db = Self { conn, snapshot_conn };
        db.migrate()?;
        Ok(db)
    }

    fn snapshot_connection(&self) -> &Connection {
        self.snapshot_conn.as_ref().unwrap_or(&self.conn)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS indexer_cursor (
              id INTEGER PRIMARY KEY CHECK (id = 1),
              last_ledger INTEGER NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS pool_swaps (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              tx_hash TEXT NOT NULL,
              event_id TEXT UNIQUE NOT NULL,
              ledger INTEGER NOT NULL,
              created_at INTEGER NOT NULL,
              pool_address TEXT NOT NULL,
              dex TEXT NOT NULL,
              token_in TEXT,
              token_out TEXT,
              amount_in TEXT,
              amount_out TEXT,
              fee_bps INTEGER,
              volume_quote REAL,
              fee_quote REAL
            );
            CREATE INDEX IF NOT EXISTS idx_pool_swaps_pool_created
              ON pool_swaps(pool_address, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_pool_swaps_ledger
              ON pool_swaps(ledger);

            CREATE TABLE IF NOT EXISTS pool_events (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              event_id TEXT UNIQUE NOT NULL,
              tx_hash TEXT,
              ledger INTEGER NOT NULL,
              created_at INTEGER NOT NULL,
              pool_address TEXT NOT NULL,
              kind TEXT NOT NULL,
              body_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_pool_events_pool_created
              ON pool_events(pool_address, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_pool_events_kind_created
              ON pool_events(kind, created_at DESC);
            -- Leader rankings group lifecycle events by the actor embedded in
            -- the normalized event body. Keep this expression indexed so a
            -- cache miss does not repeatedly scan and parse the full event log.
            CREATE INDEX IF NOT EXISTS idx_pool_events_actor_kind_created
              ON pool_events(json_extract(body_json, '$.derived.actor'), kind, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_pool_events_ledger
              ON pool_events(ledger);

            CREATE TABLE IF NOT EXISTS pool_snapshots_5m (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              pool_address TEXT NOT NULL,
              bucket_ts INTEGER NOT NULL,
              tvl REAL NOT NULL,
              reserves_json TEXT NOT NULL,
              fee_bps INTEGER NOT NULL,
              UNIQUE(pool_address, bucket_ts)
            );
            CREATE INDEX IF NOT EXISTS idx_pool_snapshots_5m_pool_ts
              ON pool_snapshots_5m(pool_address, bucket_ts DESC);

            CREATE TABLE IF NOT EXISTS pool_rollups (
              pool_address TEXT NOT NULL,
              window TEXT NOT NULL,
              as_of_ts INTEGER NOT NULL,
              sample_count INTEGER NOT NULL,
              volume_quote REAL NOT NULL,
              fee_quote REAL NOT NULL,
              avg_tvl REAL NOT NULL,
              fee_tvl REAL NOT NULL,
              tx_count INTEGER NOT NULL,
              PRIMARY KEY (pool_address, window)
            );
            "#,
        )?;
        Ok(())
    }

    pub fn cached_pool_states(&self) -> Result<Vec<CachedPoolState>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.address, p.tokens_json, p.fee_bps, s.reserves_json
            FROM pools p
            LEFT JOIN (
              SELECT s1.pool_address, s1.reserves_json
              FROM pool_snapshots s1
              INNER JOIN (
                SELECT pool_address, MAX(ts) AS max_ts
                FROM pool_snapshots
                GROUP BY pool_address
              ) latest
                ON latest.pool_address = s1.pool_address AND latest.max_ts = s1.ts
            ) s ON s.pool_address = p.address
            ORDER BY p.address ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            let address: String = row.get(0)?;
            let tokens_json: String = row.get(1)?;
            let fee_bps: i64 = row.get(2)?;
            let reserves_json: Option<String> = row.get(3)?;
            Ok((address, tokens_json, fee_bps, reserves_json))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (address, tokens_json, fee_bps, reserves_json) = row?;
            let tokens: Vec<String> = serde_json::from_str(&tokens_json).unwrap_or_default();
            let reserves: Vec<u128> = reserves_json
                .as_deref()
                .and_then(|raw| serde_json::from_str(raw).ok())
                .unwrap_or_default();
            out.push(CachedPoolState {
                address,
                tokens,
                reserves,
                fee_bps: fee_bps.max(0) as u32,
            });
        }
        Ok(out)
    }

    pub fn insert_event(&self, event: &PoolEvent) -> Result<bool> {
        let inserted = self.conn.execute(
            r#"
            INSERT OR IGNORE INTO pool_events (
              event_id, tx_hash, ledger, created_at, pool_address, kind, body_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                event.event_id,
                event.tx_hash,
                event.ledger as i64,
                event.created_at,
                event.pool_address,
                event.kind.as_str(),
                event.body_json,
            ],
        )?;
        if inserted > 0 {
            return Ok(true);
        }
        // If a prior insert omitted derived.actor (RPC blip), patch when we now have
        // one.
        let Ok(body) = serde_json::from_str::<serde_json::Value>(&event.body_json) else {
            return Ok(false);
        };
        let Some(actor) = body
            .pointer("/derived/actor")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        else {
            return Ok(false);
        };
        let _ = actor;
        let updated = self.conn.execute(
            r#"
            UPDATE pool_events
            SET body_json = ?1
            WHERE event_id = ?2
              AND (
                json_extract(body_json, '$.derived.actor') IS NULL
                OR json_extract(body_json, '$.derived.actor') = ''
              )
            "#,
            params![event.body_json, event.event_id],
        )?;
        Ok(updated > 0)
    }

    pub fn list_liquidity_events_missing_actor(&self, limit: usize) -> Result<Vec<(String, String, String)>> {
        let limit = limit.max(1).min(200) as i64;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT event_id, tx_hash, body_json
            FROM pool_events
            WHERE kind IN ('deposit_liquidity', 'withdraw_liquidity')
              AND tx_hash IS NOT NULL
              AND tx_hash != ''
              AND (
                json_extract(body_json, '$.derived.actor') IS NULL
                OR json_extract(body_json, '$.derived.actor') = ''
              )
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn patch_event_actor(&self, event_id: &str, body_json: &str) -> Result<bool> {
        let updated = self.conn.execute(
            r#"
            UPDATE pool_events
            SET body_json = ?1
            WHERE event_id = ?2
              AND (
                json_extract(body_json, '$.derived.actor') IS NULL
                OR json_extract(body_json, '$.derived.actor') = ''
              )
            "#,
            params![body_json, event_id],
        )?;
        Ok(updated > 0)
    }

    /// Add stable venue labels to older trade rows written before event
    /// derivation became explicitly multi-DEX. This is local JSON migration:
    /// it does not rescan RPC history or move the indexer cursor.
    pub fn backfill_event_venue_labels(&self) -> Result<usize> {
        let mut stmt = self.conn.prepare(
            "SELECT event_id, body_json FROM pool_events WHERE kind = 'trade'",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut patches = Vec::new();
        for row in rows {
            let (event_id, body_json) = row?;
            let Ok(mut body) = serde_json::from_str::<Value>(&body_json) else {
                continue;
            };
            let Some(topic) = body.get("topic").and_then(Value::as_array) else {
                continue;
            };
            let symbol = |index: usize| {
                topic
                    .get(index)
                    .and_then(|value| value.get("value"))
                    .and_then(Value::as_str)
            };
            let (venue, pool_type) = if symbol(0) == Some("SoroswapPair") {
                ("soroswap_amm", Some("constant_product"))
            } else if symbol(0) == Some("POOL") && symbol(1) == Some("swap") {
                ("comet", Some("weighted"))
            } else if symbol(0) == Some("swap") {
                ("phoenix", None)
            } else {
                ("aquarius", None)
            };
            let Some(derived) = body.get_mut("derived").and_then(Value::as_object_mut) else {
                continue;
            };
            if derived.get("venue").and_then(Value::as_str).is_some() {
                continue;
            }
            derived.insert("venue".into(), Value::String(venue.into()));
            if let Some(pool_type) = pool_type {
                derived.insert("pool_type".into(), Value::String(pool_type.into()));
            }
            patches.push((event_id, serde_json::to_string(&body)?));
        }
        drop(stmt);
        let mut patched = 0;
        for (event_id, body_json) in patches {
            patched += self.conn.execute(
                "UPDATE pool_events SET body_json = ?1 WHERE event_id = ?2",
                params![body_json, event_id],
            )?;
        }
        Ok(patched)
    }

    pub fn cursor_ledger(&self) -> Result<Option<u32>> {
        let mut stmt = self
            .conn
            .prepare("SELECT last_ledger FROM indexer_cursor WHERE id = 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let ledger: i64 = row.get(0)?;
            Ok(Some(ledger as u32))
        } else {
            Ok(None)
        }
    }

    pub fn set_cursor_ledger(&self, ledger: u32) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO indexer_cursor (id, last_ledger, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET last_ledger = excluded.last_ledger, updated_at = excluded.updated_at",
            params![ledger, now],
        )?;
        Ok(())
    }

    pub fn insert_swap(&self, swap: &PoolSwap) -> Result<bool> {
        let inserted = self.conn.execute(
            r#"
            INSERT OR IGNORE INTO pool_swaps (
              tx_hash, event_id, ledger, created_at, pool_address, dex,
              token_in, token_out, amount_in, amount_out, fee_bps, volume_quote, fee_quote
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                swap.tx_hash,
                swap.event_id,
                swap.ledger as i64,
                swap.created_at,
                swap.pool_address,
                swap.dex,
                swap.token_in,
                swap.token_out,
                swap.amount_in,
                swap.amount_out,
                swap.fee_bps.map(i64::from),
                swap.volume_quote,
                swap.fee_quote,
            ],
        )?;
        Ok(inserted > 0)
    }

    pub fn bucket_from_rfc3339(ts: &str) -> Option<i64> {
        let dt = chrono::DateTime::parse_from_rfc3339(ts).ok()?;
        let secs = dt.timestamp();
        Some(secs - secs.rem_euclid(5 * 60))
    }

    pub fn backfill_5m_from_hourly_snapshots(&self) -> Result<usize> {
        // Re-read the current bucket so a snapshot written during the same
        // five-minute interval replaces its earlier value, but avoid scanning
        // the full primary snapshot history on every 30-second indexer poll.
        let latest_bucket = self
            .conn
            .query_row("SELECT MAX(bucket_ts) FROM pool_snapshots_5m", [], |row| {
                row.get::<_, Option<i64>>(0)
            })?;
        let since = latest_bucket.map(bucket_to_rfc3339);
        let source = self.snapshot_connection();
        let mut stmt = source.prepare(
            r#"
            SELECT s.pool_address, s.ts, s.tvl, s.reserves_json, p.fee_bps
            FROM pool_snapshots s
            LEFT JOIN pools p ON p.address = s.pool_address
            WHERE ?1 IS NULL OR s.ts >= ?1
            ORDER BY s.pool_address ASC, s.ts ASC
            "#,
        )?;
        let rows = stmt.query_map(params![since], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<i64>>(4)?.unwrap_or(0) as u32,
            ))
        })?;

        let mut inserted = 0usize;
        for row in rows {
            let (pool_address, ts, tvl, reserves_json, fee_bps) = row?;
            let Some(bucket_ts) = Self::bucket_from_rfc3339(&ts) else {
                continue;
            };
            let changed = self.conn.execute(
                r#"
                INSERT INTO pool_snapshots_5m (pool_address, bucket_ts, tvl, reserves_json, fee_bps)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(pool_address, bucket_ts) DO UPDATE SET
                  tvl = excluded.tvl,
                  reserves_json = excluded.reserves_json,
                  fee_bps = excluded.fee_bps
                "#,
                params![pool_address, bucket_ts, tvl, reserves_json, fee_bps as i64],
            )?;
            inserted += changed as usize;
        }
        Ok(inserted)
    }

    pub fn list_rollup_pool_addresses(&self) -> Result<Vec<String>> {
        let mut pools = HashSet::new();

        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT pool_address FROM pool_snapshots_5m")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            pools.insert(row?);
        }

        let mut stmt = self.conn.prepare("SELECT DISTINCT pool_address FROM pool_swaps")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            pools.insert(row?);
        }

        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT pool_address FROM pool_events WHERE kind IN (?1, ?2, ?3, ?4)")?;
        let rows = stmt.query_map(
            params![
                PoolEventKind::UpdateReserves.as_str(),
                PoolEventKind::DepositLiquidity.as_str(),
                PoolEventKind::WithdrawLiquidity.as_str(),
                PoolEventKind::ReservesSync.as_str(),
            ],
            |row| row.get::<_, String>(0),
        )?;
        for row in rows {
            pools.insert(row?);
        }

        let mut out: Vec<String> = pools.into_iter().collect();
        out.sort();
        Ok(out)
    }

    pub fn latest_bucket_ts(&self, pool_address: &str) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT MAX(bucket_ts) FROM pool_snapshots_5m WHERE pool_address = ?1",
                params![pool_address],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn load_snapshots_for_window(&self, pool_address: &str, since_ts: i64) -> Result<Vec<PoolSnapshot5m>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT pool_address, bucket_ts, tvl, reserves_json, fee_bps
            FROM pool_snapshots_5m
            WHERE pool_address = ?1 AND bucket_ts >= ?2
            ORDER BY bucket_ts ASC
            "#,
        )?;
        let rows = stmt.query_map(params![pool_address, since_ts], |row| {
            Ok(PoolSnapshot5m {
                pool_address: row.get(0)?,
                bucket_ts: row.get(1)?,
                tvl: row.get(2)?,
                reserves_json: row.get(3)?,
                fee_bps: row.get::<_, i64>(4)? as u32,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn load_swaps_for_window(&self, pool_address: &str, since_ts: i64) -> Result<Vec<PoolSwap>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT tx_hash, event_id, ledger, created_at, pool_address, dex,
                   token_in, token_out, amount_in, amount_out, fee_bps, volume_quote, fee_quote
            FROM pool_swaps
            WHERE pool_address = ?1 AND created_at >= ?2
            ORDER BY created_at ASC, id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![pool_address, since_ts], |row| {
            Ok(PoolSwap {
                tx_hash: row.get(0)?,
                event_id: row.get(1)?,
                ledger: row.get::<_, i64>(2)? as u32,
                created_at: row.get(3)?,
                pool_address: row.get(4)?,
                dex: row.get(5)?,
                token_in: row.get(6)?,
                token_out: row.get(7)?,
                amount_in: row.get(8)?,
                amount_out: row.get(9)?,
                fee_bps: row.get::<_, Option<i64>>(10)?.map(|v| v as u32),
                volume_quote: row.get(11)?,
                fee_quote: row.get(12)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn upsert_rollup(&self, rollup: &PoolRollup) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO pool_rollups (
              pool_address, window, as_of_ts, sample_count, volume_quote,
              fee_quote, avg_tvl, fee_tvl, tx_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(pool_address, window) DO UPDATE SET
              as_of_ts = excluded.as_of_ts,
              sample_count = excluded.sample_count,
              volume_quote = excluded.volume_quote,
              fee_quote = excluded.fee_quote,
              avg_tvl = excluded.avg_tvl,
              fee_tvl = excluded.fee_tvl,
              tx_count = excluded.tx_count
            "#,
            params![
                rollup.pool_address,
                rollup.window,
                rollup.as_of_ts,
                rollup.sample_count as i64,
                rollup.volume_quote,
                rollup.fee_quote,
                rollup.avg_tvl,
                rollup.fee_tvl,
                rollup.tx_count as i64,
            ],
        )?;
        Ok(())
    }

    pub fn rebuild_rollups(&self) -> Result<usize> {
        let pools = self.list_rollup_pool_addresses()?;
        let mut count = 0usize;
        for pool_address in pools {
            let Some(as_of_ts) = self.latest_bucket_ts(&pool_address)? else {
                continue;
            };
            for window in WINDOWS {
                let rollup = self.compute_rollup(&pool_address, as_of_ts, window)?;
                self.upsert_rollup(&rollup)?;
                count += 1;
            }
        }
        Ok(count)
    }

    fn compute_rollup(&self, pool_address: &str, as_of_ts: i64, window: WindowSpec) -> Result<PoolRollup> {
        let since_ts = as_of_ts - window.seconds;
        let snapshots = self.load_snapshots_for_window(pool_address, since_ts)?;
        let swaps = self.load_swaps_for_window(pool_address, since_ts)?;

        let sample_count = snapshots.len();
        let avg_tvl = if sample_count > 0 {
            snapshots.iter().map(|s| s.tvl).sum::<f64>() / sample_count as f64
        } else {
            0.0
        };
        let volume_quote = swaps.iter().map(|s| s.volume_quote.unwrap_or(0.0)).sum::<f64>();
        let fee_quote = swaps.iter().map(|s| s.fee_quote.unwrap_or(0.0)).sum::<f64>();
        let fee_tvl = if avg_tvl > 0.0 { fee_quote / avg_tvl } else { 0.0 };

        Ok(PoolRollup {
            pool_address: pool_address.to_string(),
            window: window.label.to_string(),
            as_of_ts,
            sample_count,
            volume_quote,
            fee_quote,
            avg_tvl,
            fee_tvl,
            tx_count: swaps.len(),
        })
    }

    pub fn stats(&self) -> Result<IndexStats> {
        let cursor_ledger = self.cursor_ledger()?;
        let event_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM pool_events", [], |r| r.get(0))?;
        let distinct_event_pools: i64 =
            self.conn
                .query_row("SELECT COUNT(DISTINCT pool_address) FROM pool_events", [], |r| r.get(0))?;
        let swap_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM pool_swaps", [], |r| r.get(0))?;
        let snapshot_5m_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM pool_snapshots_5m", [], |r| r.get(0))?;
        let rollup_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM pool_rollups", [], |r| r.get(0))?;
        let distinct_rollup_pools: i64 =
            self.conn
                .query_row("SELECT COUNT(DISTINCT pool_address) FROM pool_rollups", [], |r| {
                    r.get(0)
                })?;
        Ok(IndexStats {
            cursor_ledger,
            event_count: event_count.max(0) as usize,
            distinct_event_pools: distinct_event_pools.max(0) as usize,
            swap_count: swap_count.max(0) as usize,
            snapshot_5m_count: snapshot_5m_count.max(0) as usize,
            rollup_count: rollup_count.max(0) as usize,
            distinct_rollup_pools: distinct_rollup_pools.max(0) as usize,
        })
    }
}

fn open_sqlite_with_retry(path: &str) -> Result<Connection> {
    const ATTEMPTS: usize = 20;
    for attempt in 1..=ATTEMPTS {
        match Connection::open(path).with_context(|| format!("open sqlite {path}")) {
            Ok(conn) => {
                conn.busy_timeout(std::time::Duration::from_secs(60))?;
                match conn.execute_batch(
                    r#"
                    PRAGMA journal_mode=WAL;
                    PRAGMA synchronous=NORMAL;
                    PRAGMA foreign_keys=ON;
                    "#,
                ) {
                    Ok(_) => return Ok(conn),
                    Err(error) if error.to_string().contains("database is locked") && attempt < ATTEMPTS => {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) if error.to_string().contains("database is locked") && attempt < ATTEMPTS => {
                std::thread::sleep(std::time::Duration::from_millis(500));
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    anyhow::bail!("sqlite open retry budget exhausted for {path}")
}

pub fn bucket_to_rfc3339(bucket_ts: i64) -> String {
    Utc.timestamp_opt(bucket_ts, 0)
        .single()
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| bucket_ts.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_rounds_down_to_five_minutes() {
        let ts = "2026-07-26T10:07:49Z";
        let bucket = IndexDb::bucket_from_rfc3339(ts).unwrap();
        assert_eq!(bucket_to_rfc3339(bucket), "2026-07-26T10:05:00+00:00");
    }

    #[test]
    fn historical_trade_venue_labels_are_idempotent() {
        let db = IndexDb::open_with_snapshot_path(":memory:", None).unwrap();
        db.insert_event(&PoolEvent {
            event_id: "legacy-soroswap-trade".into(),
            tx_hash: Some("tx".into()),
            ledger: 1,
            created_at: 1,
            pool_address: "CPool".into(),
            kind: PoolEventKind::Trade,
            body_json: r#"{"topic":[{"value":"SoroswapPair"}],"derived":{"amount_in":"10"}}"#.into(),
        }).unwrap();

        assert_eq!(db.backfill_event_venue_labels().unwrap(), 1);
        assert_eq!(db.backfill_event_venue_labels().unwrap(), 0);
        let body: String = db.conn.query_row(
            "SELECT body_json FROM pool_events WHERE event_id = 'legacy-soroswap-trade'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert!(body.contains("\"venue\":\"soroswap_amm\""));
        assert!(body.contains("\"pool_type\":\"constant_product\""));
    }
}
