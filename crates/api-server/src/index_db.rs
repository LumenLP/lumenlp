use {
    crate::recorder::RecorderEvent,
    anyhow::{Context, Result},
    rusqlite::{params, Connection, OptionalExtension},
    serde_json::Value,
    std::collections::{HashMap, HashSet},
};

#[derive(Debug, Clone)]
pub struct PoolRollupRow {
    pub pool_address: String,
    pub window: String,
    pub as_of_ts: i64,
    pub sample_count: usize,
    pub volume_quote: f64,
    pub fee_quote: f64,
    pub avg_tvl: f64,
    pub fee_tvl: f64,
    pub tx_count: usize,
}

#[derive(Debug, Clone)]
pub struct PoolActivityRow {
    pub pool_address: String,
    pub first_event_at: Option<i64>,
    pub last_event_at: Option<i64>,
    pub event_count: usize,
    pub swap_count: usize,
}

#[derive(Debug, Clone)]
pub struct TokenMetadataRow {
    pub address: String,
    pub symbol: String,
    pub name: Option<String>,
    pub issuer: Option<String>,
    pub domain: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PoolEventRow {
    pub event_id: String,
    pub tx_hash: Option<String>,
    pub ledger: u32,
    pub created_at: i64,
    pub pool_address: String,
    pub kind: String,
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CopySessionRow {
    pub id: String,
    pub contract_session_id: Option<u32>,
    pub follower_address: String,
    pub leader_address: String,
    pub coefficient: f64,
    pub status: String,
    pub include_claims: bool,
    pub allowed_pools: Vec<String>,
    pub max_per_op_quote_xlm: f64,
    pub max_daily_quote_xlm: f64,
    pub expires_at: Option<i64>,
    pub cursor_ts: i64,
    pub watermark_ts: i64,
    pub watermark_event_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CopyOpRow {
    pub id: String,
    pub session_id: String,
    pub source_event_id: String,
    pub pool_address: String,
    pub kind: String,
    pub position_key: String,
    pub leader_amounts_json: String,
    pub scaled_amounts_json: String,
    pub leader_quote_xlm: Option<f64>,
    pub scaled_quote_xlm: Option<f64>,
    pub status: String,
    pub note: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct RecorderOutboxRow {
    pub source_event_id: String,
    pub leader_address: String,
    pub pool_address: String,
    pub kind: String,
    pub amounts: Vec<u128>,
    pub quote_stroops: i128,
    pub ledger: u32,
    pub status: String,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecorderOutboxStatus {
    pub pending: usize,
    pub submitted: usize,
    pub failed: usize,
    pub oldest_pending_at: Option<i64>,
}

fn copy_status_transition_allowed(current: &str, next: &str) -> bool {
    if current == next {
        return true;
    }
    match current {
        "pending" => matches!(next, "drafted" | "skipped" | "failed" | "insufficient" | "rejected"),
        "drafted" => matches!(next, "signed" | "skipped" | "failed"),
        "insufficient" => matches!(next, "drafted" | "skipped" | "failed"),
        "signed" => next == "failed",
        "skipped" | "failed" | "rejected" => false,
        _ => false,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActorLiquidityActivity {
    pub since_ts: i64,
    pub event_count: usize,
    pub deposit_count: usize,
    pub withdraw_count: usize,
    pub claim_count: usize,
    pub deposit_quote_xlm: f64,
    pub withdraw_quote_xlm: f64,
    pub claim_quote_xlm: f64,
    pub distinct_pools: usize,
    pub last_activity_at: Option<i64>,
}

/// Aggregated actor row for Copy-leader scouting boards.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TopLiquidityActor {
    pub address: String,
    pub event_count: usize,
    pub deposit_count: usize,
    pub withdraw_count: usize,
    pub claim_count: usize,
    pub deposit_quote_xlm: f64,
    pub withdraw_quote_xlm: f64,
    pub claim_quote_xlm: f64,
    pub distinct_pools: usize,
    pub last_activity_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActorFeeSnapshotTotal {
    pub unclaimed_quote_xlm: f64,
    pub position_value_quote_xlm: f64,
    pub position_count: usize,
    pub pool_count: usize,
    pub observed_at: Option<i64>,
}

/// Lifetime (indexed) liquidity aggregates for an actor — used for avg monthly
/// claimed fees proxy.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActorLifetimeTotals {
    pub deposit_count: usize,
    pub withdraw_count: usize,
    pub claim_count: usize,
    pub deposit_quote_xlm: f64,
    pub withdraw_quote_xlm: f64,
    pub claim_quote_xlm: f64,
    pub distinct_pools: usize,
    pub first_activity_at: Option<i64>,
    pub last_activity_at: Option<i64>,
}

/// A concentrated-liquidity range observed for an actor in indexed events.
/// The range is a candidate for an on-chain `get_position` verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionRangeCandidate {
    pub pool_address: String,
    pub tick_lower: i32,
    pub tick_upper: i32,
}

#[derive(Debug, Clone)]
pub struct PoolActivitySummaryRow {
    pub event_count_24h: usize,
    pub swap_count_24h: usize,
    pub volume_quote_24h: f64,
    pub fee_quote_24h: f64,
    pub deposit_quote_24h: f64,
    pub withdraw_quote_24h: f64,
    pub net_liquidity_delta_quote_24h: f64,
    pub claim_quote_24h: f64,
    pub avg_update_interval_secs_24h: Option<f64>,
    pub latest_update_at_24h: Option<i64>,
    pub deposit_count_24h: usize,
    pub withdraw_count_24h: usize,
    pub claim_count_24h: usize,
    pub update_count_24h: usize,
}

pub struct IndexDb {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct IndexerStatus {
    pub cursor_ledger: Option<u32>,
    pub event_count: usize,
    pub swap_count: usize,
    pub rollup_count: usize,
    pub distinct_event_pools: usize,
    pub distinct_rollup_pools: usize,
    pub last_event_at: Option<i64>,
    pub last_rollup_at: Option<i64>,
}

impl IndexDb {
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
            CREATE TABLE IF NOT EXISTS copy_sessions (
              id TEXT PRIMARY KEY,
              contract_session_id INTEGER,
              follower_address TEXT NOT NULL,
              leader_address TEXT NOT NULL,
              coefficient REAL NOT NULL,
              status TEXT NOT NULL,
              include_claims INTEGER NOT NULL DEFAULT 0,
              allowed_pools_json TEXT NOT NULL DEFAULT '[]',
              max_per_op_quote_xlm REAL NOT NULL DEFAULT 0,
              max_daily_quote_xlm REAL NOT NULL DEFAULT 0,
              expires_at INTEGER,
              cursor_ts INTEGER NOT NULL,
              watermark_ts INTEGER NOT NULL DEFAULT 0,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_copy_sessions_follower ON copy_sessions(follower_address);

            CREATE TABLE IF NOT EXISTS copy_ops (
              id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL,
              source_event_id TEXT NOT NULL,
              pool_address TEXT NOT NULL,
              kind TEXT NOT NULL,
              position_key TEXT NOT NULL,
              leader_amounts_json TEXT NOT NULL,
              scaled_amounts_json TEXT NOT NULL,
              leader_quote_xlm REAL,
              scaled_quote_xlm REAL,
              status TEXT NOT NULL,
              note TEXT,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL,
              UNIQUE(session_id, source_event_id)
            );
            CREATE INDEX IF NOT EXISTS idx_copy_ops_session ON copy_ops(session_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS recorder_outbox (
              source_event_id TEXT PRIMARY KEY,
              leader_address TEXT NOT NULL,
              pool_address TEXT NOT NULL,
              kind TEXT NOT NULL,
              amounts_json TEXT NOT NULL,
              quote_stroops TEXT NOT NULL,
              ledger INTEGER NOT NULL,
              status TEXT NOT NULL,
              attempts INTEGER NOT NULL DEFAULT 0,
              last_error TEXT,
              created_at INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_recorder_outbox_status
              ON recorder_outbox(status, created_at ASC);

            CREATE TABLE IF NOT EXISTS token_metadata (
              address TEXT PRIMARY KEY,
              symbol TEXT NOT NULL,
              name TEXT,
              issuer TEXT,
              domain TEXT,
              icon TEXT,
              updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS actor_fee_snapshots (
              actor TEXT NOT NULL,
              pool_address TEXT NOT NULL,
              venue TEXT NOT NULL,
              unclaimed_quote_xlm REAL,
              position_value_quote_xlm REAL,
              status TEXT NOT NULL,
              observed_at INTEGER NOT NULL,
              PRIMARY KEY (actor, pool_address)
            );
            CREATE INDEX IF NOT EXISTS idx_actor_fee_snapshots_actor
              ON actor_fee_snapshots(actor, observed_at DESC);

            CREATE TABLE IF NOT EXISTS actor_fee_snapshot_history (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              actor TEXT NOT NULL,
              pool_address TEXT NOT NULL,
              venue TEXT NOT NULL,
              unclaimed_quote_xlm REAL,
              position_value_quote_xlm REAL,
              status TEXT NOT NULL,
              observed_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_actor_fee_history_actor_pool_time
              ON actor_fee_snapshot_history(actor, pool_address, observed_at DESC, id DESC);
            "#,
        )?;
        // pool_events is created by the indexer component. API startup can
        // race that initialization, so add the read-optimized index only
        // after the shared table exists.
        if self.has_table("pool_events") {
            self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_pool_events_lp_actor_time ON pool_events(json_extract(body_json, '$.derived.actor'), created_at DESC, kind, pool_address)",
                [],
            )?;
            // Leader boards group lifecycle events by actor and pool before
            // applying the requested time window. Keep this narrower index
            // separate from the general actor-time index used by event feeds.
            self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_pool_events_lp_actor_pool_time
                 ON pool_events(json_extract(body_json, '$.derived.actor'), pool_address, created_at DESC, kind)
                 WHERE kind IN ('deposit_liquidity', 'withdraw_liquidity', 'claim_fees', 'claim_protocol_fee')",
                [],
            )?;
        }
        self.ensure_column("copy_sessions", "watermark_event_id", "TEXT NOT NULL DEFAULT ''")?;
        self.ensure_column("copy_sessions", "contract_session_id", "INTEGER")?;
        self.ensure_column("copy_sessions", "allowed_pools_json", "TEXT NOT NULL DEFAULT '[]'")?;
        self.ensure_column("copy_sessions", "max_per_op_quote_xlm", "REAL NOT NULL DEFAULT 0")?;
        self.ensure_column("copy_sessions", "max_daily_quote_xlm", "REAL NOT NULL DEFAULT 0")?;
        self.ensure_column("copy_sessions", "expires_at", "INTEGER")?;
        Ok(())
    }

    fn ensure_column(&self, table: &str, column: &str, decl: &str) -> Result<()> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column {
                return Ok(());
            }
        }
        self.conn
            .execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"), [])?;
        Ok(())
    }

    fn has_table(&self, name: &str) -> bool {
        self.conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                params![name],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value != 0)
            .unwrap_or(false)
    }

    pub fn create_copy_session(
        &self,
        follower_address: &str,
        leader_address: &str,
        coefficient: f64,
        include_claims: bool,
        allowed_pools: &[String],
        max_per_op_quote_xlm: f64,
        max_daily_quote_xlm: f64,
        expires_at: Option<i64>,
        contract_session_id: Option<u32>,
    ) -> Result<CopySessionRow> {
        self.pause_active_sessions_for_pair(follower_address, leader_address)?;
        let now = chrono::Utc::now().timestamp();
        let row = CopySessionRow {
            id: new_copy_id(),
            contract_session_id,
            follower_address: follower_address.to_string(),
            leader_address: leader_address.to_string(),
            coefficient,
            status: "active".to_string(),
            include_claims,
            allowed_pools: allowed_pools.to_vec(),
            max_per_op_quote_xlm,
            max_daily_quote_xlm,
            expires_at,
            cursor_ts: now,
            watermark_ts: now,
            watermark_event_id: String::new(),
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            r#"
            INSERT INTO copy_sessions (
              id, contract_session_id, follower_address, leader_address, coefficient, status,
              include_claims, cursor_ts, watermark_ts, watermark_event_id,
              allowed_pools_json, max_per_op_quote_xlm, max_daily_quote_xlm, expires_at,
              created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
            params![
                row.id,
                row.contract_session_id,
                row.follower_address,
                row.leader_address,
                row.coefficient,
                row.status,
                row.include_claims as i64,
                row.cursor_ts,
                row.watermark_ts,
                row.watermark_event_id,
                serde_json::to_string(&row.allowed_pools)?,
                row.max_per_op_quote_xlm,
                row.max_daily_quote_xlm,
                row.expires_at,
                row.created_at,
                row.updated_at,
            ],
        )?;
        Ok(row)
    }

    pub fn list_copy_sessions(&self, follower: &str) -> Result<Vec<CopySessionRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, contract_session_id, follower_address, leader_address, coefficient, status,
                   include_claims, allowed_pools_json, max_per_op_quote_xlm,
                   max_daily_quote_xlm, expires_at, cursor_ts, watermark_ts,
                   watermark_event_id, created_at, updated_at
            FROM copy_sessions
            WHERE follower_address = ?1
            ORDER BY created_at DESC
            "#,
        )?;
        let rows = stmt.query_map(params![follower], map_copy_session_row)?;
        collect_copy_session_rows(rows)
    }

    pub fn active_copy_sessions(&self) -> Result<Vec<CopySessionRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, contract_session_id, follower_address, leader_address, coefficient, status,
                   include_claims, allowed_pools_json, max_per_op_quote_xlm,
                   max_daily_quote_xlm, expires_at, cursor_ts, watermark_ts,
                   watermark_event_id, created_at, updated_at
            FROM copy_sessions
            WHERE status = 'active'
            ORDER BY updated_at ASC
            "#,
        )?;
        let rows = stmt.query_map([], map_copy_session_row)?;
        collect_copy_session_rows(rows)
    }

    pub fn get_copy_session(&self, id: &str) -> Result<Option<CopySessionRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, contract_session_id, follower_address, leader_address, coefficient, status,
                   include_claims, allowed_pools_json, max_per_op_quote_xlm,
                   max_daily_quote_xlm, expires_at, cursor_ts, watermark_ts,
                   watermark_event_id, created_at, updated_at
            FROM copy_sessions
            WHERE id = ?1
            "#,
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(map_copy_session_row(&row)?))
        } else {
            Ok(None)
        }
    }

    pub fn update_copy_session(
        &self,
        id: &str,
        status: Option<&str>,
        coefficient: Option<f64>,
        watermark_ts: Option<i64>,
        watermark_event_id: Option<&str>,
        include_claims: Option<bool>,
        contract_session_id: Option<u32>,
    ) -> Result<()> {
        let Some(mut row) = self.get_copy_session(id)? else {
            anyhow::bail!("copy session not found: {id}");
        };
        if let Some(value) = status {
            row.status = value.to_string();
        }
        if let Some(value) = coefficient {
            row.coefficient = value;
        }
        if let Some(value) = watermark_ts {
            row.watermark_ts = value;
        }
        if let Some(value) = watermark_event_id {
            row.watermark_event_id = value.to_string();
        }
        if let Some(value) = include_claims {
            row.include_claims = value;
        }
        if contract_session_id.is_some() {
            row.contract_session_id = contract_session_id;
        }
        row.updated_at = chrono::Utc::now().timestamp();
        self.conn.execute(
            r#"
            UPDATE copy_sessions
            SET status = ?2, coefficient = ?3, watermark_ts = ?4,
                watermark_event_id = ?5, include_claims = ?6,
                contract_session_id = ?7, updated_at = ?8
            WHERE id = ?1
            "#,
            params![
                id,
                row.status,
                row.coefficient,
                row.watermark_ts,
                row.watermark_event_id,
                row.include_claims as i64,
                row.contract_session_id,
                row.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn copy_quote_used_since(&self, session_id: &str, since_ts: i64) -> Result<f64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(scaled_quote_xlm), 0) FROM copy_ops WHERE session_id = ?1 AND created_at >= ?2 AND status != 'rejected'",
            params![session_id, since_ts],
            |row| row.get(0),
        )?)
    }

    pub fn pause_active_sessions_for_pair(&self, follower_address: &str, leader_address: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            r#"
            UPDATE copy_sessions
            SET status = 'paused', updated_at = ?3
            WHERE follower_address = ?1 AND leader_address = ?2 AND status = 'active'
            "#,
            params![follower_address, leader_address, now],
        )?;
        Ok(())
    }

    pub fn insert_copy_op(&self, op: &CopyOpRow) -> Result<bool> {
        let rows = self.conn.execute(
            r#"
            INSERT OR IGNORE INTO copy_ops (
              id, session_id, source_event_id, pool_address, kind, position_key,
              leader_amounts_json, scaled_amounts_json, leader_quote_xlm,
              scaled_quote_xlm, status, note, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
            params![
                op.id,
                op.session_id,
                op.source_event_id,
                op.pool_address,
                op.kind,
                op.position_key,
                op.leader_amounts_json,
                op.scaled_amounts_json,
                op.leader_quote_xlm,
                op.scaled_quote_xlm,
                op.status,
                op.note,
                op.created_at,
                op.updated_at,
            ],
        )?;
        Ok(rows > 0)
    }

    /// Persist a canonical event exactly once for a future recorder worker.
    /// The worker is deliberately separate so this API process never needs a
    /// Soroban signing key.
    pub fn enqueue_recorder_event(&self, event: &RecorderEvent) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let amounts_json = serde_json::to_string(&event.amounts)?;
        let rows = self.conn.execute(
            r#"
            INSERT OR IGNORE INTO recorder_outbox (
              source_event_id, leader_address, pool_address, kind, amounts_json,
              quote_stroops, ledger, status, attempts, last_error, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, NULL, ?8, ?9)
            "#,
            params![
                event.source_event_id,
                event.leader_address,
                event.pool_address,
                event.kind,
                amounts_json,
                event.quote_stroops.to_string(),
                event.ledger,
                event.created_at,
                now,
            ],
        )?;
        Ok(rows > 0)
    }

    #[allow(dead_code)]
    pub fn pending_recorder_events(&self, limit: usize) -> Result<Vec<RecorderOutboxRow>> {
        let limit = limit.clamp(1, 1_000) as i64;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT source_event_id, leader_address, pool_address, kind, amounts_json,
                   quote_stroops, ledger, status, attempts, last_error, created_at, updated_at
            FROM recorder_outbox
            WHERE status = 'pending'
            ORDER BY created_at ASC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            let amounts_json: String = row.get(4)?;
            let quote_stroops: String = row.get(5)?;
            Ok(RecorderOutboxRow {
                source_event_id: row.get(0)?,
                leader_address: row.get(1)?,
                pool_address: row.get(2)?,
                kind: row.get(3)?,
                amounts: serde_json::from_str(&amounts_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(error))
                })?,
                quote_stroops: quote_stroops.parse().map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
                })?,
                ledger: row.get(6)?,
                status: row.get(7)?,
                attempts: row.get(8)?,
                last_error: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Claim a batch for one relayer. The lease makes retries safe after a
    /// worker crash while keeping the source event idempotency key intact.
    #[allow(dead_code)]
    pub fn claim_recorder_events(&self, limit: usize, lease_secs: i64) -> Result<Vec<RecorderOutboxRow>> {
        let now = chrono::Utc::now().timestamp();
        let lease_secs = lease_secs.max(30);
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result: anyhow::Result<Vec<RecorderOutboxRow>> = (|| {
            self.conn.execute(
                "UPDATE recorder_outbox SET status = 'pending', updated_at = ?1 WHERE status = 'processing' AND updated_at < ?2",
                params![now, now - lease_secs],
            )?;
            let mut rows = self.pending_recorder_events(limit)?;
            for row in &mut rows {
                self.conn.execute(
                    "UPDATE recorder_outbox SET status = 'processing', attempts = attempts + 1, updated_at = ?2 WHERE source_event_id = ?1 AND status = 'pending'",
                    params![row.source_event_id, now],
                )?;
                row.status = "processing".into();
                row.attempts = row.attempts.saturating_add(1);
                row.updated_at = now;
            }
            Ok(rows)
        })();
        match result {
            Ok(rows) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(rows)
            }
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(error.into())
            }
        }
    }

    #[allow(dead_code)]
    pub fn update_recorder_event(&self, source_event_id: &str, status: &str, error: Option<&str>) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            r#"
            UPDATE recorder_outbox
            SET status = ?2, attempts = attempts + 1, last_error = ?3, updated_at = ?4
            WHERE source_event_id = ?1
            "#,
            params![source_event_id, status, error, now],
        )?;
        Ok(())
    }

    pub fn recorder_outbox_status(&self) -> Result<RecorderOutboxStatus> {
        let mut stmt = self
            .conn
            .prepare("SELECT status, COUNT(*), MIN(created_at) FROM recorder_outbox GROUP BY status")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })?;
        let mut status = RecorderOutboxStatus::default();
        for row in rows {
            let (kind, count, oldest) = row?;
            match kind.as_str() {
                "pending" | "processing" => {
                    status.pending += count.max(0) as usize;
                    status.oldest_pending_at = match (status.oldest_pending_at, oldest) {
                        (Some(current), Some(candidate)) => Some(current.min(candidate)),
                        (current, candidate) => current.or(candidate),
                    };
                }
                "submitted" => status.submitted = count.max(0) as usize,
                "failed" => status.failed = count.max(0) as usize,
                _ => {}
            }
        }
        Ok(status)
    }

    pub fn list_copy_ops(&self, session_id: &str, status: Option<&str>) -> Result<Vec<CopyOpRow>> {
        if let Some(status) = status {
            let mut stmt = self.conn.prepare(
                r#"
                SELECT id, session_id, source_event_id, pool_address, kind, position_key,
                       leader_amounts_json, scaled_amounts_json, leader_quote_xlm,
                       scaled_quote_xlm, status, note, created_at, updated_at
                FROM copy_ops
                WHERE session_id = ?1 AND status = ?2
                ORDER BY created_at DESC
                "#,
            )?;
            let rows = stmt.query_map(params![session_id, status], map_copy_op_row)?;
            collect_copy_op_rows(rows)
        } else {
            let mut stmt = self.conn.prepare(
                r#"
                SELECT id, session_id, source_event_id, pool_address, kind, position_key,
                       leader_amounts_json, scaled_amounts_json, leader_quote_xlm,
                       scaled_quote_xlm, status, note, created_at, updated_at
                FROM copy_ops
                WHERE session_id = ?1
                ORDER BY created_at DESC
                "#,
            )?;
            let rows = stmt.query_map(params![session_id], map_copy_op_row)?;
            collect_copy_op_rows(rows)
        }
    }

    pub fn update_copy_op_status(&self, id: &str, status: &str, note: Option<&str>) -> Result<()> {
        let current: String = self
            .conn
            .query_row("SELECT status FROM copy_ops WHERE id = ?1", params![id], |row| {
                row.get(0)
            })
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("copy op not found: {id}"))?;
        if !copy_status_transition_allowed(&current, status) {
            anyhow::bail!("invalid copy op status transition: {current} -> {status}");
        }
        let updated_at = chrono::Utc::now().timestamp();
        let rows = self.conn.execute(
            r#"
            UPDATE copy_ops
            SET status = ?2, note = ?3, updated_at = ?4
            WHERE id = ?1
            "#,
            params![id, status, note, updated_at],
        )?;
        if rows == 0 {
            anyhow::bail!("copy op not found: {id}");
        }
        Ok(())
    }

    pub fn get_copy_op(&self, id: &str) -> Result<Option<CopyOpRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, session_id, source_event_id, pool_address, kind, position_key,
                   leader_amounts_json, scaled_amounts_json, leader_quote_xlm,
                   scaled_quote_xlm, status, note, created_at, updated_at
            FROM copy_ops
            WHERE id = ?1
            "#,
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(map_copy_op_row(&row)?))
        } else {
            Ok(None)
        }
    }

    pub fn events_for_actor_since(
        &self,
        actor: &str,
        since_ts: i64,
        after_event_id: &str,
        limit: usize,
    ) -> Result<Vec<PoolEventRow>> {
        let limit = limit.max(1).min(1000) as i64;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT event_id, tx_hash, ledger, created_at, pool_address, kind, body_json
            FROM pool_events
            WHERE kind IN (
              'deposit_liquidity', 'withdraw_liquidity', 'claim_fees', 'claim_protocol_fee'
            )
              AND json_extract(body_json, '$.derived.actor') = ?1
              AND (
                created_at > ?2
                OR (?3 != '' AND created_at = ?2 AND event_id > ?3)
              )
            ORDER BY created_at ASC, event_id ASC
            LIMIT ?4
            "#,
        )?;
        let rows = stmt.query_map(params![actor, since_ts, after_event_id, limit], map_event_row)?;
        collect_event_rows(rows)
    }

    /// Recent liquidity activity for an actor (Copy leader scouting).
    pub fn actor_liquidity_activity(
        &self,
        actor: &str,
        since_ts: i64,
        limit_events: usize,
    ) -> Result<(ActorLiquidityActivity, Vec<PoolEventRow>)> {
        let limit = limit_events.max(1).min(200) as i64;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT event_id, tx_hash, ledger, created_at, pool_address, kind, body_json
            FROM pool_events
            WHERE kind IN (
              'deposit_liquidity', 'withdraw_liquidity', 'claim_fees', 'claim_protocol_fee'
            )
              AND json_extract(body_json, '$.derived.actor') = ?1
              AND created_at >= ?2
            ORDER BY created_at DESC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(params![actor, since_ts, limit], map_event_row)?;
        let events = collect_event_rows(rows)?;

        let mut activity = ActorLiquidityActivity {
            since_ts,
            ..Default::default()
        };
        let mut pools = HashSet::new();
        for event in &events {
            pools.insert(event.pool_address.clone());
            activity.last_activity_at = activity
                .last_activity_at
                .map(|t| t.max(event.created_at))
                .or(Some(event.created_at));
            // Claim events must use an explicit fee quote. Some venues use
            // total_quote_xlm for position or token amounts, which is not a
            // fee and can inflate leader rankings by orders of magnitude.
            let quote_key = match event.kind.as_str() {
                "claim_fees" | "claim_protocol_fee" => "fee_quote_xlm",
                _ => "total_quote_xlm",
            };
            let unsupported_claim = event
                .body
                .pointer("/derived/venue")
                .and_then(Value::as_str)
                .is_some_and(|venue| venue != "aquarius") &&
                matches!(event.kind.as_str(), "claim_fees" | "claim_protocol_fee");
            let quote = (!unsupported_claim)
                .then(|| {
                    event
                        .body
                        .pointer(&format!("/derived/{quote_key}"))
                        .and_then(|v| v.as_f64())
                })
                .flatten()
                .or_else(|| {
                    (quote_key == "total_quote_xlm")
                        .then(|| event.body.pointer("/derived/quote_xlm").and_then(|v| v.as_f64()))
                        .flatten()
                })
                .filter(|v| v.is_finite() && *v > 0.0);
            match event.kind.as_str() {
                "deposit_liquidity" => {
                    activity.deposit_count += 1;
                    if let Some(q) = quote {
                        activity.deposit_quote_xlm += q;
                    }
                }
                "withdraw_liquidity" => {
                    activity.withdraw_count += 1;
                    if let Some(q) = quote {
                        activity.withdraw_quote_xlm += q;
                    }
                }
                "claim_fees" | "claim_protocol_fee" => {
                    activity.claim_count += 1;
                    if let Some(q) = quote {
                        activity.claim_quote_xlm += q;
                    }
                }
                _ => {}
            }
        }
        activity.event_count = events.len();
        activity.distinct_pools = pools.len();
        Ok((activity, events))
    }

    /// Full indexed history aggregates (SQL SUM — not limited to recent N
    /// events).
    pub fn actor_lifetime_totals(&self, actor: &str) -> Result<ActorLifetimeTotals> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
              kind,
              COUNT(*) AS c,
              COALESCE(SUM(CASE
                WHEN kind IN ('claim_fees', 'claim_protocol_fee')
                  AND COALESCE(json_extract(body_json, '$.derived.venue'), '') NOT IN ('', 'aquarius') THEN 0
                WHEN kind IN ('claim_fees', 'claim_protocol_fee') THEN
                  COALESCE(json_extract(body_json, '$.derived.fee_quote_xlm'), 0)
                ELSE
                  COALESCE(
                    json_extract(body_json, '$.derived.total_quote_xlm'),
                    json_extract(body_json, '$.derived.quote_xlm'),
                    0
                  )
              END), 0) AS quote_sum,
              MIN(created_at) AS first_ts,
              MAX(created_at) AS last_ts
            FROM pool_events
            WHERE kind IN (
              'deposit_liquidity', 'withdraw_liquidity', 'claim_fees', 'claim_protocol_fee'
            )
              AND json_extract(body_json, '$.derived.actor') = ?1
            GROUP BY kind
            "#,
        )?;
        let rows = stmt.query_map(params![actor], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as usize,
                row.get::<_, f64>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<i64>>(4)?,
            ))
        })?;
        let mut totals = ActorLifetimeTotals::default();
        for row in rows {
            let (kind, count, quote, first, last) = row?;
            totals.first_activity_at = match (totals.first_activity_at, first) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (None, b) => b,
                (a, None) => a,
            };
            totals.last_activity_at = match (totals.last_activity_at, last) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (None, b) => b,
                (a, None) => a,
            };
            let q = if quote.is_finite() && quote > 0.0 { quote } else { 0.0 };
            match kind.as_str() {
                "deposit_liquidity" => {
                    totals.deposit_count += count;
                    totals.deposit_quote_xlm += q;
                }
                "withdraw_liquidity" => {
                    totals.withdraw_count += count;
                    totals.withdraw_quote_xlm += q;
                }
                "claim_fees" | "claim_protocol_fee" => {
                    totals.claim_count += count;
                    totals.claim_quote_xlm += q;
                }
                _ => {}
            }
        }
        let pools: i64 = self.conn.query_row(
            r#"
            SELECT COUNT(DISTINCT pool_address)
            FROM pool_events
            WHERE kind IN (
              'deposit_liquidity', 'withdraw_liquidity', 'claim_fees', 'claim_protocol_fee'
            )
              AND json_extract(body_json, '$.derived.actor') = ?1
            "#,
            params![actor],
            |row| row.get(0),
        )?;
        totals.distinct_pools = pools.max(0) as usize;
        Ok(totals)
    }

    /// Distinct Aquarius pools an actor has deposit/withdraw/claim activity on
    /// (for narrow position scans).
    pub fn actor_pool_addresses(&self, actor: &str, since_ts: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT DISTINCT pool_address
            FROM pool_events
            WHERE kind IN (
              'deposit_liquidity', 'withdraw_liquidity', 'claim_fees', 'claim_protocol_fee'
            )
              AND json_extract(body_json, '$.derived.actor') = ?1
              AND created_at >= ?2
            ORDER BY pool_address ASC
            "#,
        )?;
        let rows = stmt.query_map(params![actor, since_ts], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn known_liquidity_actors(&self, limit: usize) -> Result<Vec<String>> {
        let limit = limit.max(1).min(10_000) as i64;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT json_extract(body_json, '$.derived.actor') AS actor,
                   MAX(created_at) AS last_activity_at
            FROM pool_events
            WHERE kind IN ('deposit_liquidity', 'withdraw_liquidity', 'claim_fees', 'claim_protocol_fee')
              AND json_extract(body_json, '$.derived.actor') GLOB 'G*'
            GROUP BY actor
            ORDER BY last_activity_at DESC, actor ASC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn upsert_actor_fee_snapshot(
        &self,
        actor: &str,
        pool_address: &str,
        venue: &str,
        unclaimed_quote_xlm: Option<f64>,
        position_value_quote_xlm: Option<f64>,
        status: &str,
        observed_at: i64,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO actor_fee_snapshots
              (actor, pool_address, venue, unclaimed_quote_xlm, position_value_quote_xlm, status, observed_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(actor, pool_address) DO UPDATE SET
              venue=excluded.venue,
              unclaimed_quote_xlm=excluded.unclaimed_quote_xlm,
              position_value_quote_xlm=excluded.position_value_quote_xlm,
              status=excluded.status,
              observed_at=excluded.observed_at
            "#,
            params![
                actor,
                pool_address,
                venue,
                unclaimed_quote_xlm,
                position_value_quote_xlm,
                status,
                observed_at
            ],
        )?;
        Ok(())
    }

    pub fn insert_actor_fee_snapshot_history(
        &self,
        actor: &str,
        pool_address: &str,
        venue: &str,
        unclaimed_quote_xlm: Option<f64>,
        position_value_quote_xlm: Option<f64>,
        status: &str,
        observed_at: i64,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO actor_fee_snapshot_history
              (actor, pool_address, venue, unclaimed_quote_xlm,
               position_value_quote_xlm, status, observed_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                actor,
                pool_address,
                venue,
                unclaimed_quote_xlm,
                position_value_quote_xlm,
                status,
                observed_at
            ],
        )?;
        Ok(())
    }

    /// Mark the actor's previously known positions as zero before replacing
    /// the current set. A later insert at the same timestamp supersedes the
    /// zero for positions that are still open.
    pub fn record_actor_fee_snapshot_history_zeroed(&self, actor: &str, observed_at: i64) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "SELECT pool_address, venue, unclaimed_quote_xlm, status FROM actor_fee_snapshots WHERE actor = ?1",
        )?;
        let rows = stmt.query_map(params![actor], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (pool_address, venue, unclaimed, status) = row?;
            let verified = status == "ok" && unclaimed.is_some();
            self.insert_actor_fee_snapshot_history(
                actor,
                &pool_address,
                &venue,
                verified.then_some(0.0),
                verified.then_some(0.0),
                if verified { "ok" } else { &status },
                observed_at,
            )?;
        }
        Ok(())
    }

    pub fn actor_fee_snapshot_deltas(&self, since_ts: i64) -> Result<HashMap<String, f64>> {
        let mut baseline = HashMap::<(String, String), f64>::new();
        let mut unavailable = HashSet::<String>::new();
        // Snapshot history is append-only and observed_at is assigned at
        // insertion time, so the largest id per actor/pool is the latest
        // boundary row. This avoids a correlated lookup over millions of rows.
        let mut stmt = self.conn.prepare(
            r#"
            SELECT actor, pool_address, unclaimed_quote_xlm, status
            FROM actor_fee_snapshot_history
            WHERE id IN (
              SELECT MAX(id)
              FROM actor_fee_snapshot_history
              WHERE observed_at <= ?1
              GROUP BY actor, pool_address
            )
            "#,
        )?;
        let rows = stmt.query_map(params![since_ts], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let ((actor, pool), value, status) = row?;
            if status != "ok" || value.is_none() {
                unavailable.insert(actor);
            } else {
                baseline.insert((actor, pool), value.unwrap_or_default());
            }
        }

        let mut deltas = HashMap::<String, f64>::new();
        let mut current_stmt = self.conn.prepare(
            "SELECT actor, pool_address, unclaimed_quote_xlm, status FROM actor_fee_snapshots WHERE actor GLOB 'G*'",
        )?;
        let current_rows = current_stmt.query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                row.get::<_, Option<f64>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in current_rows {
            let ((actor, pool), current, status) = row?;
            if status != "ok" || current.is_none() {
                unavailable.insert(actor);
                continue;
            }
            let current = current.unwrap_or_default();
            let Some(previous) = baseline.remove(&(actor.clone(), pool)) else {
                // A snapshot created after the requested window has no valid
                // starting value. Do not treat its current fee as windowed
                // accrual; wait until a real boundary snapshot exists.
                unavailable.insert(actor);
                continue;
            };
            *deltas.entry(actor).or_default() += current - previous;
        }
        for ((actor, _pool), previous) in baseline {
            if !unavailable.contains(&actor) {
                *deltas.entry(actor).or_default() -= previous;
            }
        }
        for actor in unavailable {
            deltas.remove(&actor);
        }
        Ok(deltas)
    }

    /// Calculate one actor's verified unclaimed-fee change without scanning
    /// snapshot history for every other actor. `None` means at least one leg
    /// lacks a valid boundary or current fee value.
    pub fn actor_fee_snapshot_delta(&self, actor: &str, since_ts: i64) -> Result<Option<f64>> {
        let mut baseline = HashMap::<String, f64>::new();
        let mut stmt = self.conn.prepare(
            r#"
            SELECT h.pool_address, h.unclaimed_quote_xlm, h.status
            FROM actor_fee_snapshot_history h
            WHERE h.actor = ?1
              AND h.observed_at <= ?2
              AND h.id = (
                SELECT h2.id
                FROM actor_fee_snapshot_history h2
                WHERE h2.actor = h.actor
                  AND h2.pool_address = h.pool_address
                  AND h2.observed_at <= ?2
                ORDER BY h2.observed_at DESC, h2.id DESC
                LIMIT 1
              )
            "#,
        )?;
        let rows = stmt.query_map(params![actor, since_ts], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (pool, value, status) = row?;
            if status != "ok" || value.is_none() {
                return Ok(None);
            }
            baseline.insert(pool, value.unwrap_or_default());
        }

        let mut current_stmt = self
            .conn
            .prepare("SELECT pool_address, unclaimed_quote_xlm, status FROM actor_fee_snapshots WHERE actor = ?1")?;
        let current_rows = current_stmt.query_map(params![actor], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut delta = 0.0;
        let mut current_count = 0usize;
        for row in current_rows {
            let (pool, value, status) = row?;
            if status != "ok" || value.is_none() {
                return Ok(None);
            }
            let Some(previous) = baseline.remove(&pool) else {
                return Ok(None);
            };
            delta += value.unwrap_or_default() - previous;
            current_count += 1;
        }
        if current_count == 0 {
            return if baseline.is_empty() {
                Ok(None)
            } else {
                // A zeroed history row is the valid terminal state for a
                // position closed during the window.
                Ok(Some(-baseline.values().sum::<f64>()))
            };
        }
        if !baseline.is_empty() {
            return Ok(None);
        }
        Ok(Some(delta))
    }

    pub fn clear_actor_fee_snapshots(&self, actor: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM actor_fee_snapshots WHERE actor = ?1", params![actor])?;
        Ok(())
    }

    pub fn actor_fee_snapshot_totals(&self) -> Result<HashMap<String, ActorFeeSnapshotTotal>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
              actor,
              COALESCE(SUM(unclaimed_quote_xlm), 0),
              COALESCE(SUM(position_value_quote_xlm), 0),
              COUNT(*),
              COUNT(DISTINCT pool_address),
              MAX(CASE WHEN unclaimed_quote_xlm IS NOT NULL THEN observed_at END)
            FROM actor_fee_snapshots
            WHERE actor GLOB 'G*' AND status IN ('ok', 'fee_unavailable')
            GROUP BY actor
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                ActorFeeSnapshotTotal {
                    unclaimed_quote_xlm: row.get(1)?,
                    position_value_quote_xlm: row.get(2)?,
                    position_count: row.get::<_, i64>(3)?.max(0) as usize,
                    pool_count: row.get::<_, i64>(4)?.max(0) as usize,
                    observed_at: row.get(5)?,
                },
            ))
        })?;
        let mut totals = HashMap::new();
        for row in rows {
            let (actor, total) = row?;
            totals.insert(actor, total);
        }
        Ok(totals)
    }

    /// Return Sushi V3 tick ranges observed for an actor. Sushi pools expose
    /// point reads for a known range, not an enumerable owner-position list;
    /// callers must verify these candidates against current contract state.
    pub fn sushi_position_range_candidates(
        &self,
        actor: &str,
        since_ts: i64,
        limit_events: usize,
    ) -> Result<Vec<PositionRangeCandidate>> {
        let limit = limit_events.max(1).min(10_000) as i64;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT event_id, tx_hash, ledger, created_at, pool_address, kind, body_json
            FROM pool_events
            WHERE kind IN ('deposit_liquidity', 'withdraw_liquidity')
              AND (
                json_extract(body_json, '$.derived.actor') = ?1
                OR json_extract(body_json, '$.data[0].sender.value') = ?1
                OR json_extract(body_json, '$.data[0].recipient.value') = ?1
              )
              AND created_at >= ?2
            ORDER BY created_at DESC, event_id DESC
            LIMIT ?3
            "#,
        )?;
        let rows = stmt.query_map(params![actor, since_ts, limit], map_event_row)?;
        let events = collect_event_rows(rows)?;
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for event in events {
            let derived = event.body.get("derived").unwrap_or(&Value::Null);
            if derived.get("venue").and_then(Value::as_str) != Some("sushi_v3") {
                continue;
            }
            // Older rows may have recorded the Sushi position manager contract
            // as actor. Accept the wallet in the event sender/recipient so
            // those rows remain discoverable without a full historical replay.
            let actor_matches = derived.get("actor").and_then(Value::as_str) == Some(actor) ||
                event
                    .body
                    .get("data")
                    .and_then(Value::as_array)
                    .and_then(|data| data.first())
                    .and_then(|item| item.get("sender").or_else(|| item.get("recipient")))
                    .and_then(|value| value.get("value").and_then(Value::as_str))
                    .as_deref() ==
                    Some(actor);
            if !actor_matches {
                continue;
            }
            let Some(lower) = derived.get("tick_lower").and_then(Value::as_i64) else {
                continue;
            };
            let Some(upper) = derived.get("tick_upper").and_then(Value::as_i64) else {
                continue;
            };
            let Ok(tick_lower) = i32::try_from(lower) else {
                continue;
            };
            let Ok(tick_upper) = i32::try_from(upper) else {
                continue;
            };
            if tick_lower >= tick_upper {
                continue;
            }
            let key = format!("{}:{tick_lower}:{tick_upper}", event.pool_address);
            if seen.insert(key) {
                out.push(PositionRangeCandidate {
                    pool_address: event.pool_address,
                    tick_lower,
                    tick_upper,
                });
            }
        }
        out.sort_by(|a, b| {
            (&a.pool_address, a.tick_lower, a.tick_upper).cmp(&(&b.pool_address, b.tick_lower, b.tick_upper))
        });
        Ok(out)
    }

    /// Ranked actors over a window. `fees` is the default claimed-fee proxy;
    /// `activity` surfaces deposit/withdraw/claim actors even when they have
    /// not claimed fees yet.
    pub fn top_liquidity_actors(&self, since_ts: i64, limit: usize, sort: &str) -> Result<Vec<TopLiquidityActor>> {
        let limit = limit.max(1).min(10_000) as i64;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
              json_extract(body_json, '$.derived.actor') AS actor,
              kind,
              CASE
                WHEN kind IN ('claim_fees', 'claim_protocol_fee')
                  AND COALESCE(json_extract(body_json, '$.derived.venue'), '') NOT IN ('', 'aquarius') THEN 0
                WHEN kind IN ('claim_fees', 'claim_protocol_fee') THEN
                  COALESCE(json_extract(body_json, '$.derived.fee_quote_xlm'), 0)
                ELSE
                  COALESCE(
                    json_extract(body_json, '$.derived.total_quote_xlm'),
                    json_extract(body_json, '$.derived.quote_xlm'),
                    0
                  )
              END AS quote_xlm,
              pool_address,
              created_at
            FROM pool_events
            WHERE kind IN (
              'deposit_liquidity', 'withdraw_liquidity', 'claim_fees', 'claim_protocol_fee'
            )
              AND created_at >= ?1
              AND json_extract(body_json, '$.derived.actor') IS NOT NULL
              AND length(json_extract(body_json, '$.derived.actor')) >= 56
            "#,
        )?;
        let mut by_actor: HashMap<String, TopLiquidityActor> = HashMap::new();
        let mut pools_by_actor: HashMap<String, HashSet<String>> = HashMap::new();

        // Build the candidate universe from all indexed LP lifecycle history.
        // The requested window controls the metrics below, not whether a
        // previously observed actor disappears from the board entirely.
        let mut known_stmt = self.conn.prepare(
            r#"
            SELECT
              json_extract(body_json, '$.derived.actor') AS actor,
              pool_address,
              MAX(created_at) AS last_activity_at
            FROM pool_events
            WHERE kind IN (
              'deposit_liquidity', 'withdraw_liquidity', 'claim_fees', 'claim_protocol_fee'
            )
              AND json_extract(body_json, '$.derived.actor') IS NOT NULL
              AND length(json_extract(body_json, '$.derived.actor')) >= 56
            GROUP BY actor, pool_address
            "#,
        )?;
        let known_rows = known_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        for row in known_rows {
            let (actor, pool, last_activity_at) = row?;
            if !actor.starts_with('G') {
                continue;
            }
            let entry = by_actor.entry(actor.clone()).or_insert_with(|| TopLiquidityActor {
                address: actor.clone(),
                last_activity_at: Some(last_activity_at),
                ..Default::default()
            });
            entry.last_activity_at = entry
                .last_activity_at
                .map(|t| t.max(last_activity_at))
                .or(Some(last_activity_at));
            pools_by_actor.entry(actor).or_default().insert(pool);
        }

        let rows = stmt.query_map(params![since_ts], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        for row in rows {
            let (actor, kind, quote, pool, created_at) = row?;
            if !actor.starts_with('G') {
                continue;
            }
            let entry = by_actor.entry(actor.clone()).or_insert_with(|| TopLiquidityActor {
                address: actor.clone(),
                ..Default::default()
            });
            entry.event_count += 1;
            entry.last_activity_at = entry.last_activity_at.map(|t| t.max(created_at)).or(Some(created_at));
            pools_by_actor.entry(actor.clone()).or_default().insert(pool);
            let q = if quote.is_finite() && quote > 0.0 { quote } else { 0.0 };
            match kind.as_str() {
                "deposit_liquidity" => {
                    entry.deposit_count += 1;
                    entry.deposit_quote_xlm += q;
                }
                "withdraw_liquidity" => {
                    entry.withdraw_count += 1;
                    entry.withdraw_quote_xlm += q;
                }
                "claim_fees" | "claim_protocol_fee" => {
                    entry.claim_count += 1;
                    entry.claim_quote_xlm += q;
                }
                _ => {}
            }
        }
        for (actor, pools) in pools_by_actor {
            if let Some(entry) = by_actor.get_mut(&actor) {
                entry.distinct_pools = pools.len();
            }
        }
        let mut ranked: Vec<TopLiquidityActor> = by_actor.into_values().collect();
        if sort == "activity" {
            ranked.sort_by(|a, b| {
                b.event_count
                    .cmp(&a.event_count)
                    .then_with(|| b.distinct_pools.cmp(&a.distinct_pools))
                    .then_with(|| {
                        b.deposit_quote_xlm
                            .partial_cmp(&a.deposit_quote_xlm)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });
        } else {
            ranked.sort_by(|a, b| {
                b.claim_quote_xlm
                    .partial_cmp(&a.claim_quote_xlm)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        b.deposit_quote_xlm
                            .partial_cmp(&a.deposit_quote_xlm)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| b.event_count.cmp(&a.event_count))
            });
        }
        ranked.truncate(limit as usize);
        Ok(ranked)
    }

    pub fn rollups_map(&self) -> Result<HashMap<String, HashMap<String, PoolRollupRow>>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT pool_address, window, as_of_ts, sample_count, volume_quote,
                   fee_quote, avg_tvl, fee_tvl, tx_count
            FROM pool_rollups
            ORDER BY pool_address ASC, window ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(PoolRollupRow {
                pool_address: row.get(0)?,
                window: row.get(1)?,
                as_of_ts: row.get(2)?,
                sample_count: row.get::<_, i64>(3)? as usize,
                volume_quote: row.get(4)?,
                fee_quote: row.get(5)?,
                avg_tvl: row.get(6)?,
                fee_tvl: row.get(7)?,
                tx_count: row.get::<_, i64>(8)? as usize,
            })
        })?;

        let mut out: HashMap<String, HashMap<String, PoolRollupRow>> = HashMap::new();
        for row in rows {
            let row = row?;
            out.entry(row.pool_address.clone())
                .or_default()
                .insert(row.window.clone(), row);
        }
        Ok(out)
    }

    pub fn rollups_for_pool(&self, pool_address: &str) -> Result<HashMap<String, PoolRollupRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT pool_address, window, as_of_ts, sample_count, volume_quote,
                   fee_quote, avg_tvl, fee_tvl, tx_count
            FROM pool_rollups
            WHERE pool_address = ?1
            ORDER BY window ASC
            "#,
        )?;
        let rows = stmt.query_map(params![pool_address], |row| {
            Ok(PoolRollupRow {
                pool_address: row.get(0)?,
                window: row.get(1)?,
                as_of_ts: row.get(2)?,
                sample_count: row.get::<_, i64>(3)? as usize,
                volume_quote: row.get(4)?,
                fee_quote: row.get(5)?,
                avg_tvl: row.get(6)?,
                fee_tvl: row.get(7)?,
                tx_count: row.get::<_, i64>(8)? as usize,
            })
        })?;

        let mut out = HashMap::new();
        for row in rows {
            let row = row?;
            out.insert(row.window.clone(), row);
        }
        Ok(out)
    }

    pub fn status(&self) -> Result<IndexerStatus> {
        let cursor_ledger = self
            .conn
            .query_row("SELECT last_ledger FROM indexer_cursor WHERE id = 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .ok()
            .map(|value| value as u32);
        let event_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM pool_events", [], |row| row.get(0))?;
        let swap_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM pool_swaps", [], |row| row.get(0))?;
        let rollup_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM pool_rollups", [], |row| row.get(0))?;
        let distinct_event_pools: i64 =
            self.conn
                .query_row("SELECT COUNT(DISTINCT pool_address) FROM pool_events", [], |row| {
                    row.get(0)
                })?;
        let distinct_rollup_pools: i64 =
            self.conn
                .query_row("SELECT COUNT(DISTINCT pool_address) FROM pool_rollups", [], |row| {
                    row.get(0)
                })?;
        let last_event_at = self
            .conn
            .query_row("SELECT MAX(created_at) FROM pool_events", [], |row| row.get(0))
            .ok()
            .flatten();
        let last_rollup_at = self
            .conn
            .query_row("SELECT MAX(as_of_ts) FROM pool_rollups", [], |row| row.get(0))
            .ok()
            .flatten();

        Ok(IndexerStatus {
            cursor_ledger,
            event_count: event_count.max(0) as usize,
            swap_count: swap_count.max(0) as usize,
            rollup_count: rollup_count.max(0) as usize,
            distinct_event_pools: distinct_event_pools.max(0) as usize,
            distinct_rollup_pools: distinct_rollup_pools.max(0) as usize,
            last_event_at,
            last_rollup_at,
        })
    }

    pub fn pool_activity_map(&self) -> Result<HashMap<String, PoolActivityRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
              e.pool_address,
              MIN(e.created_at) AS first_event_at,
              MAX(e.created_at) AS last_event_at,
              COUNT(*) AS event_count,
              COALESCE(s.swap_count, 0) AS swap_count
            FROM pool_events e
            LEFT JOIN (
              SELECT pool_address, COUNT(*) AS swap_count
              FROM pool_swaps
              GROUP BY pool_address
            ) s ON s.pool_address = e.pool_address
            GROUP BY e.pool_address, s.swap_count
            ORDER BY e.pool_address ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(PoolActivityRow {
                pool_address: row.get(0)?,
                first_event_at: row.get(1)?,
                last_event_at: row.get(2)?,
                event_count: row.get::<_, i64>(3)?.max(0) as usize,
                swap_count: row.get::<_, i64>(4)?.max(0) as usize,
            })
        })?;

        let mut out = HashMap::new();
        for row in rows {
            let row = row?;
            out.insert(row.pool_address.clone(), row);
        }
        Ok(out)
    }

    pub fn pool_activity(&self, pool_address: &str) -> Result<Option<PoolActivityRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
              e.pool_address,
              MIN(e.created_at) AS first_event_at,
              MAX(e.created_at) AS last_event_at,
              COUNT(*) AS event_count,
              COALESCE(s.swap_count, 0) AS swap_count
            FROM pool_events e
            LEFT JOIN (
              SELECT pool_address, COUNT(*) AS swap_count
              FROM pool_swaps
              GROUP BY pool_address
            ) s ON s.pool_address = e.pool_address
            WHERE e.pool_address = ?1
            GROUP BY e.pool_address, s.swap_count
            "#,
        )?;
        let mut rows = stmt.query(params![pool_address])?;
        if let Some(row) = rows.next()? {
            Ok(Some(PoolActivityRow {
                pool_address: row.get(0)?,
                first_event_at: row.get(1)?,
                last_event_at: row.get(2)?,
                event_count: row.get::<_, i64>(3)?.max(0) as usize,
                swap_count: row.get::<_, i64>(4)?.max(0) as usize,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn token_metadata(&self, address: &str) -> Result<Option<TokenMetadataRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT address, symbol, name, issuer, domain, icon FROM token_metadata WHERE address = ?1")?;
        let mut rows = stmt.query(params![address])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(TokenMetadataRow {
            address: row.get(0)?,
            symbol: row.get(1)?,
            name: row.get(2)?,
            issuer: row.get(3)?,
            domain: row.get(4)?,
            icon: row.get(5)?,
        }))
    }

    pub fn upsert_token_metadata(&self, metadata: &TokenMetadataRow) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO token_metadata
              (address, symbol, name, issuer, domain, icon, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%s', 'now'))
            ON CONFLICT(address) DO UPDATE SET
              symbol=excluded.symbol,
              name=excluded.name,
              issuer=excluded.issuer,
              domain=excluded.domain,
              icon=excluded.icon,
              updated_at=excluded.updated_at
            "#,
            params![
                metadata.address,
                metadata.symbol,
                metadata.name,
                metadata.issuer,
                metadata.domain,
                metadata.icon,
            ],
        )?;
        Ok(())
    }

    pub fn recent_pool_events(&self, pool_address: &str, limit: usize) -> Result<Vec<PoolEventRow>> {
        let limit = limit.max(1).min(100) as i64;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT event_id, tx_hash, ledger, created_at, pool_address, kind, body_json
            FROM pool_events
            WHERE pool_address = ?1
            ORDER BY created_at DESC, ledger DESC, id DESC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![pool_address, limit], |row| {
            let body_json: String = row.get(6)?;
            let body = serde_json::from_str(&body_json).unwrap_or(Value::Null);
            Ok(PoolEventRow {
                event_id: row.get(0)?,
                tx_hash: row.get(1)?,
                ledger: row.get::<_, i64>(2)? as u32,
                created_at: row.get(3)?,
                pool_address: row.get(4)?,
                kind: row.get(5)?,
                body,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Return the newest reserve quote event for each pool in one query.
    ///
    /// The pool list endpoint uses this to avoid showing an old snapshot while
    /// the detail endpoint is already able to see a newer reserve update.
    pub fn latest_reserves_quote_xlm_map(&self) -> Result<HashMap<String, (i64, f64)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT created_at, pool_address, body_json
            FROM pool_events
            WHERE kind IN ('update_reserves', 'reserves_sync')
            ORDER BY created_at DESC, id DESC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            let created_at: i64 = row.get(0)?;
            let pool_address: String = row.get(1)?;
            let body_json: String = row.get(2)?;
            Ok((created_at, pool_address, body_json))
        })?;
        let mut latest = HashMap::new();
        for row in rows {
            let (created_at, pool_address, body_json) = row?;
            if latest.contains_key(&pool_address) {
                continue;
            }
            let body: Value = serde_json::from_str(&body_json).unwrap_or(Value::Null);
            let Some(quote) = body
                .pointer("/derived/reserves_quote_xlm")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && *value > 0.0)
            else {
                continue;
            };
            latest.insert(pool_address, (created_at, quote));
        }
        Ok(latest)
    }

    pub fn pool_activity_summary(&self, pool_address: &str, since_ts: i64) -> Result<PoolActivitySummaryRow> {
        let (event_count_24h, deposit_count_24h, withdraw_count_24h, claim_count_24h, update_count_24h): (
            i64,
            i64,
            i64,
            i64,
            i64,
        ) = self.conn.query_row(
            r#"
            SELECT
              COUNT(*) AS event_count,
              SUM(CASE WHEN kind = 'deposit_liquidity' THEN 1 ELSE 0 END) AS deposit_count,
              SUM(CASE WHEN kind = 'withdraw_liquidity' THEN 1 ELSE 0 END) AS withdraw_count,
              SUM(CASE WHEN kind IN ('claim_fees', 'claim_protocol_fee') THEN 1 ELSE 0 END) AS claim_count,
              SUM(CASE WHEN kind IN ('update_reserves', 'reserves_sync') THEN 1 ELSE 0 END) AS update_count
            FROM pool_events
            WHERE pool_address = ?1 AND created_at >= ?2
            "#,
            params![pool_address, since_ts],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                ))
            },
        )?;

        let (swap_count_24h, volume_quote_24h, fee_quote_24h): (i64, f64, f64) = self.conn.query_row(
            r#"
            SELECT
              COUNT(*) AS swap_count,
              COALESCE(SUM(volume_quote), 0),
              COALESCE(SUM(fee_quote), 0)
            FROM pool_swaps
            WHERE pool_address = ?1 AND created_at >= ?2
            "#,
            params![pool_address, since_ts],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                    row.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                    row.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
                ))
            },
        )?;

        let recent_events = self.recent_pool_events_since(pool_address, since_ts)?;
        let mut deposit_quote_24h = 0.0;
        let mut withdraw_quote_24h = 0.0;
        let mut claim_quote_24h = 0.0;
        let mut update_times = Vec::new();
        for event in recent_events {
            match event.kind.as_str() {
                "deposit_liquidity" => {
                    deposit_quote_24h += derived_number(&event.body, "total_quote_xlm");
                }
                "withdraw_liquidity" => {
                    withdraw_quote_24h += derived_number(&event.body, "total_quote_xlm");
                }
                "claim_fees" | "claim_protocol_fee" => {
                    claim_quote_24h += derived_number(&event.body, "fee_quote_xlm");
                }
                "update_reserves" | "reserves_sync" => {
                    update_times.push(event.created_at);
                }
                _ => {}
            }
        }
        let (avg_update_interval_secs_24h, latest_update_at_24h) = cadence_from_times(&mut update_times);

        Ok(PoolActivitySummaryRow {
            event_count_24h: event_count_24h.max(0) as usize,
            swap_count_24h: swap_count_24h.max(0) as usize,
            volume_quote_24h,
            fee_quote_24h,
            deposit_quote_24h,
            withdraw_quote_24h,
            net_liquidity_delta_quote_24h: deposit_quote_24h - withdraw_quote_24h,
            claim_quote_24h,
            avg_update_interval_secs_24h,
            latest_update_at_24h,
            deposit_count_24h: deposit_count_24h.max(0) as usize,
            withdraw_count_24h: withdraw_count_24h.max(0) as usize,
            claim_count_24h: claim_count_24h.max(0) as usize,
            update_count_24h: update_count_24h.max(0) as usize,
        })
    }

    pub fn pool_activity_summary_map(&self, since_ts: i64) -> Result<HashMap<String, PoolActivitySummaryRow>> {
        let mut event_stmt = self.conn.prepare(
            r#"
            SELECT
              pool_address,
              COUNT(*) AS event_count,
              SUM(CASE WHEN kind = 'deposit_liquidity' THEN 1 ELSE 0 END) AS deposit_count,
              SUM(CASE WHEN kind = 'withdraw_liquidity' THEN 1 ELSE 0 END) AS withdraw_count,
              SUM(CASE WHEN kind IN ('claim_fees', 'claim_protocol_fee') THEN 1 ELSE 0 END) AS claim_count,
              SUM(CASE WHEN kind IN ('update_reserves', 'reserves_sync') THEN 1 ELSE 0 END) AS update_count
            FROM pool_events
            WHERE created_at >= ?1
            GROUP BY pool_address
            "#,
        )?;
        let event_rows = event_stmt.query_map(params![since_ts], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                row.get::<_, Option<i64>>(5)?.unwrap_or(0),
            ))
        })?;

        let mut out = HashMap::new();
        for row in event_rows {
            let (pool_address, event_count, deposit_count, withdraw_count, claim_count, update_count) = row?;
            out.insert(
                pool_address,
                PoolActivitySummaryRow {
                    event_count_24h: event_count.max(0) as usize,
                    swap_count_24h: 0,
                    volume_quote_24h: 0.0,
                    fee_quote_24h: 0.0,
                    deposit_quote_24h: 0.0,
                    withdraw_quote_24h: 0.0,
                    net_liquidity_delta_quote_24h: 0.0,
                    claim_quote_24h: 0.0,
                    avg_update_interval_secs_24h: None,
                    latest_update_at_24h: None,
                    deposit_count_24h: deposit_count.max(0) as usize,
                    withdraw_count_24h: withdraw_count.max(0) as usize,
                    claim_count_24h: claim_count.max(0) as usize,
                    update_count_24h: update_count.max(0) as usize,
                },
            );
        }

        let mut swap_stmt = self.conn.prepare(
            r#"
            SELECT
              pool_address,
              COUNT(*) AS swap_count,
              COALESCE(SUM(volume_quote), 0),
              COALESCE(SUM(fee_quote), 0)
            FROM pool_swaps
            WHERE created_at >= ?1
            GROUP BY pool_address
            "#,
        )?;
        let swap_rows = swap_stmt.query_map(params![since_ts], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                row.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
                row.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
            ))
        })?;

        for row in swap_rows {
            let (pool_address, swap_count, volume_quote, fee_quote) = row?;
            let summary = out.entry(pool_address).or_insert(PoolActivitySummaryRow {
                event_count_24h: 0,
                swap_count_24h: 0,
                volume_quote_24h: 0.0,
                fee_quote_24h: 0.0,
                deposit_quote_24h: 0.0,
                withdraw_quote_24h: 0.0,
                net_liquidity_delta_quote_24h: 0.0,
                claim_quote_24h: 0.0,
                avg_update_interval_secs_24h: None,
                latest_update_at_24h: None,
                deposit_count_24h: 0,
                withdraw_count_24h: 0,
                claim_count_24h: 0,
                update_count_24h: 0,
            });
            summary.swap_count_24h = swap_count.max(0) as usize;
            summary.volume_quote_24h = volume_quote;
            summary.fee_quote_24h = fee_quote;
        }

        let recent_events = self.recent_all_pool_events_since(since_ts)?;
        for event in recent_events {
            let summary = out.entry(event.pool_address.clone()).or_insert(PoolActivitySummaryRow {
                event_count_24h: 0,
                swap_count_24h: 0,
                volume_quote_24h: 0.0,
                fee_quote_24h: 0.0,
                deposit_quote_24h: 0.0,
                withdraw_quote_24h: 0.0,
                net_liquidity_delta_quote_24h: 0.0,
                claim_quote_24h: 0.0,
                avg_update_interval_secs_24h: None,
                latest_update_at_24h: None,
                deposit_count_24h: 0,
                withdraw_count_24h: 0,
                claim_count_24h: 0,
                update_count_24h: 0,
            });
            match event.kind.as_str() {
                "deposit_liquidity" => {
                    summary.deposit_quote_24h += derived_number(&event.body, "total_quote_xlm");
                }
                "withdraw_liquidity" => {
                    summary.withdraw_quote_24h += derived_number(&event.body, "total_quote_xlm");
                }
                "claim_fees" | "claim_protocol_fee" => {
                    summary.claim_quote_24h += derived_number(&event.body, "fee_quote_xlm");
                }
                "update_reserves" | "reserves_sync" => {
                    let latest = summary.latest_update_at_24h;
                    summary.latest_update_at_24h = Some(latest.map_or(event.created_at, |v| v.max(event.created_at)));
                }
                _ => {}
            }
            summary.net_liquidity_delta_quote_24h = summary.deposit_quote_24h - summary.withdraw_quote_24h;
        }

        let mut cadence_stmt = self.conn.prepare(
            r#"
            SELECT pool_address, created_at
            FROM pool_events
            WHERE created_at >= ?1 AND kind IN ('update_reserves', 'reserves_sync')
            ORDER BY pool_address ASC, created_at DESC
            "#,
        )?;
        let cadence_rows = cadence_stmt.query_map(params![since_ts], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut by_pool: HashMap<String, Vec<i64>> = HashMap::new();
        for row in cadence_rows {
            let (pool_address, created_at) = row?;
            by_pool.entry(pool_address).or_default().push(created_at);
        }
        for (pool_address, mut times) in by_pool {
            let (avg, latest) = cadence_from_times(&mut times);
            let summary = out.entry(pool_address).or_insert(PoolActivitySummaryRow {
                event_count_24h: 0,
                swap_count_24h: 0,
                volume_quote_24h: 0.0,
                fee_quote_24h: 0.0,
                deposit_quote_24h: 0.0,
                withdraw_quote_24h: 0.0,
                net_liquidity_delta_quote_24h: 0.0,
                claim_quote_24h: 0.0,
                avg_update_interval_secs_24h: None,
                latest_update_at_24h: None,
                deposit_count_24h: 0,
                withdraw_count_24h: 0,
                claim_count_24h: 0,
                update_count_24h: 0,
            });
            summary.avg_update_interval_secs_24h = avg;
            summary.latest_update_at_24h = latest;
        }

        Ok(out)
    }

    fn recent_pool_events_since(&self, pool_address: &str, since_ts: i64) -> Result<Vec<PoolEventRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT event_id, tx_hash, ledger, created_at, pool_address, kind, body_json
            FROM pool_events
            WHERE pool_address = ?1 AND created_at >= ?2
            ORDER BY created_at DESC, ledger DESC, id DESC
            "#,
        )?;
        let rows = stmt.query_map(params![pool_address, since_ts], map_event_row)?;
        collect_event_rows(rows)
    }

    fn recent_all_pool_events_since(&self, since_ts: i64) -> Result<Vec<PoolEventRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT event_id, tx_hash, ledger, created_at, pool_address, kind, body_json
            FROM pool_events
            WHERE created_at >= ?1
            ORDER BY created_at DESC, ledger DESC, id DESC
            "#,
        )?;
        let rows = stmt.query_map(params![since_ts], map_event_row)?;
        collect_event_rows(rows)
    }
}

fn map_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PoolEventRow> {
    let body_json: String = row.get(6)?;
    let body = serde_json::from_str(&body_json).unwrap_or(Value::Null);
    Ok(PoolEventRow {
        event_id: row.get(0)?,
        tx_hash: row.get(1)?,
        ledger: row.get::<_, i64>(2)? as u32,
        created_at: row.get(3)?,
        pool_address: row.get(4)?,
        kind: row.get(5)?,
        body,
    })
}

fn collect_event_rows(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<PoolEventRow>>,
) -> Result<Vec<PoolEventRow>> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn map_copy_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CopySessionRow> {
    let allowed_pools_json: String = row.get(7)?;
    Ok(CopySessionRow {
        id: row.get(0)?,
        contract_session_id: row.get(1)?,
        follower_address: row.get(2)?,
        leader_address: row.get(3)?,
        coefficient: row.get(4)?,
        status: row.get(5)?,
        include_claims: row.get::<_, i64>(6)? != 0,
        allowed_pools: serde_json::from_str(&allowed_pools_json).unwrap_or_default(),
        max_per_op_quote_xlm: row.get(8)?,
        max_daily_quote_xlm: row.get(9)?,
        expires_at: row.get(10)?,
        cursor_ts: row.get(11)?,
        watermark_ts: row.get(12)?,
        watermark_event_id: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

fn collect_copy_session_rows(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<CopySessionRow>>,
) -> Result<Vec<CopySessionRow>> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn map_copy_op_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CopyOpRow> {
    Ok(CopyOpRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        source_event_id: row.get(2)?,
        pool_address: row.get(3)?,
        kind: row.get(4)?,
        position_key: row.get(5)?,
        leader_amounts_json: row.get(6)?,
        scaled_amounts_json: row.get(7)?,
        leader_quote_xlm: row.get(8)?,
        scaled_quote_xlm: row.get(9)?,
        status: row.get(10)?,
        note: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn collect_copy_op_rows(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<CopyOpRow>>,
) -> Result<Vec<CopyOpRow>> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn new_copy_id() -> String {
    let ts = chrono::Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| chrono::Utc::now().timestamp().saturating_mul(1_000_000_000));
    format!("{ts:x}")
}

fn derived_number(body: &Value, key: &str) -> f64 {
    body.get("derived")
        .and_then(|v| v.get(key))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

fn cadence_from_times(times: &mut [i64]) -> (Option<f64>, Option<i64>) {
    if times.is_empty() {
        return (None, None);
    }
    times.sort_unstable_by(|a, b| b.cmp(a));
    let latest = Some(times[0]);
    if times.len() < 2 {
        return (None, latest);
    }
    let mut total = 0i64;
    for pair in times.windows(2) {
        total += pair[0] - pair[1];
    }
    let avg = total as f64 / (times.len() - 1) as f64;
    (Some(avg), latest)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> IndexDb {
        IndexDb::open(":memory:").expect("open in-memory db")
    }

    #[test]
    fn migrate_creates_copy_tables() {
        let db = test_db();
        assert!(db.table_exists("copy_sessions"));
        assert!(db.table_exists("copy_ops"));
        assert!(db.table_exists("recorder_outbox"));
        assert!(db.table_exists("token_metadata"));
    }

    #[test]
    fn copy_status_transitions_are_fail_closed() {
        assert!(copy_status_transition_allowed("pending", "drafted"));
        assert!(copy_status_transition_allowed("drafted", "signed"));
        assert!(copy_status_transition_allowed("insufficient", "drafted"));
        assert!(!copy_status_transition_allowed("rejected", "signed"));
        assert!(!copy_status_transition_allowed("signed", "drafted"));
    }

    #[test]
    fn recorder_outbox_is_idempotent_and_readable() {
        let db = test_db();
        let event = RecorderEvent {
            source_event_id: "evt-1".into(),
            leader_address: "GLEADER".into(),
            pool_address: "CPOOL".into(),
            kind: "deposit".into(),
            amounts: vec![100, 200],
            quote_stroops: 129_000_000,
            ledger: 123,
            created_at: 456,
        };
        assert!(db.enqueue_recorder_event(&event).unwrap());
        assert!(!db.enqueue_recorder_event(&event).unwrap());
        let pending = db.pending_recorder_events(10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].quote_stroops, 129_000_000);
        assert_eq!(pending[0].amounts, vec![100, 200]);

        let claimed = db.claim_recorder_events(10, 300).unwrap();
        assert_eq!(claimed[0].status, "processing");
        assert_eq!(claimed[0].attempts, 1);
        db.update_recorder_event("evt-1", "submitted", None).unwrap();
        assert!(db.pending_recorder_events(10).unwrap().is_empty());
    }

    #[test]
    fn recorder_claim_recovers_expired_processing_lease() {
        let db = test_db();
        let event = RecorderEvent {
            source_event_id: "evt-lease".into(),
            leader_address: "GLEADER".into(),
            pool_address: "CPOOL".into(),
            kind: "deposit".into(),
            amounts: vec![1],
            quote_stroops: 1,
            ledger: 1,
            created_at: 1,
        };
        db.enqueue_recorder_event(&event).unwrap();
        db.claim_recorder_events(1, 30).unwrap();
        db.conn
            .execute(
                "UPDATE recorder_outbox SET updated_at = 1 WHERE source_event_id = 'evt-lease'",
                [],
            )
            .unwrap();
        let retry = db.claim_recorder_events(1, 30).unwrap();
        assert_eq!(retry.len(), 1);
        assert_eq!(retry[0].attempts, 2);
        assert_eq!(retry[0].status, "processing");
    }

    #[test]
    fn token_metadata_round_trips_through_persistent_store() {
        let db = test_db();
        let metadata = TokenMetadataRow {
            address: "CTOKEN".into(),
            symbol: "AQUA".into(),
            name: Some("Aquarius Token".into()),
            issuer: Some("GISSUER".into()),
            domain: Some("aquarius.io".into()),
            icon: Some("https://example.com/aqua.svg".into()),
        };
        db.upsert_token_metadata(&metadata).unwrap();
        assert_eq!(db.token_metadata(&metadata.address).unwrap().unwrap().symbol, "AQUA");
    }

    #[test]
    fn fee_snapshot_total_keeps_unknown_fees_unavailable() {
        let db = test_db();
        db.upsert_actor_fee_snapshot("GLEADER", "CUNKNOWN", "soroswap", None, Some(100.0), "ok", 10)
            .unwrap();
        db.upsert_actor_fee_snapshot("GLEADER", "CKNOWN", "aquarius", Some(2.5), Some(50.0), "ok", 20)
            .unwrap();

        let total = db.actor_fee_snapshot_totals().unwrap().remove("GLEADER").unwrap();
        assert_eq!(total.unclaimed_quote_xlm, 2.5);
        assert_eq!(total.position_count, 2);
        assert_eq!(total.pool_count, 2);
        assert_eq!(total.observed_at, Some(20));
    }

    #[test]
    fn fee_snapshot_delta_uses_window_boundary_per_pool() {
        let db = test_db();
        db.insert_actor_fee_snapshot_history("GLEADER", "CPOOL1", "aquarius", Some(10.0), Some(100.0), "ok", 100)
            .unwrap();
        db.insert_actor_fee_snapshot_history("GLEADER", "CPOOL2", "aquarius", Some(3.0), Some(50.0), "ok", 100)
            .unwrap();
        db.upsert_actor_fee_snapshot("GLEADER", "CPOOL1", "aquarius", Some(14.0), Some(110.0), "ok", 200)
            .unwrap();
        db.upsert_actor_fee_snapshot("GLEADER", "CPOOL2", "aquarius", Some(5.5), Some(55.0), "ok", 200)
            .unwrap();

        let deltas = db.actor_fee_snapshot_deltas(100).unwrap();
        assert_eq!(deltas.get("GLEADER"), Some(&6.5));
    }

    #[test]
    fn fee_snapshot_delta_is_unavailable_without_window_boundary() {
        let db = test_db();
        db.insert_actor_fee_snapshot_history("GLEADER", "CPOOL", "aquarius", Some(10.0), Some(100.0), "ok", 200)
            .unwrap();
        db.upsert_actor_fee_snapshot("GLEADER", "CPOOL", "aquarius", Some(14.0), Some(110.0), "ok", 300)
            .unwrap();

        let deltas = db.actor_fee_snapshot_deltas(100).unwrap();
        assert!(!deltas.contains_key("GLEADER"));
    }

    #[test]
    fn fee_snapshot_delta_is_unavailable_for_unverified_fee_legs() {
        let db = test_db();
        db.insert_actor_fee_snapshot_history(
            "GLEADER",
            "CPOOL",
            "soroswap",
            None,
            Some(100.0),
            "fee_unavailable",
            100,
        )
        .unwrap();
        db.upsert_actor_fee_snapshot("GLEADER", "CPOOL", "soroswap", Some(14.0), Some(110.0), "ok", 200)
            .unwrap();

        let deltas = db.actor_fee_snapshot_deltas(100).unwrap();
        assert!(!deltas.contains_key("GLEADER"));
    }

    #[test]
    fn actor_fee_snapshot_delta_scans_only_requested_actor() {
        let db = test_db();
        db.insert_actor_fee_snapshot_history("GLEADER", "CPOOL", "aquarius", Some(10.0), Some(100.0), "ok", 100)
            .unwrap();
        db.insert_actor_fee_snapshot_history("GOTHER", "CPOOL", "aquarius", Some(99.0), Some(100.0), "ok", 100)
            .unwrap();
        db.upsert_actor_fee_snapshot("GLEADER", "CPOOL", "aquarius", Some(13.5), Some(110.0), "ok", 200)
            .unwrap();
        db.upsert_actor_fee_snapshot("GOTHER", "CPOOL", "aquarius", Some(120.0), Some(110.0), "ok", 200)
            .unwrap();

        assert_eq!(db.actor_fee_snapshot_delta("GLEADER", 100).unwrap(), Some(3.5));
    }

    #[test]
    fn actor_fee_snapshot_delta_handles_closed_positions() {
        let db = test_db();
        db.upsert_actor_fee_snapshot("GLEADER", "CPOOL", "aquarius", Some(10.0), Some(100.0), "ok", 100)
            .unwrap();
        db.insert_actor_fee_snapshot_history("GLEADER", "CPOOL", "aquarius", Some(10.0), Some(100.0), "ok", 100)
            .unwrap();
        db.record_actor_fee_snapshot_history_zeroed("GLEADER", 200).unwrap();
        db.clear_actor_fee_snapshots("GLEADER").unwrap();

        assert_eq!(db.actor_fee_snapshot_delta("GLEADER", 150).unwrap(), Some(-10.0));
    }

    #[test]
    fn zeroed_history_preserves_unverified_fee_status() {
        let db = test_db();
        db.upsert_actor_fee_snapshot(
            "GLEADER",
            "CPOOL",
            "soroswap",
            None,
            Some(100.0),
            "fee_unavailable",
            100,
        )
        .unwrap();
        db.record_actor_fee_snapshot_history_zeroed("GLEADER", 200).unwrap();
        db.clear_actor_fee_snapshots("GLEADER").unwrap();
        db.upsert_actor_fee_snapshot("GLEADER", "CPOOL", "soroswap", Some(4.0), Some(90.0), "ok", 300)
            .unwrap();

        let deltas = db.actor_fee_snapshot_deltas(200).unwrap();
        assert!(!deltas.contains_key("GLEADER"));
    }

    #[test]
    fn insert_copy_op_is_idempotent() {
        let db = test_db();
        let session = db
            .create_copy_session("GFOLLOWER", "GLEADER", 0.5, false, &[], 0.0, 0.0, None, None)
            .unwrap();
        let op = CopyOpRow {
            id: "op-1".into(),
            session_id: session.id.clone(),
            source_event_id: "evt-1".into(),
            pool_address: "CPOOL".into(),
            kind: "deposit".into(),
            position_key: "cp:CPOOL".into(),
            leader_amounts_json: r#"[{"token":"CA","amount":"1000"}]"#.into(),
            scaled_amounts_json: r#"[{"token":"CA","amount":"500"}]"#.into(),
            leader_quote_xlm: Some(10.0),
            scaled_quote_xlm: Some(5.0),
            status: "pending".into(),
            note: None,
            created_at: 1,
            updated_at: 1,
        };
        assert!(db.insert_copy_op(&op).unwrap());
        assert!(!db.insert_copy_op(&op).unwrap());
        assert_eq!(db.list_copy_ops(&session.id, None).unwrap().len(), 1);
    }

    #[test]
    fn create_copy_session_pauses_prior_active_pair() {
        let db = test_db();
        let first = db
            .create_copy_session("GFOLLOWER", "GLEADER", 1.0, false, &[], 0.0, 0.0, None, None)
            .unwrap();
        assert_eq!(first.status, "active");

        let second = db
            .create_copy_session("GFOLLOWER", "GLEADER", 0.5, true, &[], 0.0, 0.0, None, None)
            .unwrap();
        assert_eq!(second.status, "active");
        assert_eq!(db.get_copy_session(&first.id).unwrap().unwrap().status, "paused");
    }

    #[test]
    fn events_for_actor_since_filters_by_actor_and_timestamp() {
        let db = test_db();
        db.ensure_pool_events_table_for_test().unwrap();
        db.insert_pool_event_for_test(
            "evt-old",
            100,
            "deposit_liquidity",
            r#"{"derived":{"actor":"GLEADER"}}"#,
        )
        .unwrap();
        db.insert_pool_event_for_test(
            "evt-new",
            200,
            "deposit_liquidity",
            r#"{"derived":{"actor":"GLEADER"}}"#,
        )
        .unwrap();
        db.insert_pool_event_for_test(
            "evt-other",
            300,
            "deposit_liquidity",
            r#"{"derived":{"actor":"GOTHER"}}"#,
        )
        .unwrap();

        let page1 = db.events_for_actor_since("GLEADER", 100, "", 10).unwrap();
        assert_eq!(page1.len(), 1);
        assert_eq!(page1[0].event_id, "evt-new");
    }

    #[test]
    fn events_for_actor_since_pages_same_timestamp_by_event_id() {
        let db = test_db();
        db.ensure_pool_events_table_for_test().unwrap();
        db.insert_pool_event_for_test(
            "evt-same-a",
            200,
            "deposit_liquidity",
            r#"{"derived":{"actor":"GLEADER"}}"#,
        )
        .unwrap();
        db.insert_pool_event_for_test(
            "evt-same-b",
            200,
            "deposit_liquidity",
            r#"{"derived":{"actor":"GLEADER"}}"#,
        )
        .unwrap();
        let page2 = db.events_for_actor_since("GLEADER", 200, "evt-same-a", 10).unwrap();
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].event_id, "evt-same-b");
    }

    #[test]
    fn sushi_claim_amount_is_not_counted_as_fee() {
        let db = test_db();
        db.ensure_pool_events_table_for_test().unwrap();
        let actor = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        db.insert_pool_event_for_test(
            "evt-sushi-claim",
            200,
            "claim_fees",
            &format!(r#"{{"derived":{{"actor":"{actor}","venue":"sushi","total_quote_xlm":999999.0}}}}"#),
        )
        .unwrap();

        let leaders = db.top_liquidity_actors(100, 10, "fees").unwrap();
        let leader = leaders.iter().find(|row| row.address == actor).unwrap();
        assert_eq!(leader.claim_count, 1);
        assert_eq!(leader.claim_quote_xlm, 0.0);
    }
}

#[cfg(test)]
impl IndexDb {
    fn table_exists(&self, name: &str) -> bool {
        self.conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![name],
                |_| Ok(()),
            )
            .is_ok()
    }

    fn ensure_pool_events_table_for_test(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
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
            "#,
        )?;
        Ok(())
    }

    fn insert_pool_event_for_test(&self, event_id: &str, created_at: i64, kind: &str, body_json: &str) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO pool_events (
              event_id, tx_hash, ledger, created_at, pool_address, kind, body_json
            ) VALUES (?1, NULL, 1, ?2, 'CPOOL', ?3, ?4)
            "#,
            params![event_id, created_at, kind, body_json],
        )?;
        Ok(())
    }
}
