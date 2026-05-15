use std::{env, net::SocketAddr, path::PathBuf};

use config::{Config, Environment, File};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub database_pool_size: u32,
    pub id_worker_id: u64,
    pub log_level: String,
    pub public_dir: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8000,
            database_url: default_database_url(),
            database_pool_size: 5,
            id_worker_id: 1,
            log_level: "info".to_string(),
            public_dir: "front/dist".to_string(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self, config::ConfigError> {
        let defaults = Self::default();
        let mut builder = Config::builder()
            .set_default("host", defaults.host)?
            .set_default("port", defaults.port)?
            .set_default("database_url", defaults.database_url)?
            .set_default("database_pool_size", defaults.database_pool_size)?
            .set_default("id_worker_id", defaults.id_worker_id)?
            .set_default("log_level", defaults.log_level)?
            .set_default("public_dir", defaults.public_dir)?;

        if let Some(config_path) = env::var_os("APP_CONFIG_PATH") {
            builder = builder.add_source(File::from(PathBuf::from(config_path)).required(false));
        } else {
            builder = builder.add_source(File::with_name("config").required(false));
        }

        builder
            .add_source(app_environment_source())
            .build()?
            .try_deserialize()
    }

    pub fn bind_address(&self) -> Result<SocketAddr, std::net::AddrParseError> {
        format!("{}:{}", self.host, self.port).parse()
    }
}

fn default_database_url() -> String {
    let data_dir = env::var_os("APP_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".app/dev"));

    data_dir
        .join("db")
        .join("cyder-template.sqlite")
        .to_string_lossy()
        .into_owned()
}

fn app_environment_source() -> Environment {
    Environment::with_prefix("APP")
        .prefix_separator("_")
        .separator("__")
        .try_parsing(true)
        .ignore_empty(true)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn app_environment_overrides_flat_field_names() {
        let mut env = HashMap::new();
        env.insert("APP_HOST".to_string(), "0.0.0.0".to_string());
        env.insert("APP_PORT".to_string(), "9000".to_string());
        env.insert("APP_DATABASE_URL".to_string(), ":memory:".to_string());
        env.insert("APP_DATABASE_POOL_SIZE".to_string(), "7".to_string());
        env.insert("APP_ID_WORKER_ID".to_string(), "3".to_string());
        env.insert("APP_LOG_LEVEL".to_string(), "debug".to_string());
        env.insert("APP_PUBLIC_DIR".to_string(), "front/dist".to_string());

        let config: AppConfig = Config::builder()
            .add_source(app_environment_source().source(Some(env)))
            .build()
            .expect("environment config should build")
            .try_deserialize()
            .expect("environment config should deserialize");

        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 9000);
        assert_eq!(config.database_url, ":memory:");
        assert_eq!(config.database_pool_size, 7);
        assert_eq!(config.id_worker_id, 3);
        assert_eq!(config.log_level, "debug");
        assert_eq!(config.public_dir, "front/dist");
    }
}
