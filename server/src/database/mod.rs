use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use diesel::{QueryableByName, SqliteConnection as DieselSqliteConnection, sql_types::Integer};
use diesel_async::{
    AsyncConnection, AsyncMigrationHarness, AsyncPgConnection, RunQueryDsl, SimpleAsyncConnection,
    pooled_connection::{
        AsyncDieselConnectionManager, ManagerConfig,
        bb8::{Pool, PooledConnection},
    },
    sync_connection_wrapper::SyncConnectionWrapper,
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use serde::Serialize;

use crate::{
    config::DatabaseKind,
    error::{HttpError, HttpResult},
};

// Empty migration directories use an equivalent path spelling so Diesel's proc
// macro cannot reuse a stale expansion after their contents change.
const SQLITE_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/./sqlite");
const POSTGRES_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/./postgres");
static SQLITE_MEMORY_DATABASE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn database_kind(database_url: &str) -> DatabaseKind {
    let database_url = database_url.to_ascii_lowercase();
    if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
        DatabaseKind::Postgres
    } else {
        DatabaseKind::Sqlite
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DatabaseInitError {
    #[error("failed to create sqlite database directory")]
    CreateSqliteDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to create sqlite database file")]
    CreateSqliteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("sqlite database path exists but is not a file")]
    InvalidSqliteFile { path: PathBuf },
    #[error("sqlite database parent path exists but is not a directory")]
    InvalidSqliteDirectory { path: PathBuf },
    #[error("failed to establish sqlite migration connection")]
    SqliteConnection {
        path: PathBuf,
        #[source]
        source: diesel::ConnectionError,
    },
    #[error("failed to establish postgres migration connection")]
    PostgresConnection {
        #[source]
        source: diesel::ConnectionError,
    },
    #[error("failed to run {backend} migrations")]
    Migration {
        backend: &'static str,
        message: String,
    },
    #[error("failed to create {backend} database pool")]
    Pool {
        backend: &'static str,
        message: String,
    },
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum DatabaseError {
    #[error("{backend} database pool checkout failed")]
    PoolCheckout {
        backend: &'static str,
        #[source]
        source: DatabaseDiagnostic,
    },
    #[error("{backend} database operation failed")]
    Operation {
        backend: &'static str,
        source: diesel::result::Error,
    },
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct DatabaseDiagnostic {
    message: String,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl DatabaseDiagnostic {
    #[cfg(test)]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    fn with_source(source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self {
            message: source.to_string(),
            source: Some(Box::new(source)),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DbPoolOptions {
    pub pool_size: u32,
    pub acquire_timeout: Duration,
    pub sqlite_busy_timeout: Duration,
}

impl DbPoolOptions {
    pub fn new(pool_size: u32, acquire_timeout_ms: u64, sqlite_busy_timeout_ms: u64) -> Self {
        Self {
            pool_size,
            acquire_timeout: Duration::from_millis(acquire_timeout_ms),
            sqlite_busy_timeout: Duration::from_millis(sqlite_busy_timeout_ms),
        }
    }
}

impl Default for DbPoolOptions {
    fn default() -> Self {
        Self::new(1, 30_000, 5_000)
    }
}

pub type PostgresConnection = AsyncPgConnection;
pub type SqliteConnection = SyncConnectionWrapper<DieselSqliteConnection>;
pub type PostgresPool = Pool<PostgresConnection>;
pub type SqlitePool = Pool<SqliteConnection>;
pub type PostgresPooledConnection<'a> = PooledConnection<'a, PostgresConnection>;
pub type SqlitePooledConnection<'a> = PooledConnection<'a, SqliteConnection>;

#[derive(Clone)]
pub enum DbPool {
    Postgres(PostgresPool),
    Sqlite(SqlitePool),
}

#[expect(
    clippy::large_enum_variant,
    reason = "connections are immediately borrowed, while boxing PostgreSQL would allocate on every checkout"
)]
pub enum DbConnection<'a> {
    Postgres(PostgresPooledConnection<'a>),
    Sqlite(SqlitePooledConnection<'a>),
}

impl DbPool {
    pub async fn connect(
        database_url: &str,
        options: DbPoolOptions,
    ) -> Result<Self, DatabaseInitError> {
        match database_kind(database_url) {
            DatabaseKind::Postgres => init_postgres_pool(database_url, options)
                .await
                .map(Self::Postgres),
            DatabaseKind::Sqlite => init_sqlite_pool(database_url, options)
                .await
                .map(Self::Sqlite),
        }
    }

    pub fn kind(&self) -> DatabaseKind {
        match self {
            Self::Postgres(_) => DatabaseKind::Postgres,
            Self::Sqlite(_) => DatabaseKind::Sqlite,
        }
    }

    pub async fn get(&self) -> Result<DbConnection<'_>, DatabaseError> {
        match self {
            Self::Postgres(pool) => {
                pool.get()
                    .await
                    .map(DbConnection::Postgres)
                    .map_err(|source| DatabaseError::PoolCheckout {
                        backend: "postgres",
                        source: DatabaseDiagnostic::with_source(source),
                    })
            }
            Self::Sqlite(pool) => pool
                .get()
                .await
                .map(DbConnection::Sqlite)
                .map_err(|source| DatabaseError::PoolCheckout {
                    backend: "sqlite",
                    source: DatabaseDiagnostic::with_source(source),
                }),
        }
    }
}

impl DatabaseKind {
    #[allow(dead_code)]
    fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Sqlite => "sqlite",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DatabaseHealth {
    pub kind: DatabaseKind,
    pub connected: bool,
}

#[derive(QueryableByName)]
struct ReadyRow {
    #[diesel(sql_type = Integer)]
    value: i32,
}

pub async fn check_readiness(pool: &DbPool) -> HttpResult<DatabaseHealth> {
    let kind = pool.kind();
    let mut conn = pool
        .get()
        .await
        .map_err(|source| HttpError::readiness_with_source("database checkout failed", source))?;

    match &mut conn {
        DbConnection::Postgres(conn) => check_postgres_query(conn).await?,
        DbConnection::Sqlite(conn) => check_sqlite_query(conn).await?,
    }

    Ok(DatabaseHealth {
        kind,
        connected: true,
    })
}

async fn check_postgres_query(conn: &mut PostgresConnection) -> HttpResult<()> {
    let row = diesel::sql_query("SELECT 1 AS value")
        .get_result::<ReadyRow>(conn)
        .await
        .map_err(|source| {
            HttpError::readiness_with_source("postgres readiness query failed", source)
        })?;

    ensure_ready_value(row, DatabaseKind::Postgres)
}

async fn check_sqlite_query(conn: &mut SqliteConnection) -> HttpResult<()> {
    let row = diesel::sql_query("SELECT 1 AS value")
        .get_result::<ReadyRow>(conn)
        .await
        .map_err(|source| {
            HttpError::readiness_with_source("sqlite readiness query failed", source)
        })?;

    ensure_ready_value(row, DatabaseKind::Sqlite)
}

fn ensure_ready_value(row: ReadyRow, kind: DatabaseKind) -> HttpResult<()> {
    (row.value == 1).then_some(()).ok_or_else(|| {
        HttpError::readiness(format!("{kind} readiness query returned {}", row.value))
    })
}

async fn init_sqlite_pool(
    database_url: &str,
    options: DbPoolOptions,
) -> Result<SqlitePool, DatabaseInitError> {
    init_sqlite_pool_with_reaper_config(database_url, options, None).await
}

#[derive(Debug, Clone, Copy)]
struct SqlitePoolReaperConfig {
    max_lifetime: Option<Duration>,
    idle_timeout: Option<Duration>,
    reaper_rate: Duration,
}

async fn init_sqlite_pool_with_reaper_config(
    database_url: &str,
    options: DbPoolOptions,
    reaper_config: Option<SqlitePoolReaperConfig>,
) -> Result<SqlitePool, DatabaseInitError> {
    ensure_sqlite_database_file(database_url)?;
    let connection_url = sqlite_connection_url(database_url);

    let database_path = PathBuf::from(database_url);
    let conn = establish_sqlite_connection(&connection_url, options.sqlite_busy_timeout)
        .await
        .map_err(|source| DatabaseInitError::SqliteConnection {
            path: database_path.clone(),
            source,
        })?;
    let mut migrations = AsyncMigrationHarness::new(conn);

    migrations
        .run_pending_migrations(SQLITE_MIGRATIONS)
        .map_err(|source| DatabaseInitError::Migration {
            backend: "sqlite",
            message: source.to_string(),
        })?;
    // Shared in-memory SQLite databases exist only while at least one
    // connection remains open, so keep the migration connection alive until
    // the pool establishes its minimum idle connection below.
    let _migration_conn = migrations.into_inner();

    let manager = sqlite_connection_manager(&connection_url, options.sqlite_busy_timeout);
    let mut builder = Pool::builder()
        .max_size(effective_sqlite_pool_size(database_url, options.pool_size))
        .connection_timeout(options.acquire_timeout)
        .test_on_check_out(true);
    if let Some(reaper_config) = reaper_config {
        builder = builder
            .max_lifetime(reaper_config.max_lifetime)
            .idle_timeout(reaper_config.idle_timeout)
            .reaper_rate(reaper_config.reaper_rate);
    }
    if is_sqlite_memory_database(&connection_url) {
        // bb8 reap closes expired idle connections before replenishing min_idle.
        // For shared in-memory SQLite, closing the sole idle connection drops
        // the database, so keep memory pools alive for the pool's lifetime.
        builder = builder
            .min_idle(1)
            .max_lifetime(None::<Duration>)
            .idle_timeout(None::<Duration>);
    }

    builder
        .build(manager)
        .await
        .map_err(|source| DatabaseInitError::Pool {
            backend: "sqlite",
            message: source.to_string(),
        })
}

async fn init_postgres_pool(
    database_url: &str,
    options: DbPoolOptions,
) -> Result<PostgresPool, DatabaseInitError> {
    let conn = AsyncPgConnection::establish(database_url)
        .await
        .map_err(|source| DatabaseInitError::PostgresConnection { source })?;
    let mut migrations = AsyncMigrationHarness::new(conn);

    migrations
        .run_pending_migrations(POSTGRES_MIGRATIONS)
        .map_err(|source| DatabaseInitError::Migration {
            backend: "postgres",
            message: source.to_string(),
        })?;

    let manager = AsyncDieselConnectionManager::<PostgresConnection>::new(database_url);
    Pool::builder()
        .max_size(options.pool_size)
        .connection_timeout(options.acquire_timeout)
        .test_on_check_out(true)
        .build(manager)
        .await
        .map_err(|source| DatabaseInitError::Pool {
            backend: "postgres",
            message: source.to_string(),
        })
}

fn sqlite_connection_manager(
    database_url: &str,
    busy_timeout: Duration,
) -> AsyncDieselConnectionManager<SqliteConnection> {
    let mut manager_config = ManagerConfig::<SqliteConnection>::default();
    manager_config.custom_setup = Box::new(move |database_url| {
        let database_url = database_url.to_string();
        Box::pin(async move { establish_sqlite_connection(&database_url, busy_timeout).await })
    });

    AsyncDieselConnectionManager::<SqliteConnection>::new_with_config(database_url, manager_config)
}

async fn establish_sqlite_connection(
    database_url: &str,
    busy_timeout: Duration,
) -> diesel::ConnectionResult<SqliteConnection> {
    let mut conn = SqliteConnection::establish(database_url).await?;
    configure_sqlite_connection(database_url, &mut conn, busy_timeout)
        .await
        .map_err(|source| {
            diesel::ConnectionError::BadConnection(format!(
                "failed to configure sqlite connection: {source}"
            ))
        })?;
    Ok(conn)
}

async fn configure_sqlite_connection(
    database_url: &str,
    conn: &mut SqliteConnection,
    busy_timeout: Duration,
) -> diesel::QueryResult<()> {
    let busy_timeout_ms = busy_timeout.as_millis().min(i64::MAX as u128);

    if is_sqlite_memory_database(database_url) {
        conn.batch_execute(&format!(
            "PRAGMA busy_timeout = {busy_timeout_ms}; PRAGMA foreign_keys = ON"
        ))
        .await
    } else {
        conn.batch_execute(&format!(
            "PRAGMA journal_mode = WAL; PRAGMA busy_timeout = {busy_timeout_ms}; PRAGMA foreign_keys = ON"
        ))
        .await
    }
}

fn ensure_sqlite_database_file(database_url: &str) -> Result<(), DatabaseInitError> {
    if is_sqlite_memory_or_uri(database_url) {
        return Ok(());
    }

    let database_path = Path::new(database_url);
    if database_path.exists() {
        if database_path.is_file() {
            return Ok(());
        }
        return Err(DatabaseInitError::InvalidSqliteFile {
            path: database_path.to_path_buf(),
        });
    }

    if let Some(parent) = database_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        if parent.exists() && !parent.is_dir() {
            return Err(DatabaseInitError::InvalidSqliteDirectory {
                path: parent.to_path_buf(),
            });
        }

        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|source| {
                DatabaseInitError::CreateSqliteDirectory {
                    path: parent.to_path_buf(),
                    source,
                }
            })?;
        }
    }

    File::create(database_path).map_err(|source| DatabaseInitError::CreateSqliteFile {
        path: database_path.to_path_buf(),
        source,
    })?;

    Ok(())
}

fn is_sqlite_memory_or_uri(database_url: &str) -> bool {
    database_url == ":memory:" || database_url.starts_with("file:")
}

fn is_sqlite_memory_database(database_url: &str) -> bool {
    database_url == ":memory:"
        || database_url.starts_with("file::memory:")
        || (database_url.starts_with("file:") && database_url.contains("mode=memory"))
}

fn effective_sqlite_pool_size(database_url: &str, configured_pool_size: u32) -> u32 {
    // Plain SQLite memory databases are connection-local. A single effective
    // connection keeps migrations and application queries on the same schema.
    if is_sqlite_memory_database(database_url) {
        1
    } else {
        configured_pool_size
    }
}

fn sqlite_connection_url(database_url: &str) -> String {
    if database_url == ":memory:" {
        let counter = SQLITE_MEMORY_DATABASE_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!(
            "file:cyder-music-memory-{}-{counter}?mode=memory&cache=shared",
            std::process::id()
        )
    } else {
        database_url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_kind_detects_postgres_urls() {
        assert_eq!(
            database_kind("postgres://app:secret@localhost/app"),
            DatabaseKind::Postgres
        );
        assert_eq!(
            database_kind("postgresql://app:secret@localhost/app"),
            DatabaseKind::Postgres
        );
        assert_eq!(
            database_kind(".app/dev/db/cyder-music.sqlite"),
            DatabaseKind::Sqlite
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sqlite_pool_creates_file_and_checks_readiness() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let database_url = temp_dir
            .path()
            .join("cyder-music.sqlite")
            .to_string_lossy()
            .into_owned();

        let pool = DbPool::connect(&database_url, DbPoolOptions::default())
            .await
            .expect("sqlite pool should initialize");

        assert!(temp_dir.path().join("cyder-music.sqlite").is_file());
        assert_eq!(pool.kind(), DatabaseKind::Sqlite);

        let health = check_readiness(&pool).await.expect("readiness should pass");
        assert!(health.connected);
    }

    #[test]
    fn sqlite_memory_database_uses_single_effective_connection() {
        assert_eq!(effective_sqlite_pool_size(":memory:", 4), 1);
        assert_eq!(
            effective_sqlite_pool_size("file::memory:?cache=shared", 4),
            1
        );
        assert_eq!(
            effective_sqlite_pool_size("file:pool-size-test?mode=memory&cache=shared", 4),
            1
        );
        assert_eq!(effective_sqlite_pool_size(".app/dev/db.sqlite", 4), 4);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires DEV_POSTGRES_TEST_URL pointing at an isolated PostgreSQL test database"]
    async fn postgres_pool_connects_and_checks_readiness() {
        let database_url = std::env::var("DEV_POSTGRES_TEST_URL")
            .expect("DEV_POSTGRES_TEST_URL must point at an isolated PostgreSQL test database");
        let pool = DbPool::connect(&database_url, DbPoolOptions::new(4, 2_000, 5_000))
            .await
            .expect("postgres pool should initialize");

        assert_eq!(pool.kind(), DatabaseKind::Postgres);
        assert!(
            check_readiness(&pool)
                .await
                .expect("postgres readiness should pass")
                .connected
        );
    }
}
