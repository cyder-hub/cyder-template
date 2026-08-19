mod app;
mod cli;
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

use std::{env, net::SocketAddr, process::ExitCode};

use serde::Serialize;

use error::{AppError, AppResult};

type MainResult<T> = Result<T, MainError>;

#[derive(Debug, thiserror::Error)]
enum MainError {
    #[error(transparent)]
    Application(#[from] AppError),
    #[error("command-line error: {0}")]
    CommandLine(#[from] cli::ParseError),
}

#[derive(Debug, Serialize)]
struct ConfigEndpoint {
    host: String,
    port: u16,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> MainResult<()> {
    match cli::parse(env::args_os().skip(1))? {
        cli::Command::Serve => serve().await?,
        cli::Command::ConfigEndpointJson => print_config_endpoint()?,
        cli::Command::Help => {
            print!("{}", cli::HELP);
        }
    }
    Ok(())
}

async fn serve() -> AppResult<()> {
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

fn print_config_endpoint() -> AppResult<()> {
    let config = config::AppConfig::load()?;
    config.validate()?;
    let address = config.bind_address()?;
    let endpoint = ConfigEndpoint {
        host: address.ip().to_string(),
        port: address.port(),
    };
    let output = serde_json::to_string(&endpoint).map_err(|source| AppError::Internal {
        message: format!("failed to serialize configuration endpoint: {source}"),
    })?;
    println!("{output}");
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
