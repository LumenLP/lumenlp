use {
    crate::db::IndexDb,
    anyhow::Result,
    tracing::{info, warn},
};

pub fn sync_derived_tables(db: &IndexDb) -> Result<()> {
    let snapshot_rows = match db.backfill_5m_from_hourly_snapshots() {
        Ok(rows) => rows,
        Err(error) if error.to_string().contains("no such table") => {
            warn!(%error, "source snapshot tables missing; skipping 5m snapshot sync");
            0
        }
        Err(error) => return Err(error),
    };
    let rollup_rows = db.rebuild_rollups()?;
    info!(
        snapshot_rows,
        rollup_rows, "synced derived pool index tables"
    );
    Ok(())
}
