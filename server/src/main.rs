mod app;
mod config;
mod controller;
mod database;
mod error;
mod id;
mod schema;
mod service;

use std::net::SocketAddr;

use error::AppResult;

#[tokio::main]
async fn main() -> AppResult<()> {
    let config = config::AppConfig::load()?;
    init_tracing(&config.log_level)?;

    let address: SocketAddr = config.bind_address()?;
    let state = app::AppState::new(config.clone()).await?;
    let app = app::build_app(state);
    let listener = tokio::net::TcpListener::bind(address).await?;

    tracing::info!(
        service = app::APP_NAME,
        address = %address,
        database = %database::database_kind(&config.database_url),
        public_dir = %config.public_dir,
        "starting server"
    );

    axum::serve(listener, app).await?;
    Ok(())
}

fn init_tracing(log_level: &str) -> AppResult<()> {
    let filter = tracing_subscriber::EnvFilter::try_new(log_level)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .try_init()
        .map_err(|source| error::AppError::Internal {
            message: format!("failed to initialize tracing: {source}"),
        })?;

    Ok(())
}
