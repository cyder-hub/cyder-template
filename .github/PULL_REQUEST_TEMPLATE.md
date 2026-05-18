## Summary

-

## Verification

Commands run:

- [ ] `cargo fmt --check`
- [ ] `cargo check --workspace`
- [ ] `cargo test --workspace`
- [ ] `APP_TEST_POSTGRES_URL=... just test-postgres` (database changes that affect PostgreSQL)
- [ ] `npm --prefix front ci`
- [ ] `npm --prefix front test`
- [ ] `npm --prefix front run build`
- [ ] `docker compose -f docker-compose.yml config`
- [ ] `docker build -t cyder-template:ci -f Dockerfile .`
- [ ] Not run; reason:

## Impact

- [ ] Backend Rust code
- [ ] Frontend Vue code
- [ ] Database migrations or schema
- [ ] Docker or compose
- [ ] GitHub Actions or repository metadata
- [ ] Documentation only
- [ ] Template naming or rename guidance
- [ ] Dependencies or lockfiles

## Checklist

- [ ] I did not commit `front/node_modules/`, `front/dist/`, `target/`, `.app/`, `.env`, local databases, logs, or credentials.
- [ ] I updated README or contributing guidance where behavior, commands, naming, or dependencies changed.
- [ ] I listed SQLite and PostgreSQL verification results for database behavior changes, or explained why a backend was not run.
- [ ] I avoided adding claims for unimplemented features.
