mod app;
mod cli;
mod config;
mod controller;
mod database;
mod error;
mod http_middleware;
mod id;
mod schema;
mod shutdown;

use std::{
    env,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    process::{Command as ProcessCommand, ExitCode},
};

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
    http_middleware::install_redacting_panic_hook();

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
        cli::Command::ConfigCheck { format } => check_config(format)?,
        cli::Command::Healthcheck => healthcheck()?,
        cli::Command::Help => {
            print!("{}", cli::HELP);
        }
    }
    Ok(())
}

fn healthcheck() -> AppResult<()> {
    let resolved = config::AppConfig::load(config::ConfigLoadMode::Runtime)?;
    emit_config_warnings(&resolved.warnings);
    let bind_address = resolved.config.bind_address()?;
    let probe_ip = match bind_address.ip() {
        IpAddr::V4(address) if address.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(address) if address.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        address => address,
    };
    let probe_host = match probe_ip {
        IpAddr::V4(address) => address.to_string(),
        IpAddr::V6(address) => format!("[{address}]"),
    };
    let url = format!("http://{probe_host}:{}/readyz", bind_address.port());
    let status = ProcessCommand::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "4",
            "--output",
            "/dev/null",
            &url,
        ])
        .status()
        .map_err(|source| AppError::Internal {
            message: format!("failed to execute readiness probe: {source}"),
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(AppError::Readiness {
            message: format!("GET {url} exited with {status}"),
        })
    }
}

async fn serve() -> AppResult<()> {
    let resolved = config::AppConfig::load(config::ConfigLoadMode::Runtime)?;
    let config = resolved.config;
    init_tracing(&config.log_level)?;
    log_config_warnings(&resolved.warnings);
    log_config_summary(&resolved.summary);

    let address: SocketAddr = config.bind_address()?;
    let state = app::AppState::new(config.clone()).await?;
    let lifecycle = state.lifecycle().clone();
    let app = app::build_app(state);
    let listener = tokio::net::TcpListener::bind(address).await?;

    tracing::info!(
        service = app::APP_NAME,
        address = %address,
        database = %config.database_kind,
        "starting server"
    );

    shutdown::serve(listener, app, lifecycle, &config).await
}

fn print_config_endpoint() -> AppResult<()> {
    let resolved = config::AppConfig::load(config::ConfigLoadMode::Runtime)?;
    emit_config_warnings(&resolved.warnings);
    let config = resolved.config;
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

fn check_config(format: cli::OutputFormat) -> AppResult<()> {
    let resolved = config::AppConfig::load(config::ConfigLoadMode::Check)?;
    emit_config_warnings(&resolved.warnings);
    match format {
        cli::OutputFormat::Text => print!("{}", resolved.summary),
        cli::OutputFormat::Json => {
            let output =
                serde_json::to_string(&resolved.summary).map_err(|source| AppError::Internal {
                    message: format!("failed to serialize safe configuration summary: {source}"),
                })?;
            println!("{output}");
        }
    }
    Ok(())
}

fn emit_config_warnings(warnings: &[config::ConfigWarning]) {
    for warning in warnings {
        eprintln!(
            "configuration warning [{}] {} from {}: {}",
            warning.code, warning.key, warning.source, warning.message
        );
    }
}

fn log_config_warnings(warnings: &[config::ConfigWarning]) {
    for warning in warnings {
        tracing::warn!(
            event = "configuration_warning",
            code = %warning.code,
            key = %warning.key,
            source = %warning.source,
            message = %warning.message,
            "configuration warning"
        );
    }
}

fn log_config_summary(summary: &config::ConfigSummary) {
    tracing::info!(
        event = "configuration_resolved",
        data_dir = %summary.data_dir.display(),
        config_file = %summary.config_file,
        host = %summary.host,
        port = summary.port,
        database = %summary.database_kind,
        database_pool_size = summary.database_pool_size,
        database_acquire_timeout_ms = summary.database_acquire_timeout_ms,
        sqlite_busy_timeout_ms = summary.sqlite_busy_timeout_ms,
        log_level = %summary.log_level,
        shutdown_readiness_delay_ms = summary.shutdown_readiness_delay_ms,
        shutdown_timeout_ms = summary.shutdown_timeout_ms,
        http_request_timeout_ms = summary.http_request_timeout_ms,
        http_max_concurrent_requests = summary.http_max_concurrent_requests,
        http_max_request_body_bytes = summary.http_max_request_body_bytes,
        warnings = summary.warnings.len(),
        "resolved application configuration"
    );
}

fn init_tracing(log_level: &str) -> AppResult<()> {
    let filter = tracing_subscriber::EnvFilter::try_new(log_level).map_err(|source| {
        error::AppError::Internal {
            message: format!("validated tracing filter could not be initialized: {source}"),
        }
    })?;

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(filter)
        .try_init()
        .map_err(|source| error::AppError::Internal {
            message: format!("failed to initialize tracing: {source}"),
        })?;

    Ok(())
}
