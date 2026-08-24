#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::database::{DatabaseError, DbConnection, DbPool};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub completed: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub completed: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

pub async fn create(pool: &DbPool, new_item: NewItem) -> Result<Item, DatabaseError> {
    let mut conn = pool.get().await?;
    match &mut conn {
        DbConnection::Postgres(conn) => {
            postgres::create(conn, new_item)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "postgres",
                    source,
                })
        }
        DbConnection::Sqlite(conn) => {
            sqlite::create(conn, new_item)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "sqlite",
                    source,
                })
        }
    }
}

pub async fn list(pool: &DbPool) -> Result<Vec<Item>, DatabaseError> {
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

pub async fn get(pool: &DbPool, item_id: i64) -> Result<Option<Item>, DatabaseError> {
    let mut conn = pool.get().await?;
    match &mut conn {
        DbConnection::Postgres(conn) => {
            postgres::get(conn, item_id)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "postgres",
                    source,
                })
        }
        DbConnection::Sqlite(conn) => {
            sqlite::get(conn, item_id)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "sqlite",
                    source,
                })
        }
    }
}

pub async fn delete(pool: &DbPool, item_id: i64) -> Result<bool, DatabaseError> {
    let mut conn = pool.get().await?;
    match &mut conn {
        DbConnection::Postgres(conn) => {
            postgres::delete(conn, item_id)
                .await
                .map_err(|source| DatabaseError::Operation {
                    backend: "postgres",
                    source,
                })
        }
        DbConnection::Sqlite(conn) => {
            sqlite::delete(conn, item_id)
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

    use super::{Item, NewItem};
    use crate::database::SqliteConnection;
    use crate::schema::sqlite::items;

    #[derive(Queryable, Selectable)]
    #[diesel(table_name = items)]
    struct ItemRow {
        id: i64,
        title: String,
        description: String,
        completed: bool,
        created_at: i64,
        updated_at: i64,
    }

    #[derive(Insertable)]
    #[diesel(table_name = items)]
    struct NewItemRow {
        id: i64,
        title: String,
        description: String,
        completed: bool,
        created_at: i64,
        updated_at: i64,
    }

    impl From<ItemRow> for Item {
        fn from(row: ItemRow) -> Self {
            Self {
                id: row.id,
                title: row.title,
                description: row.description,
                completed: row.completed,
                created_at: row.created_at,
                updated_at: row.updated_at,
            }
        }
    }

    impl From<NewItem> for NewItemRow {
        fn from(new_item: NewItem) -> Self {
            Self {
                id: new_item.id,
                title: new_item.title,
                description: new_item.description,
                completed: new_item.completed,
                created_at: new_item.created_at,
                updated_at: new_item.updated_at,
            }
        }
    }

    pub(super) async fn create(
        conn: &mut SqliteConnection,
        new_item: NewItem,
    ) -> QueryResult<Item> {
        diesel::insert_into(items::table)
            .values(NewItemRow::from(new_item))
            .returning(ItemRow::as_returning())
            .get_result(conn)
            .await
            .map(Item::from)
    }

    pub(super) async fn list(conn: &mut SqliteConnection) -> QueryResult<Vec<Item>> {
        items::table
            .order(items::created_at.desc())
            .then_order_by(items::id.desc())
            .select(ItemRow::as_select())
            .load::<ItemRow>(conn)
            .await
            .map(|rows| rows.into_iter().map(Item::from).collect())
    }

    pub(super) async fn get(
        conn: &mut SqliteConnection,
        item_id: i64,
    ) -> QueryResult<Option<Item>> {
        items::table
            .find(item_id)
            .select(ItemRow::as_select())
            .first::<ItemRow>(conn)
            .await
            .optional()
            .map(|row| row.map(Item::from))
    }

    pub(super) async fn delete(conn: &mut SqliteConnection, item_id: i64) -> QueryResult<bool> {
        diesel::delete(items::table.find(item_id))
            .execute(conn)
            .await
            .map(|affected| affected > 0)
    }
}

mod postgres {
    use diesel::{OptionalExtension, prelude::*};
    use diesel_async::RunQueryDsl;

    use super::{Item, NewItem};
    use crate::database::PostgresConnection;
    use crate::schema::postgres::items;

    #[derive(Queryable, Selectable)]
    #[diesel(table_name = items)]
    struct ItemRow {
        id: i64,
        title: String,
        description: String,
        completed: bool,
        created_at: i64,
        updated_at: i64,
    }

    #[derive(Insertable)]
    #[diesel(table_name = items)]
    struct NewItemRow {
        id: i64,
        title: String,
        description: String,
        completed: bool,
        created_at: i64,
        updated_at: i64,
    }

    impl From<ItemRow> for Item {
        fn from(row: ItemRow) -> Self {
            Self {
                id: row.id,
                title: row.title,
                description: row.description,
                completed: row.completed,
                created_at: row.created_at,
                updated_at: row.updated_at,
            }
        }
    }

    impl From<NewItem> for NewItemRow {
        fn from(new_item: NewItem) -> Self {
            Self {
                id: new_item.id,
                title: new_item.title,
                description: new_item.description,
                completed: new_item.completed,
                created_at: new_item.created_at,
                updated_at: new_item.updated_at,
            }
        }
    }

    pub(super) async fn create(
        conn: &mut PostgresConnection,
        new_item: NewItem,
    ) -> QueryResult<Item> {
        diesel::insert_into(items::table)
            .values(NewItemRow::from(new_item))
            .returning(ItemRow::as_returning())
            .get_result(conn)
            .await
            .map(Item::from)
    }

    pub(super) async fn list(conn: &mut PostgresConnection) -> QueryResult<Vec<Item>> {
        items::table
            .order(items::created_at.desc())
            .then_order_by(items::id.desc())
            .select(ItemRow::as_select())
            .load::<ItemRow>(conn)
            .await
            .map(|rows| rows.into_iter().map(Item::from).collect())
    }

    pub(super) async fn get(
        conn: &mut PostgresConnection,
        item_id: i64,
    ) -> QueryResult<Option<Item>> {
        items::table
            .find(item_id)
            .select(ItemRow::as_select())
            .first::<ItemRow>(conn)
            .await
            .optional()
            .map(|row| row.map(Item::from))
    }

    pub(super) async fn delete(conn: &mut PostgresConnection, item_id: i64) -> QueryResult<bool> {
        diesel::delete(items::table.find(item_id))
            .execute(conn)
            .await
            .map(|affected| affected > 0)
    }
}
