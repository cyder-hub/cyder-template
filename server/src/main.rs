mod app;
mod config;
mod controller;
mod database;
mod error;
mod id;
mod schema;
// template-example:start
mod service;
// template-example:end
mod shutdown;

use std::net::SocketAddr;

use error::AppResult;

#[tokio::main]
async fn main() -> AppResult<()> {
    let config = config::AppConfig::load()?;
    config.validate()?;
    init_tracing(&config.log_level)?;

    let address: SocketAddr = config.bind_address()?;
    let state = app::AppState::new(config.clone()).await?;
    let lifecycle = state.lifecycle().clone();
    let app = app::build_app(state);
    let listener = tokio::net::TcpListener::bind(address).await?;

    tracing::info!(
        service = app::APP_NAME,
        address = %address,
        database = %database::database_kind(&config.database_url),
        public_dir = %config.public_dir,
        "starting server"
    );

    shutdown::serve(listener, app, lifecycle, &config).await
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
