#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::database::{DatabaseError, DbConnection, DbPool};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

pub async fn create(pool: &DbPool, new_user: NewUser) -> Result<User, DatabaseError> {
    let mut conn = pool.get().await?;
    match &mut conn {
        DbConnection::Postgres(conn) => {
            postgres::create(conn, new_user)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "postgres",
                    source,
                })
        }
        DbConnection::Sqlite(conn) => {
            sqlite::create(conn, new_user)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "sqlite",
                    source,
                })
        }
    }
}

pub async fn list(pool: &DbPool) -> Result<Vec<User>, DatabaseError> {
    let mut conn = pool.get().await?;
    match &mut conn {
        DbConnection::Postgres(conn) => {
            postgres::list(conn)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "postgres",
                    source,
                })
        }
        DbConnection::Sqlite(conn) => {
            sqlite::list(conn)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "sqlite",
                    source,
                })
        }
    }
}

pub async fn get(pool: &DbPool, user_id: i64) -> Result<Option<User>, DatabaseError> {
    let mut conn = pool.get().await?;
    match &mut conn {
        DbConnection::Postgres(conn) => {
            postgres::get(conn, user_id)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "postgres",
                    source,
                })
        }
        DbConnection::Sqlite(conn) => {
            sqlite::get(conn, user_id)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "sqlite",
                    source,
                })
        }
    }
}

pub async fn delete(pool: &DbPool, user_id: i64) -> Result<bool, DatabaseError> {
    let mut conn = pool.get().await?;
    match &mut conn {
        DbConnection::Postgres(conn) => {
            postgres::delete(conn, user_id)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "postgres",
                    source,
                })
        }
        DbConnection::Sqlite(conn) => {
            sqlite::delete(conn, user_id)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "sqlite",
                    source,
                })
        }
    }
}

mod sqlite {
    use diesel::{OptionalExtension, prelude::*};
    use diesel_async::RunQueryDsl;

    use super::{NewUser, User};
    use crate::database::SqliteConnection;
    use crate::schema::sqlite::users;

    #[derive(Queryable, Selectable)]
    #[diesel(table_name = users)]
    struct UserRow {
        id: i64,
        name: String,
        email: String,
        active: bool,
        created_at: i64,
        updated_at: i64,
    }

    #[derive(Insertable)]
    #[diesel(table_name = users)]
    struct NewUserRow {
        id: i64,
        name: String,
        email: String,
        active: bool,
        created_at: i64,
        updated_at: i64,
    }

    impl From<UserRow> for User {
        fn from(row: UserRow) -> Self {
            Self {
                id: row.id,
                name: row.name,
                email: row.email,
                active: row.active,
                created_at: row.created_at,
                updated_at: row.updated_at,
            }
        }
    }

    impl From<NewUser> for NewUserRow {
        fn from(new_user: NewUser) -> Self {
            Self {
                id: new_user.id,
                name: new_user.name,
                email: new_user.email,
                active: new_user.active,
                created_at: new_user.created_at,
                updated_at: new_user.updated_at,
            }
        }
    }

    pub(super) async fn create(
        conn: &mut SqliteConnection,
        new_user: NewUser,
    ) -> QueryResult<User> {
        diesel::insert_into(users::table)
            .values(NewUserRow::from(new_user))
            .returning(UserRow::as_returning())
            .get_result(conn)
            .await
            .map(User::from)
    }

    pub(super) async fn list(conn: &mut SqliteConnection) -> QueryResult<Vec<User>> {
        users::table
            .order(users::created_at.desc())
            .then_order_by(users::id.desc())
            .select(UserRow::as_select())
            .load::<UserRow>(conn)
            .await
            .map(|rows| rows.into_iter().map(User::from).collect())
    }

    pub(super) async fn get(
        conn: &mut SqliteConnection,
        user_id: i64,
    ) -> QueryResult<Option<User>> {
        users::table
            .find(user_id)
            .select(UserRow::as_select())
            .first::<UserRow>(conn)
            .await
            .optional()
            .map(|row| row.map(User::from))
    }

    pub(super) async fn delete(conn: &mut SqliteConnection, user_id: i64) -> QueryResult<bool> {
        diesel::delete(users::table.find(user_id))
            .execute(conn)
            .await
            .map(|affected| affected > 0)
    }
}

mod postgres {
    use diesel::{OptionalExtension, prelude::*};
    use diesel_async::RunQueryDsl;

    use super::{NewUser, User};
    use crate::database::PostgresConnection;
    use crate::schema::postgres::users;

    #[derive(Queryable, Selectable)]
    #[diesel(table_name = users)]
    struct UserRow {
        id: i64,
        name: String,
        email: String,
        active: bool,
        created_at: i64,
        updated_at: i64,
    }

    #[derive(Insertable)]
    #[diesel(table_name = users)]
    struct NewUserRow {
        id: i64,
        name: String,
        email: String,
        active: bool,
        created_at: i64,
        updated_at: i64,
    }

    impl From<UserRow> for User {
        fn from(row: UserRow) -> Self {
            Self {
                id: row.id,
                name: row.name,
                email: row.email,
                active: row.active,
                created_at: row.created_at,
                updated_at: row.updated_at,
            }
        }
    }

    impl From<NewUser> for NewUserRow {
        fn from(new_user: NewUser) -> Self {
            Self {
                id: new_user.id,
                name: new_user.name,
                email: new_user.email,
                active: new_user.active,
                created_at: new_user.created_at,
                updated_at: new_user.updated_at,
            }
        }
    }

    pub(super) async fn create(
        conn: &mut PostgresConnection,
        new_user: NewUser,
    ) -> QueryResult<User> {
        diesel::insert_into(users::table)
            .values(NewUserRow::from(new_user))
            .returning(UserRow::as_returning())
            .get_result(conn)
            .await
            .map(User::from)
    }

    pub(super) async fn list(conn: &mut PostgresConnection) -> QueryResult<Vec<User>> {
        users::table
            .order(users::created_at.desc())
            .then_order_by(users::id.desc())
            .select(UserRow::as_select())
            .load::<UserRow>(conn)
            .await
            .map(|rows| rows.into_iter().map(User::from).collect())
    }

    pub(super) async fn get(
        conn: &mut PostgresConnection,
        user_id: i64,
    ) -> QueryResult<Option<User>> {
        users::table
            .find(user_id)
            .select(UserRow::as_select())
            .first::<UserRow>(conn)
            .await
            .optional()
            .map(|row| row.map(User::from))
    }

    pub(super) async fn delete(conn: &mut PostgresConnection, user_id: i64) -> QueryResult<bool> {
        diesel::delete(users::table.find(user_id))
            .execute(conn)
            .await
            .map(|affected| affected > 0)
    }
}
