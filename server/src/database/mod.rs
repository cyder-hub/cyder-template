pub mod items;
pub mod users;

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

use crate::error::{AppError, AppResult};

const SQLITE_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/sqlite");
const POSTGRES_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/postgres");
static SQLITE_MEMORY_DATABASE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseKind {
    Postgres,
    Sqlite,
}

impl std::fmt::Display for DatabaseKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseKind::Postgres => f.write_str("postgres"),
            DatabaseKind::Sqlite => f.write_str("sqlite"),
        }
    }
}

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
    #[error("failed to create sqlite database directory '{path}': {source}")]
    CreateSqliteDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to create sqlite database file '{path}': {source}")]
    CreateSqliteFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("sqlite database path exists but is not a file: '{path}'")]
    InvalidSqliteFile { path: PathBuf },
    #[error("sqlite database parent path exists but is not a directory: '{path}'")]
    InvalidSqliteDirectory { path: PathBuf },
    #[error("failed to establish sqlite migration connection for '{path}': {source}")]
    SqliteConnection {
        path: PathBuf,
        source: diesel::ConnectionError,
    },
    #[error("failed to establish postgres migration connection: {source}")]
    PostgresConnection { source: diesel::ConnectionError },
    #[error("failed to run {backend} migrations: {message}")]
    Migration {
        backend: &'static str,
        message: String,
    },
    #[error("failed to create {backend} database pool: {message}")]
    Pool {
        backend: &'static str,
        message: String,
    },
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum DatabaseError {
    #[error("{backend} database pool checkout failed: {message}")]
    PoolCheckout {
        backend: &'static str,
        message: String,
    },
    #[error("{backend} database operation failed: {source}")]
    Operation {
        backend: &'static str,
        source: diesel::result::Error,
    },
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
            pool_size: pool_size.max(1),
            acquire_timeout: Duration::from_millis(acquire_timeout_ms.max(1)),
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
                        message: source.to_string(),
                    })
            }
            Self::Sqlite(pool) => pool
                .get()
                .await
                .map(DbConnection::Sqlite)
                .map_err(|source| DatabaseError::PoolCheckout {
                    backend: "sqlite",
                    message: source.to_string(),
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

pub async fn check_readiness(pool: &DbPool) -> AppResult<DatabaseHealth> {
    let kind = pool.kind();
    let mut conn = pool.get().await.map_err(|source| AppError::Readiness {
        message: source.to_string(),
    })?;

    match &mut conn {
        DbConnection::Postgres(conn) => check_postgres_query(conn).await?,
        DbConnection::Sqlite(conn) => check_sqlite_query(conn).await?,
    }

    Ok(DatabaseHealth {
        kind,
        connected: true,
    })
}

async fn check_postgres_query(conn: &mut PostgresConnection) -> AppResult<()> {
    let row = diesel::sql_query("SELECT 1 AS value")
        .get_result::<ReadyRow>(conn)
        .await
        .map_err(|source| AppError::Readiness {
            message: format!("postgres readiness query failed: {source}"),
        })?;

    ensure_ready_value(row, DatabaseKind::Postgres)
}

async fn check_sqlite_query(conn: &mut SqliteConnection) -> AppResult<()> {
    let row = diesel::sql_query("SELECT 1 AS value")
        .get_result::<ReadyRow>(conn)
        .await
        .map_err(|source| AppError::Readiness {
            message: format!("sqlite readiness query failed: {source}"),
        })?;

    ensure_ready_value(row, DatabaseKind::Sqlite)
}

fn ensure_ready_value(row: ReadyRow, kind: DatabaseKind) -> AppResult<()> {
    (row.value == 1)
        .then_some(())
        .ok_or_else(|| AppError::Readiness {
            message: format!("{kind} readiness query returned {}", row.value),
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
        configured_pool_size.max(1)
    }
}

fn sqlite_connection_url(database_url: &str) -> String {
    if database_url == ":memory:" {
        let counter = SQLITE_MEMORY_DATABASE_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!(
            "file:cyder-template-memory-{}-{counter}?mode=memory&cache=shared",
            std::process::id()
        )
    } else {
        database_url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::sql_types::{BigInt, Text};
    use diesel_async::RunQueryDsl;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(QueryableByName)]
    struct CountRow {
        #[diesel(sql_type = BigInt)]
        count: i64,
    }

    #[derive(QueryableByName)]
    struct JournalModeRow {
        #[diesel(sql_type = Text)]
        journal_mode: String,
    }

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
            database_kind(".app/dev/db/cyder-template.sqlite"),
            DatabaseKind::Sqlite
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sqlite_pool_creates_file_runs_migrations_and_checks_readiness() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let database_url = temp_dir
            .path()
            .join("cyder-template.sqlite")
            .to_string_lossy()
            .into_owned();

        let pool = DbPool::connect(&database_url, DbPoolOptions::default())
            .await
            .expect("sqlite pool should initialize");

        assert!(temp_dir.path().join("cyder-template.sqlite").is_file());
        assert_eq!(pool.kind(), DatabaseKind::Sqlite);

        let health = check_readiness(&pool).await.expect("readiness should pass");
        assert!(health.connected);

        let mut conn = match pool.get().await.expect("sqlite connection should checkout") {
            DbConnection::Sqlite(conn) => conn,
            DbConnection::Postgres(_) => unreachable!("sqlite pool should use sqlite"),
        };

        let item_count = diesel::sql_query("SELECT COUNT(*) AS count FROM items")
            .get_result::<CountRow>(&mut conn)
            .await
            .map(|row| row.count)
            .expect("items table should exist");
        assert_eq!(item_count, 0);

        let user_count = diesel::sql_query("SELECT COUNT(*) AS count FROM users")
            .get_result::<CountRow>(&mut conn)
            .await
            .map(|row| row.count)
            .expect("users table should exist");
        assert_eq!(user_count, 0);
    }

    #[test]
    fn sqlite_memory_database_uses_single_effective_connection() {
        assert_eq!(effective_sqlite_pool_size(":memory:", 4), 1);
        assert_eq!(
            effective_sqlite_pool_size("file::memory:?cache=shared", 4),
            1
        );
        assert_eq!(
            effective_sqlite_pool_size("file:template?mode=memory&cache=shared", 4),
            1
        );
        assert_eq!(effective_sqlite_pool_size(".app/dev/db.sqlite", 4), 4);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sqlite_memory_pool_keeps_migrations_visible() {
        let pool = DbPool::connect(":memory:", DbPoolOptions::new(4, 500, 5_000))
            .await
            .expect("sqlite memory pool should initialize");

        let created = items::create(
            &pool,
            items::NewItem {
                id: 10,
                title: "Memory item".to_string(),
                description: String::new(),
                completed: false,
                created_at: 1,
                updated_at: 1,
            },
        )
        .await
        .expect("memory pool should share migrated schema with checked-out connection");

        assert_eq!(created.id, 10);
        assert_eq!(
            items::list(&pool)
                .await
                .expect("memory pool should list items"),
            vec![created]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sqlite_memory_pool_preserves_schema_after_reaper_interval() {
        let sqlite_pool = init_sqlite_pool_with_reaper_config(
            ":memory:",
            DbPoolOptions::new(4, 500, 5_000),
            Some(SqlitePoolReaperConfig {
                max_lifetime: Some(Duration::from_millis(5)),
                idle_timeout: Some(Duration::from_millis(5)),
                reaper_rate: Duration::from_millis(5),
            }),
        )
        .await
        .expect("sqlite memory pool should initialize");
        let pool = DbPool::Sqlite(sqlite_pool);

        tokio::time::sleep(Duration::from_millis(50)).await;

        let created = items::create(
            &pool,
            items::NewItem {
                id: 11,
                title: "Memory item after reaper".to_string(),
                description: String::new(),
                completed: false,
                created_at: 1,
                updated_at: 1,
            },
        )
        .await
        .expect("memory pool should keep migrated schema after reaper interval");

        assert_eq!(
            items::list(&pool)
                .await
                .expect("memory pool should list items"),
            vec![created]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sqlite_file_pool_with_multiple_connections_runs_concurrent_crud_and_readiness() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let database_url = temp_dir
            .path()
            .join("multi-connection.sqlite")
            .to_string_lossy()
            .into_owned();
        let pool = DbPool::connect(&database_url, DbPoolOptions::new(4, 2_000, 5_000))
            .await
            .expect("sqlite pool should initialize");

        assert_eq!(pool.kind(), DatabaseKind::Sqlite);

        let mut conn = match pool.get().await.expect("sqlite connection should checkout") {
            DbConnection::Sqlite(conn) => conn,
            DbConnection::Postgres(_) => unreachable!("sqlite pool should use sqlite"),
        };
        let journal_mode = diesel::sql_query("PRAGMA journal_mode")
            .get_result::<JournalModeRow>(&mut conn)
            .await
            .map(|row| row.journal_mode)
            .expect("sqlite journal_mode should be readable");
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        drop(conn);

        let create_first = items::create(&pool, new_test_item(1));
        let create_second = items::create(&pool, new_test_item(2));
        let create_third = items::create(&pool, new_test_item(3));
        let create_fourth = items::create(&pool, new_test_item(4));
        let (first, second, third, fourth) =
            tokio::join!(create_first, create_second, create_third, create_fourth);

        let first = first.expect("first item should be created");
        let second = second.expect("second item should be created");
        let third = third.expect("third item should be created");
        let fourth = fourth.expect("fourth item should be created");

        let readiness = check_readiness(&pool);
        let list = items::list(&pool);
        let get_second = items::get(&pool, second.id);
        let (health, listed, fetched) = tokio::join!(readiness, list, get_second);

        assert!(health.expect("readiness should pass").connected);
        assert_eq!(
            listed.expect("items should list"),
            vec![fourth.clone(), third.clone(), second.clone(), first.clone()]
        );
        assert_eq!(
            fetched.expect("item lookup should succeed"),
            Some(second.clone())
        );

        let delete_first = items::delete(&pool, first.id);
        let delete_fourth = items::delete(&pool, fourth.id);
        let delete_missing = items::delete(&pool, 999);
        let (deleted_first, deleted_fourth, deleted_missing) =
            tokio::join!(delete_first, delete_fourth, delete_missing);

        assert!(deleted_first.expect("first item should delete"));
        assert!(deleted_fourth.expect("fourth item should delete"));
        assert!(!deleted_missing.expect("missing item delete should succeed"));
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires APP_TEST_POSTGRES_URL pointing at an isolated PostgreSQL test database"]
    async fn postgres_pool_with_multiple_connections_runs_crud_and_readiness() {
        let database_url = std::env::var("APP_TEST_POSTGRES_URL")
            .expect("APP_TEST_POSTGRES_URL must point at an isolated PostgreSQL test database");
        let pool = DbPool::connect(&database_url, DbPoolOptions::new(4, 2_000, 5_000))
            .await
            .expect("postgres pool should initialize");
        let run_id = unique_test_id();

        assert_eq!(pool.kind(), DatabaseKind::Postgres);
        cleanup_postgres_test_records(&pool, run_id)
            .await
            .expect("postgres test records should clean before test");

        let exercise_result = exercise_postgres_pool(&pool, run_id).await;
        let cleanup_result = cleanup_postgres_test_records(&pool, run_id).await;
        cleanup_result.expect("postgres test records should clean after test");
        exercise_result.expect("postgres multi-connection CRUD/readiness should pass");
    }

    async fn exercise_postgres_pool(pool: &DbPool, run_id: i64) -> AppResult<()> {
        let first_item = items::create(pool, new_postgres_test_item(run_id, 1));
        let second_item = items::create(pool, new_postgres_test_item(run_id, 2));
        let first_user = users::create(pool, new_postgres_test_user(run_id, 1));
        let second_user = users::create(pool, new_postgres_test_user(run_id, 2));
        let readiness = check_readiness(pool);
        let (first_item, second_item, first_user, second_user, health) =
            tokio::join!(first_item, second_item, first_user, second_user, readiness);

        let first_item = first_item?;
        let second_item = second_item?;
        let first_user = first_user?;
        let second_user = second_user?;
        assert!(health?.connected);

        assert_eq!(
            items::get(pool, first_item.id).await?,
            Some(first_item.clone())
        );
        assert_eq!(
            users::get(pool, second_user.id).await?,
            Some(second_user.clone())
        );

        let item_ids = matching_item_ids(items::list(pool).await?, run_id);
        assert_eq!(item_ids, vec![second_item.id, first_item.id]);

        let user_ids = matching_user_ids(users::list(pool).await?, run_id);
        assert_eq!(user_ids, vec![second_user.id, first_user.id]);

        let delete_first_item = items::delete(pool, first_item.id);
        let delete_second_item = items::delete(pool, second_item.id);
        let delete_first_user = users::delete(pool, first_user.id);
        let delete_second_user = users::delete(pool, second_user.id);
        let (first_item_deleted, second_item_deleted, first_user_deleted, second_user_deleted) = tokio::join!(
            delete_first_item,
            delete_second_item,
            delete_first_user,
            delete_second_user
        );

        assert!(first_item_deleted?);
        assert!(second_item_deleted?);
        assert!(first_user_deleted?);
        assert!(second_user_deleted?);
        assert!(!items::delete(pool, first_item.id).await?);
        assert!(!users::delete(pool, first_user.id).await?);

        Ok(())
    }

    async fn cleanup_postgres_test_records(pool: &DbPool, run_id: i64) -> AppResult<()> {
        for id in [
            postgres_test_id(run_id, 1),
            postgres_test_id(run_id, 2),
            postgres_test_id(run_id, 101),
            postgres_test_id(run_id, 102),
        ] {
            let _ = items::delete(pool, id).await?;
            let _ = users::delete(pool, id).await?;
        }

        Ok(())
    }

    fn matching_item_ids(items: Vec<items::Item>, run_id: i64) -> Vec<i64> {
        let first = postgres_test_id(run_id, 1);
        let second = postgres_test_id(run_id, 2);
        items
            .into_iter()
            .map(|item| item.id)
            .filter(|id| *id == first || *id == second)
            .collect()
    }

    fn matching_user_ids(users: Vec<users::User>, run_id: i64) -> Vec<i64> {
        let first = postgres_test_id(run_id, 101);
        let second = postgres_test_id(run_id, 102);
        users
            .into_iter()
            .map(|user| user.id)
            .filter(|id| *id == first || *id == second)
            .collect()
    }

    fn new_postgres_test_item(run_id: i64, offset: i64) -> items::NewItem {
        let id = postgres_test_id(run_id, offset);
        items::NewItem {
            id,
            title: format!("Postgres item {id}"),
            description: format!("Postgres integration item {id}"),
            completed: offset % 2 == 0,
            created_at: id,
            updated_at: id,
        }
    }

    fn new_postgres_test_user(run_id: i64, offset: i64) -> users::NewUser {
        let id = postgres_test_id(run_id, 100 + offset);
        users::NewUser {
            id,
            name: format!("Postgres User {id}"),
            email: format!("postgres-integration-{id}@example.test"),
            active: offset % 2 == 1,
            created_at: id,
            updated_at: id,
        }
    }

    fn postgres_test_id(run_id: i64, offset: i64) -> i64 {
        run_id * 1_000 + offset
    }

    fn unique_test_id() -> i64 {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_millis();
        let millis = i64::try_from(millis % 1_000_000_000).expect("millis should fit in i64");
        1_000_000_000 + millis + i64::from(std::process::id())
    }

    fn new_test_item(id: i64) -> items::NewItem {
        items::NewItem {
            id,
            title: format!("Item {id}"),
            description: format!("Description {id}"),
            completed: id % 2 == 0,
            created_at: id,
            updated_at: id,
        }
    }
}
