use std::time::{Duration, SystemTime, UNIX_EPOCH};

use diesel::{QueryableByName, sql_types::Text};
use diesel_async::RunQueryDsl;

use super::*;

#[derive(QueryableByName)]
struct JournalModeRow {
    #[diesel(sql_type = Text)]
    journal_mode: String,
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
#[ignore = "requires DEV_POSTGRES_TEST_URL pointing at an isolated PostgreSQL test database"]
async fn postgres_pool_with_multiple_connections_runs_crud_and_readiness() {
    let database_url = std::env::var("DEV_POSTGRES_TEST_URL")
        .expect("DEV_POSTGRES_TEST_URL must point at an isolated PostgreSQL test database");
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

async fn exercise_postgres_pool(pool: &DbPool, run_id: i64) -> HttpResult<()> {
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

async fn cleanup_postgres_test_records(pool: &DbPool, run_id: i64) -> HttpResult<()> {
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
