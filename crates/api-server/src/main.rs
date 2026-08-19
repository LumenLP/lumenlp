mod copy_lp;
mod copy_policy;
mod handlers;
mod index_db;
mod pricing;
mod token_registry;

use {
    anyhow::Result,
    dex::{db::Db, SorobanRpc, MAINNET_PASSPHRASE},
    axum::Router,
    handlers::AppState,
    index_db::IndexDb,
    pricing::service::PriceService,
    std::{
        collections::HashMap,
        net::SocketAddr,
        sync::{Arc, Mutex},
    },
    tower_http::cors::{Any, CorsLayer},
    tracing::info,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let rpc_url = std::env::var("RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8003".into());
    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "./data/lumenlp.db".into());
    let index_db_path =
        std::env::var("INDEXER_DB_PATH").unwrap_or_else(|_| "./data/pool-indexer.db".into());
    let bind = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());

    let rpc = Arc::new(SorobanRpc::new(&rpc_url, MAINNET_PASSPHRASE));
    let db = Arc::new(Mutex::new(Db::open(&db_path)?));
    let index_db = Arc::new(Mutex::new(IndexDb::open(&index_db_path)?));
    let token_meta_cache = Arc::new(Mutex::new(HashMap::new()));
    let prices = Arc::new(PriceService::new());
    let redis = std::env::var("REDIS_URL")
        .ok()
        .and_then(|url| redis::Client::open(url).ok());
    let state = AppState {
        rpc,
        db,
        index_db,
        token_meta_cache,
        prices,
        pool_list_cache: Arc::new(Mutex::new(None)),
        redis,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let warm_state = state.clone();
    let app = Router::new()
        .merge(handlers::router())
        .layer(cors)
        .with_state(state);

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        handlers::warm_pool_list_cache(warm_state).await;
        info!("pool list cache warmed");
    });

    let addr: SocketAddr = bind.parse()?;
    info!(%addr, %rpc_url, %db_path, %index_db_path, "api-server listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
