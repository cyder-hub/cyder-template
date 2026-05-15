#![allow(dead_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::{
    database::{self, DbPool},
    error::{AppError, AppResult},
    id::IdGenerator,
};

pub use database::users::User;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateUserInput {
    pub name: String,
    pub email: String,
    pub active: Option<bool>,
}

pub fn create(
    pool: &DbPool,
    id_generator: &IdGenerator,
    input: CreateUserInput,
) -> AppResult<User> {
    let now = now_millis()?;
    let user = database::users::NewUser {
        id: id_generator.next_id()?,
        name: input.name,
        email: input.email,
        active: input.active.unwrap_or(true),
        created_at: now,
        updated_at: now,
    };

    database::users::create(pool, user).map_err(AppError::from)
}

pub fn list(pool: &DbPool) -> AppResult<Vec<User>> {
    database::users::list(pool).map_err(AppError::from)
}

pub fn get(pool: &DbPool, user_id: i64) -> AppResult<Option<User>> {
    database::users::get(pool, user_id).map_err(AppError::from)
}

pub fn delete(pool: &DbPool, user_id: i64) -> AppResult<bool> {
    database::users::delete(pool, user_id).map_err(AppError::from)
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
            .join("users.sqlite")
            .to_string_lossy()
            .into_owned();
        let pool = DbPool::connect(&database_url, 1).expect("sqlite pool should initialize");
        (temp_dir, pool)
    }

    #[test]
    fn user_service_creates_lists_gets_and_deletes_users() {
        let (_temp_dir, pool) = sqlite_pool();
        let ids = IdGenerator::for_worker(3).expect("id generator should initialize");

        let created = create(
            &pool,
            &ids,
            CreateUserInput {
                name: "Example User".to_string(),
                email: "user@example.com".to_string(),
                active: Some(false),
            },
        )
        .expect("user should be created");

        assert!(created.id > 0);
        assert_eq!(created.name, "Example User");
        assert_eq!(created.email, "user@example.com");
        assert!(!created.active);
        assert_eq!(created.created_at, created.updated_at);

        let listed = list(&pool).expect("users should list");
        assert_eq!(listed, vec![created.clone()]);

        let fetched = get(&pool, created.id).expect("user lookup should succeed");
        assert_eq!(fetched, Some(created.clone()));

        assert!(delete(&pool, created.id).expect("user should delete"));
        assert_eq!(
            get(&pool, created.id).expect("deleted user lookup should succeed"),
            None
        );
        assert!(!delete(&pool, created.id).expect("missing user delete should succeed"));
    }
}
