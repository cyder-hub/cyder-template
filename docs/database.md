# Database

The backend uses Diesel with `diesel_async` connection pooling and supports SQLite and PostgreSQL through separate schema and migration trees.

## SQLite

SQLite is the default development backend and requires no external service:

```bash
just dev-backend
```

The default database is `.app/dev/db/cyder-template.sqlite`. The default pool size is `1`. File-backed SQLite can use a larger YAML `database_pool_size`; every pooled connection enables WAL mode, foreign keys, and the configured `sqlite_busy_timeout_ms`.

SQLite still allows only one writer at a time. Plain `:memory:` databases require an effective pool size of `1` so migrations and requests use the same in-memory database.

## PostgreSQL

Select PostgreSQL with `APP_DATABASE_URL`:

```bash
APP_DATABASE_URL=postgres://cyder_template:cyder_template_dev@127.0.0.1:5432/cyder_template just dev-backend
```

The service detects the backend from the URL, defaults the PostgreSQL pool to `5`, and runs the matching embedded migrations at startup. `database_acquire_timeout_ms` controls how long readiness and request operations wait for a connection.

The Compose environment provides a local PostgreSQL service for development. Its fixed credentials and exposed port are not production defaults.

## Migrations and Schema

Migrations are split by backend:

```text
server/migrations/sqlite/
server/migrations/postgres/
```

Schema modules follow the same split:

```text
server/src/schema/sqlite.rs
server/src/schema/postgres.rs
```

Every persistent model change must update both backend migration trees and both schema modules. Keep the empty `.gitkeep` files when a migration directory has no migrations. See `server/diesel.toml` for the Diesel CLI commands used to regenerate schema output.

## PostgreSQL Integration Tests

PostgreSQL tests are opt-in because they require a disposable database:

```bash
DEV_POSTGRES_TEST_URL=postgres://cyder_template:cyder_template_dev@127.0.0.1:5432/cyder_template_test just test-postgres
```

Never point this setting at production, shared, or long-lived data. The Compose initialization script creates the test database only for a fresh PostgreSQL volume. Create it manually or recreate the local development volume when an existing volume predates the test database.

Without `DEV_POSTGRES_TEST_URL`, the ignored PostgreSQL test is not part of the default workspace test path.

## IDs at the JSON Boundary

Application-generated IDs use a 43-bit timestamp, 8-bit worker, and 12-bit sequence layout. Database and service types retain `i64`; HTTP DTOs serialize IDs as strings so browsers do not lose 64-bit integer precision.

Controller response DTOs should use `controller::api_id::ApiId` for ID fields. Path extractors can use `Path<ApiId>` and convert with `into_i64()` before entering service or database code. Frontend resource types should use `string` and send the same strings back in URLs.

The current strategy supports one application instance with one internal worker ID. Replace the ID and migration ownership strategy before horizontally scaling the service.
