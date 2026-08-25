use std::{
    collections::{BTreeMap, HashMap},
    env, fmt, fs,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
};

use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

const DEFAULT_DATA_DIR: &str = ".app/dev";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 8_000;
const DEFAULT_DATABASE_POOL_SIZE_SQLITE: u32 = 1;
const DEFAULT_DATABASE_POOL_SIZE_POSTGRES: u32 = 5;
const DEFAULT_DATABASE_ACQUIRE_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_SQLITE_BUSY_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_LOG_LEVEL: &str = "info";
const DEFAULT_SHUTDOWN_READINESS_DELAY_MS: u64 = 1_000;
const DEFAULT_SHUTDOWN_TIMEOUT_MS: u64 = 8_000;
const DEFAULT_HTTP_REQUEST_TIMEOUT_MS: u64 = 30_000;
const MIN_HTTP_REQUEST_TIMEOUT_MS: u64 = 1;
const MAX_HTTP_REQUEST_TIMEOUT_MS: u64 = 300_000;
const DEFAULT_HTTP_MAX_CONCURRENT_REQUESTS: u32 = 64;
const MIN_HTTP_MAX_CONCURRENT_REQUESTS: u32 = 1;
const MAX_HTTP_MAX_CONCURRENT_REQUESTS: u32 = 4_096;
const DEFAULT_HTTP_MAX_REQUEST_BODY_BYTES: u64 = 1_048_576;
const MIN_HTTP_MAX_REQUEST_BODY_BYTES: u64 = 1;
const MAX_HTTP_MAX_REQUEST_BODY_BYTES: u64 = 67_108_864;
const DEFAULT_CONFIG_RELATIVE_PATH: &str = "config/config.yaml";
const DEFAULT_DATABASE_RELATIVE_PATH: &str = "db/cyder-template.sqlite";

const RUNTIME_ENVIRONMENT_KEYS: &[&str] =
    &["APP_HOST", "APP_PORT", "APP_DATABASE_URL", "APP_LOG_LEVEL"];

const BOOTSTRAP_ENVIRONMENT_KEYS: &[&str] = &["APP_DATA_DIR", "APP_CONFIG_PATH"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseKind {
    Postgres,
    Sqlite,
}

impl fmt::Display for DatabaseKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Postgres => formatter.write_str("postgres"),
            Self::Sqlite => formatter.write_str("sqlite"),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct DatabaseUrl(String);

impl DatabaseUrl {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[cfg(test)]
    pub fn sqlite_memory() -> Self {
        Self(":memory:".to_string())
    }

    fn parse(value: String) -> Result<(Self, DatabaseKind), ConfigValidationError> {
        validate_non_empty_string("database_url", &value)?;

        if value == ":memory:" || value.starts_with("file:") {
            return Ok((Self(value), DatabaseKind::Sqlite));
        }

        let lower = value.to_ascii_lowercase();
        if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
            let parsed = value
                .parse::<tokio_postgres::Config>()
                .map_err(|_| ConfigValidationError::InvalidDatabaseUrlSyntax)?;
            if parsed.get_dbname().is_none_or(str::is_empty) {
                return Err(ConfigValidationError::MissingPostgresDatabaseName);
            }
            return Ok((Self(value), DatabaseKind::Postgres));
        }

        if lower.starts_with("postgres:")
            || lower.starts_with("postgresql:")
            || value.contains("://")
        {
            return Err(ConfigValidationError::UnsupportedDatabaseUrlScheme);
        }

        Ok((Self(value), DatabaseKind::Sqlite))
    }
}

impl fmt::Debug for DatabaseUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseUrl([REDACTED])")
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub host: String,
    pub port: u16,
    pub database_url: DatabaseUrl,
    pub database_kind: DatabaseKind,
    pub database_pool_size: u32,
    pub database_acquire_timeout_ms: u64,
    pub sqlite_busy_timeout_ms: u64,
    pub log_level: String,
    pub shutdown_readiness_delay_ms: u64,
    pub shutdown_timeout_ms: u64,
    pub http_request_timeout_ms: u64,
    pub http_max_concurrent_requests: u32,
    pub http_max_request_body_bytes: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        let data_dir = PathBuf::from(DEFAULT_DATA_DIR);
        Self {
            database_url: DatabaseUrl(
                data_dir
                    .join(DEFAULT_DATABASE_RELATIVE_PATH)
                    .to_string_lossy()
                    .into_owned(),
            ),
            data_dir,
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            database_kind: DatabaseKind::Sqlite,
            database_pool_size: DEFAULT_DATABASE_POOL_SIZE_SQLITE,
            database_acquire_timeout_ms: DEFAULT_DATABASE_ACQUIRE_TIMEOUT_MS,
            sqlite_busy_timeout_ms: DEFAULT_SQLITE_BUSY_TIMEOUT_MS,
            log_level: DEFAULT_LOG_LEVEL.to_string(),
            shutdown_readiness_delay_ms: DEFAULT_SHUTDOWN_READINESS_DELAY_MS,
            shutdown_timeout_ms: DEFAULT_SHUTDOWN_TIMEOUT_MS,
            http_request_timeout_ms: DEFAULT_HTTP_REQUEST_TIMEOUT_MS,
            http_max_concurrent_requests: DEFAULT_HTTP_MAX_CONCURRENT_REQUESTS,
            http_max_request_body_bytes: DEFAULT_HTTP_MAX_REQUEST_BODY_BYTES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigLoadMode {
    Runtime,
    Check,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConfigWarning {
    pub code: String,
    pub source: String,
    pub key: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConfigFileSource {
    None,
    Default { path: PathBuf },
    Explicit { path: PathBuf },
}

impl fmt::Display for ConfigFileSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str("none"),
            Self::Default { path } => write!(formatter, "default:{}", path.display()),
            Self::Explicit { path } => write!(formatter, "explicit:{}", path.display()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigSummary {
    pub valid: bool,
    pub data_dir: PathBuf,
    pub config_file: ConfigFileSource,
    pub host: String,
    pub port: u16,
    pub database_kind: DatabaseKind,
    pub database_pool_size: u32,
    pub database_acquire_timeout_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sqlite_busy_timeout_ms: Option<u64>,
    pub log_level: String,
    pub shutdown_readiness_delay_ms: u64,
    pub shutdown_timeout_ms: u64,
    pub http_request_timeout_ms: u64,
    pub http_max_concurrent_requests: u32,
    pub http_max_request_body_bytes: u64,
    pub warnings: Vec<ConfigWarning>,
}

impl fmt::Display for ConfigSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "configuration: valid")?;
        writeln!(formatter, "data_dir: {}", self.data_dir.display())?;
        writeln!(formatter, "config_file: {}", self.config_file)?;
        writeln!(formatter, "listen: {}:{}", self.host, self.port)?;
        writeln!(formatter, "database_kind: {}", self.database_kind)?;
        writeln!(formatter, "database_pool_size: {}", self.database_pool_size)?;
        writeln!(
            formatter,
            "database_acquire_timeout_ms: {}",
            self.database_acquire_timeout_ms
        )?;
        if let Some(timeout) = self.sqlite_busy_timeout_ms {
            writeln!(formatter, "sqlite_busy_timeout_ms: {timeout}")?;
        }
        writeln!(formatter, "log_level: {}", self.log_level)?;
        writeln!(
            formatter,
            "shutdown_readiness_delay_ms: {}",
            self.shutdown_readiness_delay_ms
        )?;
        writeln!(
            formatter,
            "shutdown_timeout_ms: {}",
            self.shutdown_timeout_ms
        )?;
        writeln!(
            formatter,
            "http_request_timeout_ms: {}",
            self.http_request_timeout_ms
        )?;
        writeln!(
            formatter,
            "http_max_concurrent_requests: {}",
            self.http_max_concurrent_requests
        )?;
        writeln!(
            formatter,
            "http_max_request_body_bytes: {}",
            self.http_max_request_body_bytes
        )?;
        writeln!(formatter, "warnings: {}", self.warnings.len())
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub config: AppConfig,
    pub summary: ConfigSummary,
    pub warnings: Vec<ConfigWarning>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{message}")]
    InvalidBootstrap { message: String },
    #[error("failed to inspect configuration path: {source}")]
    InspectConfigPath {
        #[source]
        source: std::io::Error,
    },
    #[error("configuration path is not a regular file")]
    ConfigPathNotFile,
    #[error("failed to read configuration file: {source}")]
    ReadConfigFile {
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse application configuration: {source}")]
    Parse {
        #[source]
        source: config::ConfigError,
    },
    #[error("invalid application configuration: {source}")]
    Validation {
        #[from]
        source: ConfigValidationError,
    },
    #[error("configuration check rejected: {issues}")]
    CheckRejected { issues: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("{field} must not be empty or contain surrounding whitespace")]
    InvalidString { field: &'static str },
    #[error("port must be between 1 and 65535")]
    InvalidPort,
    #[error("host must be an IP address")]
    InvalidHost,
    #[error("database_url contains an unsupported URL scheme")]
    UnsupportedDatabaseUrlScheme,
    #[error("database_url contains invalid PostgreSQL URL syntax")]
    InvalidDatabaseUrlSyntax,
    #[error("database_url must name a PostgreSQL database")]
    MissingPostgresDatabaseName,
    #[error("database_pool_size must be greater than 0")]
    ZeroDatabasePoolSize,
    #[error("plain in-memory SQLite requires database_pool_size=1")]
    InvalidMemoryDatabasePoolSize,
    #[error("database_acquire_timeout_ms must be greater than 0")]
    ZeroDatabaseAcquireTimeout,
    #[error("shutdown_timeout_ms must be greater than 0")]
    ZeroShutdownTimeout,
    #[error(
        "shutdown_readiness_delay_ms ({shutdown_readiness_delay_ms}) must be less than shutdown_timeout_ms ({shutdown_timeout_ms})"
    )]
    InvalidShutdownTiming {
        shutdown_readiness_delay_ms: u64,
        shutdown_timeout_ms: u64,
    },
    #[error("log_level contains an invalid tracing filter expression")]
    InvalidLogFilter,
    #[error("data directory path exists but is not a directory")]
    InvalidDataDirectory,
    #[error("http_request_timeout_ms must be between 1 and 300000")]
    InvalidHttpRequestTimeout,
    #[error("http_max_concurrent_requests must be between 1 and 4096")]
    InvalidHttpMaxConcurrentRequests,
    #[error("http_max_request_body_bytes must be between 1 and 67108864")]
    InvalidHttpMaxRequestBodyBytes,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PartialAppConfig {
    host: Option<String>,
    port: Option<u16>,
    database_url: Option<String>,
    database_pool_size: Option<u32>,
    database_acquire_timeout_ms: Option<u64>,
    sqlite_busy_timeout_ms: Option<u64>,
    log_level: Option<String>,
    shutdown_readiness_delay_ms: Option<u64>,
    shutdown_timeout_ms: Option<u64>,
    http_request_timeout_ms: Option<u64>,
    http_max_concurrent_requests: Option<u32>,
    http_max_request_body_bytes: Option<u64>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

impl AppConfig {
    pub fn load(mode: ConfigLoadMode) -> Result<ResolvedConfig, ConfigError> {
        let environment = collect_app_environment()?;
        Self::load_from_environment(environment, mode)
    }

    fn load_from_environment(
        environment: HashMap<String, String>,
        mode: ConfigLoadMode,
    ) -> Result<ResolvedConfig, ConfigError> {
        let data_dir = resolve_data_dir(&environment)?;
        let (config_file, config_path) = resolve_config_file(&environment, &data_dir)?;
        let mut warnings = environment_warnings(&environment);

        let mut builder = Config::builder();
        if let Some(path) = &config_path {
            builder = builder.add_source(File::from(path.clone()).required(true));
        }

        let runtime_environment = environment
            .iter()
            .filter(|(key, _)| RUNTIME_ENVIRONMENT_KEYS.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<HashMap<_, _>>();
        builder = builder.add_source(app_environment_source().source(Some(runtime_environment)));

        let partial: PartialAppConfig = builder
            .build()
            .map_err(|source| ConfigError::Parse { source })?
            .try_deserialize()
            .map_err(|source| ConfigError::Parse { source })?;

        warnings.extend(file_warnings(&partial.extra, &config_file));
        let config = resolve_values(partial, data_dir, &mut warnings)?;

        if mode == ConfigLoadMode::Check {
            let blocking = warnings
                .iter()
                .map(|warning| format!("{} ({})", warning.key, warning.message))
                .collect::<Vec<_>>();
            if !blocking.is_empty() {
                return Err(ConfigError::CheckRejected {
                    issues: blocking.join(", "),
                });
            }
        }

        let summary = ConfigSummary {
            valid: true,
            data_dir: config.data_dir.clone(),
            config_file,
            host: config.host.clone(),
            port: config.port,
            database_kind: config.database_kind,
            database_pool_size: config.database_pool_size,
            database_acquire_timeout_ms: config.database_acquire_timeout_ms,
            sqlite_busy_timeout_ms: (config.database_kind == DatabaseKind::Sqlite)
                .then_some(config.sqlite_busy_timeout_ms),
            log_level: config.log_level.clone(),
            shutdown_readiness_delay_ms: config.shutdown_readiness_delay_ms,
            shutdown_timeout_ms: config.shutdown_timeout_ms,
            http_request_timeout_ms: config.http_request_timeout_ms,
            http_max_concurrent_requests: config.http_max_concurrent_requests,
            http_max_request_body_bytes: config.http_max_request_body_bytes,
            warnings: warnings.clone(),
        };

        Ok(ResolvedConfig {
            config,
            summary,
            warnings,
        })
    }

    #[cfg(test)]
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        validate_resolved_config(self)
    }

    pub fn bind_address(&self) -> Result<SocketAddr, std::net::AddrParseError> {
        let host: IpAddr = self.host.parse()?;
        Ok(SocketAddr::new(host, self.port))
    }
}

fn collect_app_environment() -> Result<HashMap<String, String>, ConfigError> {
    let mut environment = HashMap::new();
    for (key, value) in env::vars_os() {
        let Some(key) = key.to_str() else {
            continue;
        };
        if !key.starts_with("APP_") {
            continue;
        }
        let value = value
            .into_string()
            .map_err(|_| ConfigError::InvalidBootstrap {
                message: format!("{key} must contain valid Unicode"),
            })?;
        environment.insert(key.to_string(), value);
    }
    Ok(environment)
}

fn resolve_data_dir(environment: &HashMap<String, String>) -> Result<PathBuf, ConfigError> {
    let value = environment
        .get("APP_DATA_DIR")
        .map(String::as_str)
        .unwrap_or(DEFAULT_DATA_DIR);
    if value.trim().is_empty() || value != value.trim() {
        return Err(ConfigError::InvalidBootstrap {
            message: "APP_DATA_DIR must not be empty or contain surrounding whitespace".to_string(),
        });
    }

    let path = PathBuf::from(value);
    if path.exists() && !path.is_dir() {
        return Err(ConfigValidationError::InvalidDataDirectory.into());
    }
    Ok(path)
}

fn resolve_config_file(
    environment: &HashMap<String, String>,
    data_dir: &Path,
) -> Result<(ConfigFileSource, Option<PathBuf>), ConfigError> {
    let (path, explicit) = match environment.get("APP_CONFIG_PATH") {
        Some(value) => {
            if value.trim().is_empty() || value != value.trim() {
                return Err(ConfigError::InvalidBootstrap {
                    message: "APP_CONFIG_PATH must not be empty or contain surrounding whitespace"
                        .to_string(),
                });
            }
            (PathBuf::from(value), true)
        }
        None => (data_dir.join(DEFAULT_CONFIG_RELATIVE_PATH), false),
    };

    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            if explicit {
                return Err(ConfigError::ReadConfigFile {
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "explicit configuration file does not exist",
                    ),
                });
            }
            return Ok((ConfigFileSource::None, None));
        }
        Err(source) => return Err(ConfigError::InspectConfigPath { source }),
    };
    if !metadata.is_file() {
        return Err(ConfigError::ConfigPathNotFile);
    }
    fs::File::open(&path).map_err(|source| ConfigError::ReadConfigFile { source })?;

    let source = if explicit {
        ConfigFileSource::Explicit { path: path.clone() }
    } else {
        ConfigFileSource::Default { path: path.clone() }
    };
    Ok((source, Some(path)))
}

fn environment_warnings(environment: &HashMap<String, String>) -> Vec<ConfigWarning> {
    let mut warnings = Vec::new();
    let mut keys = environment.keys().collect::<Vec<_>>();
    keys.sort();

    for key in keys {
        if !RUNTIME_ENVIRONMENT_KEYS.contains(&key.as_str())
            && !BOOTSTRAP_ENVIRONMENT_KEYS.contains(&key.as_str())
        {
            warnings.push(ConfigWarning {
                code: "unknown_key".to_string(),
                source: "environment".to_string(),
                key: key.clone(),
                message: "unknown APP_* environment variable is ignored".to_string(),
            });
        }
    }
    warnings
}

fn file_warnings(
    extra: &BTreeMap<String, serde_json::Value>,
    source: &ConfigFileSource,
) -> Vec<ConfigWarning> {
    let source = match source {
        ConfigFileSource::None => "configuration_file".to_string(),
        ConfigFileSource::Default { path } | ConfigFileSource::Explicit { path } => {
            format!("configuration_file:{}", path.display())
        }
    };

    extra
        .keys()
        .map(|key| ConfigWarning {
            code: "unknown_key".to_string(),
            source: source.clone(),
            key: key.clone(),
            message: "unknown YAML field is ignored".to_string(),
        })
        .collect()
}

fn resolve_values(
    partial: PartialAppConfig,
    data_dir: PathBuf,
    warnings: &mut Vec<ConfigWarning>,
) -> Result<AppConfig, ConfigValidationError> {
    let host = partial.host.unwrap_or_else(|| DEFAULT_HOST.to_string());
    validate_non_empty_string("host", &host)?;
    host.parse::<IpAddr>()
        .map_err(|_| ConfigValidationError::InvalidHost)?;

    let port = partial.port.unwrap_or(DEFAULT_PORT);
    if port == 0 {
        return Err(ConfigValidationError::InvalidPort);
    }

    let database_url = partial.database_url.unwrap_or_else(|| {
        data_dir
            .join(DEFAULT_DATABASE_RELATIVE_PATH)
            .to_string_lossy()
            .into_owned()
    });
    let (database_url, database_kind) = DatabaseUrl::parse(database_url)?;

    let database_pool_size = partial.database_pool_size.unwrap_or(match database_kind {
        DatabaseKind::Sqlite => DEFAULT_DATABASE_POOL_SIZE_SQLITE,
        DatabaseKind::Postgres => DEFAULT_DATABASE_POOL_SIZE_POSTGRES,
    });
    if database_pool_size == 0 {
        return Err(ConfigValidationError::ZeroDatabasePoolSize);
    }
    if database_url.as_str() == ":memory:" && database_pool_size != 1 {
        return Err(ConfigValidationError::InvalidMemoryDatabasePoolSize);
    }

    let database_acquire_timeout_ms = partial
        .database_acquire_timeout_ms
        .unwrap_or(DEFAULT_DATABASE_ACQUIRE_TIMEOUT_MS);
    if database_acquire_timeout_ms == 0 {
        return Err(ConfigValidationError::ZeroDatabaseAcquireTimeout);
    }

    let sqlite_busy_timeout_was_explicit = partial.sqlite_busy_timeout_ms.is_some();
    let sqlite_busy_timeout_ms = partial
        .sqlite_busy_timeout_ms
        .unwrap_or(DEFAULT_SQLITE_BUSY_TIMEOUT_MS);
    if database_kind == DatabaseKind::Postgres && sqlite_busy_timeout_was_explicit {
        warnings.push(ConfigWarning {
            code: "inactive_setting".to_string(),
            source: "resolved_configuration".to_string(),
            key: "sqlite_busy_timeout_ms".to_string(),
            message: "setting is ignored when database_url selects PostgreSQL".to_string(),
        });
    }

    let log_level = partial
        .log_level
        .unwrap_or_else(|| DEFAULT_LOG_LEVEL.to_string());
    validate_non_empty_string("log_level", &log_level)?;
    EnvFilter::try_new(&log_level).map_err(|_| ConfigValidationError::InvalidLogFilter)?;

    let shutdown_readiness_delay_ms = partial
        .shutdown_readiness_delay_ms
        .unwrap_or(DEFAULT_SHUTDOWN_READINESS_DELAY_MS);
    let shutdown_timeout_ms = partial
        .shutdown_timeout_ms
        .unwrap_or(DEFAULT_SHUTDOWN_TIMEOUT_MS);
    let http_request_timeout_ms = partial
        .http_request_timeout_ms
        .unwrap_or(DEFAULT_HTTP_REQUEST_TIMEOUT_MS);
    let http_max_concurrent_requests = partial
        .http_max_concurrent_requests
        .unwrap_or(DEFAULT_HTTP_MAX_CONCURRENT_REQUESTS);
    let http_max_request_body_bytes = partial
        .http_max_request_body_bytes
        .unwrap_or(DEFAULT_HTTP_MAX_REQUEST_BODY_BYTES);

    let config = AppConfig {
        data_dir,
        host,
        port,
        database_url,
        database_kind,
        database_pool_size,
        database_acquire_timeout_ms,
        sqlite_busy_timeout_ms,
        log_level,
        shutdown_readiness_delay_ms,
        shutdown_timeout_ms,
        http_request_timeout_ms,
        http_max_concurrent_requests,
        http_max_request_body_bytes,
    };
    validate_resolved_config(&config)?;
    Ok(config)
}

fn validate_resolved_config(config: &AppConfig) -> Result<(), ConfigValidationError> {
    if config.database_pool_size == 0 {
        return Err(ConfigValidationError::ZeroDatabasePoolSize);
    }
    if config.database_acquire_timeout_ms == 0 {
        return Err(ConfigValidationError::ZeroDatabaseAcquireTimeout);
    }
    if config.shutdown_timeout_ms == 0 {
        return Err(ConfigValidationError::ZeroShutdownTimeout);
    }
    if config.shutdown_readiness_delay_ms >= config.shutdown_timeout_ms {
        return Err(ConfigValidationError::InvalidShutdownTiming {
            shutdown_readiness_delay_ms: config.shutdown_readiness_delay_ms,
            shutdown_timeout_ms: config.shutdown_timeout_ms,
        });
    }
    if !(MIN_HTTP_REQUEST_TIMEOUT_MS..=MAX_HTTP_REQUEST_TIMEOUT_MS)
        .contains(&config.http_request_timeout_ms)
    {
        return Err(ConfigValidationError::InvalidHttpRequestTimeout);
    }
    if !(MIN_HTTP_MAX_CONCURRENT_REQUESTS..=MAX_HTTP_MAX_CONCURRENT_REQUESTS)
        .contains(&config.http_max_concurrent_requests)
    {
        return Err(ConfigValidationError::InvalidHttpMaxConcurrentRequests);
    }
    if !(MIN_HTTP_MAX_REQUEST_BODY_BYTES..=MAX_HTTP_MAX_REQUEST_BODY_BYTES)
        .contains(&config.http_max_request_body_bytes)
    {
        return Err(ConfigValidationError::InvalidHttpMaxRequestBodyBytes);
    }
    Ok(())
}

fn validate_non_empty_string(
    field: &'static str,
    value: &str,
) -> Result<(), ConfigValidationError> {
    if value.trim().is_empty() || value != value.trim() {
        return Err(ConfigValidationError::InvalidString { field });
    }
    Ok(())
}

fn app_environment_source() -> Environment {
    Environment::with_prefix("APP")
        .prefix_separator("_")
        .separator("__")
        .try_parsing(true)
        .ignore_empty(false)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn environment(values: &[(&str, &str)]) -> HashMap<String, String> {
        values
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect()
    }

    fn file_environment(contents: &str) -> (tempfile::TempDir, HashMap<String, String>) {
        let temporary_directory = tempdir().expect("temporary directory should be created");
        let config_path = temporary_directory.path().join("config.yaml");
        fs::write(&config_path, contents).expect("configuration should be written");
        let environment = environment(&[(
            "APP_CONFIG_PATH",
            config_path.to_str().expect("UTF-8 configuration path"),
        )]);
        (temporary_directory, environment)
    }

    #[test]
    fn zero_configuration_uses_local_sqlite_defaults() {
        assert_eq!(AppConfig::default().data_dir, PathBuf::from(".app/dev"));
        let resolved = AppConfig::load_from_environment(HashMap::new(), ConfigLoadMode::Runtime)
            .expect("defaults should resolve");

        assert_eq!(resolved.config.data_dir, PathBuf::from(".app/dev"));
        assert_eq!(resolved.config.host, "127.0.0.1");
        assert_eq!(resolved.config.port, 8000);
        assert_eq!(resolved.config.database_kind, DatabaseKind::Sqlite);
        assert_eq!(resolved.config.database_pool_size, 1);
        assert_eq!(resolved.config.http_request_timeout_ms, 30_000);
        assert_eq!(resolved.config.http_max_concurrent_requests, 64);
        assert_eq!(resolved.config.http_max_request_body_bytes, 1_048_576);
        assert_eq!(
            resolved.config.database_url.as_str(),
            ".app/dev/db/cyder-template.sqlite"
        );
        assert!(resolved.warnings.is_empty());
    }

    #[test]
    fn environment_overrides_file_and_postgres_defaults_to_five_connections() {
        let temporary_directory = tempdir().expect("temporary directory should be created");
        let data_dir = temporary_directory.path();
        let config_dir = data_dir.join("config");
        fs::create_dir_all(&config_dir).expect("config directory should be created");
        fs::write(
            config_dir.join("config.yaml"),
            "host: 127.0.0.1\nport: 9000\ndatabase_url: file:local.sqlite\n",
        )
        .expect("config should be written");

        let resolved = AppConfig::load_from_environment(
            environment(&[
                ("APP_DATA_DIR", data_dir.to_str().expect("UTF-8 path")),
                ("APP_HOST", "0.0.0.0"),
                ("APP_DATABASE_URL", "postgres://app:secret@localhost/app"),
            ]),
            ConfigLoadMode::Runtime,
        )
        .expect("configuration should resolve");

        assert_eq!(resolved.config.host, "0.0.0.0");
        assert_eq!(resolved.config.port, 9000);
        assert_eq!(resolved.config.database_kind, DatabaseKind::Postgres);
        assert_eq!(resolved.config.database_pool_size, 5);
        assert!(matches!(
            resolved.summary.config_file,
            ConfigFileSource::Default { .. }
        ));
    }

    #[test]
    fn explicit_pool_size_overrides_backend_default() {
        let (_temporary_directory, values) =
            file_environment("database_url: postgres://localhost/app\ndatabase_pool_size: 9\n");
        let resolved = AppConfig::load_from_environment(values, ConfigLoadMode::Runtime)
            .expect("configuration should resolve");

        assert_eq!(resolved.config.database_pool_size, 9);
    }

    #[test]
    fn explicit_missing_config_file_fails() {
        let error = AppConfig::load_from_environment(
            environment(&[("APP_CONFIG_PATH", "/definitely/missing/config.yaml")]),
            ConfigLoadMode::Runtime,
        )
        .expect_err("missing explicit config should fail");

        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn default_config_path_inspection_errors_are_not_treated_as_missing() {
        let temporary_directory = tempdir().expect("temporary directory should be created");
        fs::write(temporary_directory.path().join("config"), "not a directory")
            .expect("blocking file should be written");

        let error = AppConfig::load_from_environment(
            environment(&[(
                "APP_DATA_DIR",
                temporary_directory.path().to_str().expect("UTF-8 path"),
            )]),
            ConfigLoadMode::Runtime,
        )
        .expect_err("default config inspection errors should fail");

        assert!(matches!(error, ConfigError::InspectConfigPath { .. }));
    }

    #[test]
    fn unknown_environment_keys_warn_at_runtime_and_fail_checks() {
        let values = environment(&[
            ("APP_DATABASE_URl", "ignored"),
            ("APP_UNSUPPORTED_SETTING", "ignored"),
        ]);
        let resolved = AppConfig::load_from_environment(values.clone(), ConfigLoadMode::Runtime)
            .expect("compatible loading should succeed");
        assert_eq!(resolved.warnings.len(), 2);
        assert_eq!(resolved.config.database_pool_size, 1);

        let error = AppConfig::load_from_environment(values, ConfigLoadMode::Check)
            .expect_err("configuration checks should reject warnings");
        assert!(error.to_string().contains("APP_DATABASE_URl"));
        assert!(error.to_string().contains("APP_UNSUPPORTED_SETTING"));
    }

    #[test]
    fn invalid_log_filter_and_empty_database_url_fail() {
        let log_error = AppConfig::load_from_environment(
            environment(&[("APP_LOG_LEVEL", "[")]),
            ConfigLoadMode::Runtime,
        )
        .expect_err("invalid log filter should fail");
        assert!(log_error.to_string().contains("log_level"));

        let database_error = AppConfig::load_from_environment(
            environment(&[("APP_DATABASE_URL", "")]),
            ConfigLoadMode::Runtime,
        )
        .expect_err("empty database URL should fail");
        assert!(database_error.to_string().contains("database_url"));
    }

    #[test]
    fn postgres_summary_never_contains_database_url() {
        let secret = "unique-password-marker";
        let resolved = AppConfig::load_from_environment(
            environment(&[(
                "APP_DATABASE_URL",
                &format!("postgres://app:{secret}@localhost/app"),
            )]),
            ConfigLoadMode::Runtime,
        )
        .expect("configuration should resolve");

        let json = serde_json::to_string(&resolved.summary).expect("summary should serialize");
        let debug = format!("{:?}", resolved.config);
        assert!(!json.contains(secret));
        assert!(!debug.contains(secret));
    }

    #[test]
    fn inactive_settings_warn_at_runtime_and_fail_checks() {
        let (_temporary_directory, values) = file_environment(
            "database_url: postgres://localhost/app\nsqlite_busy_timeout_ms: 100\n",
        );
        let resolved = AppConfig::load_from_environment(values.clone(), ConfigLoadMode::Runtime)
            .expect("inactive setting should only warn at runtime");

        assert_eq!(resolved.warnings.len(), 1);
        assert_eq!(resolved.warnings[0].code, "inactive_setting");
        assert!(resolved.summary.sqlite_busy_timeout_ms.is_none());

        let error = AppConfig::load_from_environment(values, ConfigLoadMode::Check)
            .expect_err("configuration checks should reject inactive settings");
        assert!(error.to_string().contains("sqlite_busy_timeout_ms"));
    }

    #[test]
    fn shutdown_timing_is_validated() {
        let config = AppConfig {
            shutdown_readiness_delay_ms: 1_000,
            shutdown_timeout_ms: 1_000,
            ..AppConfig::default()
        };

        assert!(matches!(
            config.validate(),
            Err(ConfigValidationError::InvalidShutdownTiming { .. })
        ));
    }

    #[test]
    fn http_limits_are_loaded_from_yaml_and_reported_safely() {
        let (_temporary_directory, values) = file_environment(
            "http_request_timeout_ms: 45000\nhttp_max_concurrent_requests: 128\nhttp_max_request_body_bytes: 2097152\n",
        );
        let resolved = AppConfig::load_from_environment(values, ConfigLoadMode::Check)
            .expect("HTTP limits should resolve from YAML");

        assert_eq!(resolved.config.http_request_timeout_ms, 45_000);
        assert_eq!(resolved.config.http_max_concurrent_requests, 128);
        assert_eq!(resolved.config.http_max_request_body_bytes, 2_097_152);
        assert_eq!(resolved.summary.http_request_timeout_ms, 45_000);
        assert_eq!(resolved.summary.http_max_concurrent_requests, 128);
        assert_eq!(resolved.summary.http_max_request_body_bytes, 2_097_152);
    }

    #[test]
    fn http_limit_ranges_are_validated() {
        let invalid_timeout = AppConfig {
            http_request_timeout_ms: 300_001,
            ..AppConfig::default()
        };
        assert!(matches!(
            invalid_timeout.validate(),
            Err(ConfigValidationError::InvalidHttpRequestTimeout)
        ));

        let invalid_concurrency = AppConfig {
            http_max_concurrent_requests: 0,
            ..AppConfig::default()
        };
        assert!(matches!(
            invalid_concurrency.validate(),
            Err(ConfigValidationError::InvalidHttpMaxConcurrentRequests)
        ));

        let invalid_body = AppConfig {
            http_max_request_body_bytes: 67_108_865,
            ..AppConfig::default()
        };
        assert!(matches!(
            invalid_body.validate(),
            Err(ConfigValidationError::InvalidHttpMaxRequestBodyBytes)
        ));
    }

    #[test]
    fn yaml_unknown_keys_warn_at_runtime_and_fail_checks() {
        let temporary_directory = tempdir().expect("temporary directory should be created");
        let config_path = temporary_directory.path().join("config.yaml");
        fs::write(
            &config_path,
            "host: 127.0.0.1\nid_worker_id: 7\nmisspelled_timeout: 42\n",
        )
        .expect("configuration should be written");
        let values = environment(&[("APP_CONFIG_PATH", config_path.to_str().expect("UTF-8 path"))]);

        let resolved = AppConfig::load_from_environment(values.clone(), ConfigLoadMode::Runtime)
            .expect("compatible loading should succeed");
        assert_eq!(resolved.warnings.len(), 2);
        assert!(
            resolved
                .warnings
                .iter()
                .all(|warning| warning.code == "unknown_key")
        );
        assert!(resolved.warnings.iter().any(|warning| {
            warning.code == "unknown_key" && warning.key == "misspelled_timeout"
        }));

        let error = AppConfig::load_from_environment(values, ConfigLoadMode::Check)
            .expect_err("configuration checks should reject unknown YAML keys");
        assert!(error.to_string().contains("id_worker_id"));
        assert!(error.to_string().contains("misspelled_timeout"));
    }

    #[test]
    fn invalid_database_and_numeric_semantics_fail() {
        for (key, value, expected) in [
            ("APP_DATABASE_URL", "mysql://localhost/app", "unsupported"),
            (
                "APP_DATABASE_URL",
                "postgres://localhost",
                "must name a PostgreSQL database",
            ),
        ] {
            let error = AppConfig::load_from_environment(
                environment(&[(key, value)]),
                ConfigLoadMode::Runtime,
            )
            .expect_err("invalid configuration should fail");
            assert!(
                error.to_string().contains(expected),
                "unexpected error for {key}: {error}"
            );
        }

        let (_temporary_directory, values) =
            file_environment("database_url: ':memory:'\ndatabase_pool_size: 2\n");
        let memory_error = AppConfig::load_from_environment(values, ConfigLoadMode::Runtime)
            .expect_err("in-memory SQLite must use one connection");
        assert!(
            memory_error
                .to_string()
                .contains("requires database_pool_size=1")
        );
    }

    #[test]
    fn data_directory_must_be_a_directory_if_it_exists() {
        let temporary_directory = tempdir().expect("temporary directory should be created");
        let file_path = temporary_directory.path().join("not-a-directory");
        fs::write(&file_path, "content").expect("test file should be written");

        let error = AppConfig::load_from_environment(
            environment(&[("APP_DATA_DIR", file_path.to_str().expect("UTF-8 path"))]),
            ConfigLoadMode::Runtime,
        )
        .expect_err("file data path should fail");
        assert!(error.to_string().contains("not a directory"));
    }
}
