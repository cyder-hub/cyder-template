pub mod items;
pub mod users;

use std::{
    fs::File,
    path::{Path, PathBuf},
};

use diesel::{
    Connection, PgConnection, QueryableByName, RunQueryDsl, SqliteConnection,
    r2d2::{ConnectionManager, Pool, PooledConnection},
    sql_types::Integer,
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use serde::Serialize;

use crate::error::{AppError, AppResult};

const SQLITE_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/sqlite");
const POSTGRES_MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/postgres");

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

#[derive(Clone)]
pub enum DbPool {
    Postgres(Pool<ConnectionManager<PgConnection>>),
    Sqlite(Pool<ConnectionManager<SqliteConnection>>),
}

pub enum DbConnection {
    Postgres(PooledConnection<ConnectionManager<PgConnection>>),
    Sqlite(PooledConnection<ConnectionManager<SqliteConnection>>),
}

#[allow(dead_code)]
pub enum DbConnectionRef<'a> {
    Postgres(&'a mut PgConnection),
    Sqlite(&'a mut SqliteConnection),
}

impl DbPool {
    pub fn connect(database_url: &str, pool_size: u32) -> Result<Self, DatabaseInitError> {
        match database_kind(database_url) {
            DatabaseKind::Postgres => {
                init_postgres_pool(database_url, pool_size).map(Self::Postgres)
            }
            DatabaseKind::Sqlite => init_sqlite_pool(database_url, pool_size).map(Self::Sqlite),
        }
    }

    pub fn kind(&self) -> DatabaseKind {
        match self {
            Self::Postgres(_) => DatabaseKind::Postgres,
            Self::Sqlite(_) => DatabaseKind::Sqlite,
        }
    }

    pub fn get(&self) -> Result<DbConnection, DatabaseError> {
        match self {
            Self::Postgres(pool) => pool.get().map(DbConnection::Postgres).map_err(|source| {
                DatabaseError::PoolCheckout {
                    backend: "postgres",
                    message: source.to_string(),
                }
            }),
            Self::Sqlite(pool) => {
                pool.get()
                    .map(DbConnection::Sqlite)
                    .map_err(|source| DatabaseError::PoolCheckout {
                        backend: "sqlite",
                        message: source.to_string(),
                    })
            }
        }
    }

    #[allow(dead_code)]
    pub fn with_connection<T>(
        &self,
        operation: impl FnOnce(DbConnectionRef<'_>) -> Result<T, diesel::result::Error>,
    ) -> Result<T, DatabaseError> {
        let backend = self.kind().as_str();
        match self.get()? {
            DbConnection::Postgres(mut conn) => operation(DbConnectionRef::Postgres(&mut conn)),
            DbConnection::Sqlite(mut conn) => operation(DbConnectionRef::Sqlite(&mut conn)),
        }
        .map_err(|source| DatabaseError::Operation { backend, source })
    }

    #[allow(dead_code)]
    pub fn with_transaction<T>(
        &self,
        operation: impl FnOnce(DbConnectionRef<'_>) -> Result<T, diesel::result::Error>,
    ) -> Result<T, DatabaseError> {
        let backend = self.kind().as_str();
        match self.get()? {
            DbConnection::Postgres(mut conn) => conn
                .transaction(|conn| operation(DbConnectionRef::Postgres(conn)))
                .map_err(|source| DatabaseError::Operation { backend, source }),
            DbConnection::Sqlite(mut conn) => conn
                .transaction(|conn| operation(DbConnectionRef::Sqlite(conn)))
                .map_err(|source| DatabaseError::Operation { backend, source }),
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

pub fn check_readiness(pool: &DbPool) -> AppResult<DatabaseHealth> {
    let kind = pool.kind();
    let mut conn = pool.get().map_err(|source| AppError::Readiness {
        message: source.to_string(),
    })?;

    match &mut conn {
        DbConnection::Postgres(conn) => check_postgres_query(conn)?,
        DbConnection::Sqlite(conn) => check_sqlite_query(conn)?,
    }

    Ok(DatabaseHealth {
        kind,
        connected: true,
    })
}

fn check_postgres_query(conn: &mut PgConnection) -> AppResult<()> {
    let row = diesel::sql_query("SELECT 1 AS value")
        .get_result::<ReadyRow>(conn)
        .map_err(|source| AppError::Readiness {
            message: format!("postgres readiness query failed: {source}"),
        })?;

    ensure_ready_value(row, DatabaseKind::Postgres)
}

fn check_sqlite_query(conn: &mut SqliteConnection) -> AppResult<()> {
    let row = diesel::sql_query("SELECT 1 AS value")
        .get_result::<ReadyRow>(conn)
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

fn init_sqlite_pool(
    database_url: &str,
    pool_size: u32,
) -> Result<Pool<ConnectionManager<SqliteConnection>>, DatabaseInitError> {
    ensure_sqlite_database_file(database_url)?;

    let database_path = PathBuf::from(database_url);
    let mut conn = SqliteConnection::establish(database_url).map_err(|source| {
        DatabaseInitError::SqliteConnection {
            path: database_path.clone(),
            source,
        }
    })?;

    conn.run_pending_migrations(SQLITE_MIGRATIONS)
        .map_err(|source| DatabaseInitError::Migration {
            backend: "sqlite",
            message: source.to_string(),
        })?;

    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    Pool::builder()
        .max_size(pool_size.max(1))
        .test_on_check_out(true)
        .build(manager)
        .map_err(|source| DatabaseInitError::Pool {
            backend: "sqlite",
            message: source.to_string(),
        })
}

fn init_postgres_pool(
    database_url: &str,
    pool_size: u32,
) -> Result<Pool<ConnectionManager<PgConnection>>, DatabaseInitError> {
    let mut conn = PgConnection::establish(database_url)
        .map_err(|source| DatabaseInitError::PostgresConnection { source })?;

    conn.run_pending_migrations(POSTGRES_MIGRATIONS)
        .map_err(|source| DatabaseInitError::Migration {
            backend: "postgres",
            message: source.to_string(),
        })?;

    let manager = ConnectionManager::<PgConnection>::new(database_url);
    Pool::builder()
        .max_size(pool_size.max(1))
        .test_on_check_out(true)
        .build(manager)
        .map_err(|source| DatabaseInitError::Pool {
            backend: "postgres",
            message: source.to_string(),
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::sql_types::BigInt;

    #[derive(QueryableByName)]
    struct CountRow {
        #[diesel(sql_type = BigInt)]
        count: i64,
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

    #[test]
    fn sqlite_pool_creates_file_runs_migrations_and_checks_readiness() {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let database_url = temp_dir
            .path()
            .join("cyder-template.sqlite")
            .to_string_lossy()
            .into_owned();

        let pool = DbPool::connect(&database_url, 1).expect("sqlite pool should initialize");

        assert!(temp_dir.path().join("cyder-template.sqlite").is_file());
        assert_eq!(pool.kind(), DatabaseKind::Sqlite);

        let health = check_readiness(&pool).expect("readiness should pass");
        assert!(health.connected);

        let item_count = pool
            .with_connection(|conn| match conn {
                DbConnectionRef::Sqlite(conn) => {
                    diesel::sql_query("SELECT COUNT(*) AS count FROM items")
                        .get_result::<CountRow>(conn)
                        .map(|row| row.count)
                }
                DbConnectionRef::Postgres(_) => unreachable!("sqlite pool should use sqlite"),
            })
            .expect("items table should exist");
        assert_eq!(item_count, 0);

        let user_count = pool
            .with_connection(|conn| match conn {
                DbConnectionRef::Sqlite(conn) => {
                    diesel::sql_query("SELECT COUNT(*) AS count FROM users")
                        .get_result::<CountRow>(conn)
                        .map(|row| row.count)
                }
                DbConnectionRef::Postgres(_) => unreachable!("sqlite pool should use sqlite"),
            })
            .expect("users table should exist");
        assert_eq!(user_count, 0);
    }
}
