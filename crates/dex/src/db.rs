//! SQLite persistence for pool catalogue and snapshots.

use {
    crate::types::{PoolSnapshotRow, PoolType, SharePoolState},
    anyhow::{Context, Result},
    chrono::Utc,
    rusqlite::{params, Connection},
};

pub struct Db {
    conn: Connection,
}

pub struct DbStats {
    pub pool_count: usize,
    pub latest_snapshot_at: Option<String>,
}

impl Db {
    pub fn open(path: &str) -> Result<Self> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = open_sqlite_with_retry(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS pools (
              address TEXT PRIMARY KEY,
              pool_type TEXT NOT NULL,
              tokens_json TEXT NOT NULL,
              fee_bps INTEGER NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS pool_snapshots (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              pool_address TEXT NOT NULL,
              ts TEXT NOT NULL,
              tvl REAL NOT NULL,
              volume_24h REAL NOT NULL,
              est_apr REAL NOT NULL,
              reserves_json TEXT NOT NULL,
              UNIQUE(pool_address, ts)
            );
            CREATE INDEX IF NOT EXISTS idx_snapshots_pool_ts
              ON pool_snapshots(pool_address, ts);
            "#,
        )?;
        Ok(())
    }

    pub fn upsert_pool(&self, state: &SharePoolState) -> Result<()> {
        let tokens_json = serde_json::to_string(&state.tokens)?;
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            r#"
            INSERT INTO pools (address, pool_type, tokens_json, fee_bps, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(address) DO UPDATE SET
              pool_type=excluded.pool_type,
              tokens_json=excluded.tokens_json,
              fee_bps=excluded.fee_bps,
              updated_at=excluded.updated_at
            "#,
            params![
                state.address,
                state.pool_type.as_str(),
                tokens_json,
                state.fee_bps as i64,
                now
            ],
        )?;
        Ok(())
    }

    pub fn insert_snapshot(
        &self,
        pool_address: &str,
        tvl: f64,
        volume_24h: f64,
        est_apr: f64,
        reserves: &[u128],
    ) -> Result<()> {
        let ts = Utc::now().to_rfc3339();
        let reserves_json = serde_json::to_string(reserves)?;
        self.conn.execute(
            r#"
            INSERT INTO pool_snapshots
              (pool_address, ts, tvl, volume_24h, est_apr, reserves_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(pool_address, ts) DO UPDATE SET
              tvl=excluded.tvl,
              volume_24h=excluded.volume_24h,
              est_apr=excluded.est_apr,
              reserves_json=excluded.reserves_json
            "#,
            params![pool_address, ts, tvl, volume_24h, est_apr, reserves_json],
        )?;
        Ok(())
    }

    pub fn list_pool_addresses(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT address FROM pools ORDER BY address")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn stats(&self) -> Result<DbStats> {
        let pool_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM pools", [], |r| r.get(0))?;
        let latest_snapshot_at = self
            .conn
            .query_row("SELECT MAX(ts) FROM pool_snapshots", [], |r| r.get(0))
            .ok()
            .flatten();
        Ok(DbStats {
            pool_count: pool_count.max(0) as usize,
            latest_snapshot_at,
        })
    }

    pub fn latest_snapshots(&self) -> Result<Vec<PoolSnapshotRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT s.pool_address, s.ts, s.tvl, s.volume_24h, s.est_apr, s.reserves_json
            FROM pool_snapshots s
            INNER JOIN (
              SELECT pool_address, MAX(ts) AS max_ts
              FROM pool_snapshots
              GROUP BY pool_address
            ) t ON s.pool_address = t.pool_address AND s.ts = t.max_ts
            ORDER BY s.tvl DESC
            "#,
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(PoolSnapshotRow {
                pool_address: r.get(0)?,
                ts: r.get(1)?,
                tvl: r.get(2)?,
                volume_24h: r.get(3)?,
                est_apr: r.get(4)?,
                reserves_json: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn history(&self, pool_address: &str, limit: usize) -> Result<Vec<PoolSnapshotRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT pool_address, ts, tvl, volume_24h, est_apr, reserves_json
            FROM pool_snapshots
            WHERE pool_address = ?1
            ORDER BY ts DESC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![pool_address, limit as i64], |r| {
            Ok(PoolSnapshotRow {
                pool_address: r.get(0)?,
                ts: r.get(1)?,
                tvl: r.get(2)?,
                volume_24h: r.get(3)?,
                est_apr: r.get(4)?,
                reserves_json: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        out.reverse();
        Ok(out)
    }

    pub fn snapshots_since(&self, since_ts: &str) -> Result<Vec<PoolSnapshotRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT pool_address, ts, tvl, volume_24h, est_apr, reserves_json
            FROM pool_snapshots
            WHERE ts >= ?1
            ORDER BY pool_address ASC, ts ASC
            "#,
        )?;
        let rows = stmt.query_map(params![since_ts], |r| {
            Ok(PoolSnapshotRow {
                pool_address: r.get(0)?,
                ts: r.get(1)?,
                tvl: r.get(2)?,
                volume_24h: r.get(3)?,
                est_apr: r.get(4)?,
                reserves_json: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Previous snapshot for volume delta (any older row).
    pub fn previous_snapshot(&self, pool_address: &str) -> Result<Option<PoolSnapshotRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT pool_address, ts, tvl, volume_24h, est_apr, reserves_json
            FROM pool_snapshots
            WHERE pool_address = ?1
            ORDER BY ts DESC
            LIMIT 1
            "#,
        )?;
        let mut rows = stmt.query(params![pool_address])?;
        if let Some(r) = rows.next()? {
            Ok(Some(PoolSnapshotRow {
                pool_address: r.get(0)?,
                ts: r.get(1)?,
                tvl: r.get(2)?,
                volume_24h: r.get(3)?,
                est_apr: r.get(4)?,
                reserves_json: r.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn pool_meta(&self, address: &str) -> Result<Option<(String, String, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT pool_type, tokens_json, fee_bps FROM pools WHERE address = ?1")?;
        let mut rows = stmt.query(params![address])?;
        if let Some(r) = rows.next()? {
            Ok(Some((r.get(0)?, r.get(1)?, r.get(2)?)))
        } else {
            Ok(None)
        }
    }

    pub fn list_pools_with_latest(&self) -> Result<Vec<serde_json::Value>> {
        let snaps = self.latest_snapshots()?;
        let mut out = Vec::new();
        for s in snaps {
            let meta = self.pool_meta(&s.pool_address)?;
            let (pool_type, tokens_json, fee_bps) =
                meta.unwrap_or_else(|| ("unknown".into(), "[]".into(), 0));
            out.push(serde_json::json!({
                "address": s.pool_address,
                "pool_type": pool_type,
                "tokens": serde_json::from_str::<serde_json::Value>(&tokens_json).unwrap_or(serde_json::json!([])),
                "fee_bps": fee_bps,
                "tvl": s.tvl,
                "volume_24h": s.volume_24h,
                "est_apr": s.est_apr,
                "last_snapshot_at": s.ts,
            }));
        }
        Ok(out)
    }

    /// Pools + latest reserves for building an XLM price book.
    pub fn pool_states_for_pricing(&self) -> Result<Vec<SharePoolState>> {
        let snaps = self.latest_snapshots()?;
        let mut out = Vec::new();
        for s in snaps {
            let Some((pool_type, tokens_json, fee_bps)) = self.pool_meta(&s.pool_address)? else {
                continue;
            };
            let tokens: Vec<String> = serde_json::from_str(&tokens_json).unwrap_or_default();
            let reserves: Vec<u128> = serde_json::from_str(&s.reserves_json).unwrap_or_default();
            out.push(SharePoolState {
                address: s.pool_address,
                pool_type: PoolType::parse(&pool_type),
                tokens,
                reserves,
                fee_bps: fee_bps as u32,
                total_shares: 0,
                share_token: None,
                amp: None,
            });
        }
        Ok(out)
    }
}

fn open_sqlite_with_retry(path: &str) -> Result<Connection> {
    const ATTEMPTS: usize = 20;
    for attempt in 1..=ATTEMPTS {
        match Connection::open(path).with_context(|| format!("open sqlite {path}")) {
            Ok(conn) => {
                conn.busy_timeout(std::time::Duration::from_secs(15))?;
                match conn.execute_batch(
                    r#"
                    PRAGMA journal_mode=WAL;
                    PRAGMA synchronous=NORMAL;
                    PRAGMA foreign_keys=ON;
                    "#,
                ) {
                    Ok(_) => return Ok(conn),
                    Err(error)
                        if error.to_string().contains("database is locked")
                            && attempt < ATTEMPTS =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error)
                if error.to_string().contains("database is locked") && attempt < ATTEMPTS =>
            {
                std::thread::sleep(std::time::Duration::from_millis(500));
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    anyhow::bail!("sqlite open retry budget exhausted for {path}")
}
