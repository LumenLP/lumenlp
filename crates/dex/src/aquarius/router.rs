//! Discover Aquarius pools from the on-chain router.

use {
    crate::aquarius::AQUARIUS_ROUTER,
    crate::rpc::{scval_to_address, SorobanRpc},
    anyhow::Result,
    std::collections::HashSet,
    stellar_xdr::curr as xdr,
    tracing::{info, warn},
};

pub async fn discover_pool_addresses(rpc: &SorobanRpc) -> Result<Vec<String>> {
    let count_val = rpc
        .call_no_args(AQUARIUS_ROUTER, "get_tokens_sets_count")
        .await?;
    let total_count = crate::rpc::scval_to_u128(&count_val)?;
    info!(total_count, "Aquarius router token sets");
    if total_count == 0 {
        return Ok(vec![]);
    }

    let mut pool_addresses = HashSet::new();
    let batch_size: u128 = 50;
    let mut start: u128 = 0;

    while start < total_count {
        let end = (start + batch_size).min(total_count);
        let start_val = xdr::ScVal::U128(xdr::UInt128Parts {
            hi: (start >> 64) as u64,
            lo: start as u64,
        });
        let end_val = xdr::ScVal::U128(xdr::UInt128Parts {
            hi: (end >> 64) as u64,
            lo: end as u64,
        });

        match rpc
            .simulate_call(
                AQUARIUS_ROUTER,
                "get_pools_for_tokens_range",
                vec![start_val, end_val],
            )
            .await
        {
            Ok(result) => collect_pool_addresses(&result, &mut pool_addresses),
            Err(e) => warn!(start, end, error = %e, "router batch failed"),
        }

        start = end;
        if start < total_count {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    let mut out: Vec<String> = pool_addresses.into_iter().collect();
    out.sort();
    info!(count = out.len(), "discovered Aquarius pools");
    Ok(out)
}

fn collect_pool_addresses(val: &xdr::ScVal, out: &mut HashSet<String>) {
    let xdr::ScVal::Vec(Some(entries)) = val else {
        return;
    };
    for entry in entries.0.iter() {
        let xdr::ScVal::Vec(Some(pair)) = entry else {
            continue;
        };
        if pair.0.len() < 2 {
            continue;
        }
        let xdr::ScVal::Map(Some(map)) = &pair.0[1] else {
            continue;
        };
        for map_entry in map.0.iter() {
            if let Ok(addr) = scval_to_address(&map_entry.val) {
                out.insert(addr);
            }
        }
    }
}
