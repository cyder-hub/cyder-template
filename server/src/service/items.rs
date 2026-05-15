#![allow(dead_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::{
    database::{self, DbPool},
    error::{AppError, AppResult},
    id::IdGenerator,
};

pub use database::items::Item;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateItemInput {
    pub title: String,
    pub description: Option<String>,
    pub completed: Option<bool>,
}

pub fn create(
    pool: &DbPool,
    id_generator: &IdGenerator,
    input: CreateItemInput,
) -> AppResult<Item> {
    let now = now_millis()?;
    let item = database::items::NewItem {
        id: id_generator.next_id()?,
        title: input.title,
        description: input.description.unwrap_or_default(),
        completed: input.completed.unwrap_or(false),
        created_at: now,
        updated_at: now,
    };

    database::items::create(pool, item).map_err(AppError::from)
}

pub fn list(pool: &DbPool) -> AppResult<Vec<Item>> {
    database::items::list(pool).map_err(AppError::from)
}

pub fn get(pool: &DbPool, item_id: i64) -> AppResult<Option<Item>> {
    database::items::get(pool, item_id).map_err(AppError::from)
}

pub fn delete(pool: &DbPool, item_id: i64) -> AppResult<bool> {
    database::items::delete(pool, item_id).map_err(AppError::from)
}

fn now_millis() -> AppResult<i64> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|source| AppError::Internal {
            message: format!("system clock is before unix epoch: {source}"),
        })?
        .as_millis();

    i64::try_from(millis).map_err(|_| AppError::Internal {
        message: "current timestamp exceeds signed 64-bit range".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{database::DbPool, id::IdGenerator};

    fn sqlite_pool() -> (tempfile::TempDir, DbPool) {
        let temp_dir = tempfile::tempdir().expect("temp dir should be created");
        let database_url = temp_dir
            .path()
            .join("items.sqlite")
            .to_string_lossy()
            .into_owned();
        let pool = DbPool::connect(&database_url, 1).expect("sqlite pool should initialize");
        (temp_dir, pool)
    }

    #[test]
    fn item_service_creates_lists_gets_and_deletes_items() {
        let (_temp_dir, pool) = sqlite_pool();
        let ids = IdGenerator::for_worker(2).expect("id generator should initialize");

        let created = create(
            &pool,
            &ids,
            CreateItemInput {
                title: "Write template".to_string(),
                description: Some("Add persistence layer".to_string()),
                completed: Some(true),
            },
        )
        .expect("item should be created");

        assert!(created.id > 0);
        assert_eq!(created.title, "Write template");
        assert_eq!(created.description, "Add persistence layer");
        assert!(created.completed);
        assert_eq!(created.created_at, created.updated_at);

        let listed = list(&pool).expect("items should list");
        assert_eq!(listed, vec![created.clone()]);

        let fetched = get(&pool, created.id).expect("item lookup should succeed");
        assert_eq!(fetched, Some(created.clone()));

        assert!(delete(&pool, created.id).expect("item should delete"));
        assert_eq!(
            get(&pool, created.id).expect("deleted item lookup should succeed"),
            None
        );
        assert!(!delete(&pool, created.id).expect("missing item delete should succeed"));
    }
}
